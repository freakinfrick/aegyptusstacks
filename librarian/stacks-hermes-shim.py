#!/usr/bin/env python3
"""stacks-hermes-shim — FIFO bridge for tui_gateway with tmux debug output.

Reads JSON-RPC requests from /tmp/stacks-hermes.in (named FIFO),
forwards them to tui_gateway.server.handle_request(), and writes
JSON-RPC responses/events to /tmp/stacks-hermes.out (named FIFO).

All traffic is printed to stderr (visible in tmux pane):
  -> for requests, <- for responses.

Requires hermes-agent (https://github.com/NousResearch/hermes-agent) to be
installed. Set HERMES_AGENT_PATH if it is not already on sys.path.
Your LLM backend must be running before launching this shim.
"""

import json
import os
import signal
import subprocess
import sys
import threading
from pathlib import Path

FIFO_IN = "/tmp/stacks-hermes.in"
FIFO_OUT = "/tmp/stacks-hermes.out"
SHIM_LOG = "/tmp/stacks-hermes-shim.log"
SYSTEM_PROMPT_FILE = Path(__file__).parent / "system_prompt.md"
MAX_IDENTICAL_CALLS = 2  # hard cap on identical terminal commands per session

# Tee stderr to a log file for post-mortem when tmux session dies
_log_file = None

def _log(msg):
    """Write to stderr AND log file."""
    print(msg, file=sys.stderr, flush=True)
    if _log_file:
        _log_file.write(msg + "\n")
        _log_file.flush()


def _install_dedup_guard():
    """Monkey-patch terminal_tool to reject identical consecutive commands."""
    import tools.terminal_tool as tt
    if getattr(tt, "_dedup_installed", False):
        return
    _original = tt.terminal_tool
    _recent = []  # list of (command, exit_code) tuples
    _recent_lock = threading.Lock()

    def _guarded(command: str, **kwargs):
        with _recent_lock:
            # Count how many times this exact command has run
            count = sum(1 for cmd, _ in _recent if cmd == command)
            if count >= MAX_IDENTICAL_CALLS:
                msg = (f"BLOCKED: This exact command has already been run {count} times "
                       f"with the same result. Try a different approach or respond to the visitor.")
                _log(f"[dedup] blocked: {command}")
                return json.dumps({"output": msg, "exit_code": 1})
        result = _original(command, **kwargs)
        # Track the command and its exit code
        try:
            ec = json.loads(result).get("exit_code", 0)
        except Exception:
            ec = 0
        with _recent_lock:
            _recent.append((command, ec))
            # Keep window bounded
            if len(_recent) > 20:
                _recent.pop(0)
        return result

    tt.terminal_tool = _guarded
    tt._dedup_installed = True
    _log("[dedup] terminal tool dedup guard installed")


MAX_FIFO_REOPEN = 5  # exit after this many consecutive FIFO reopen failures

_shutdown_requested = False

def _sigint_handler(signum, frame):
    """Graceful shutdown on SIGINT — set flag so main loop exits."""
    global _shutdown_requested
    _shutdown_requested = True
    _log("[signal] SIGINT received, shutting down gracefully...")


def main():
    global _log_file
    _log_file = open(SHIM_LOG, "w")

    # ── Signals ──
    signal.signal(signal.SIGPIPE, signal.SIG_IGN)
    signal.signal(signal.SIGINT, _sigint_handler)

    # ── Open FIFOs FIRST — unblocks Rust's blocking read/write immediately ──
    # Must happen before tui_gateway import, which can be slow.
    # Without this, Rust's 90s FIFO-open timeout fires during cold model loads.
    # The Rust side has a separate 90s GatewayReady timeout for the slow work.
    _log("Opening FIFOs...")
    fifo_out = open(FIFO_OUT, "w", buffering=1)
    fifo_in_fd = open(FIFO_IN, "r")
    _log("FIFOs connected")

    # ── Import tui_gateway from hermes-agent ──
    _log("Importing tui_gateway...")
    hermes_path = os.environ.get("HERMES_AGENT_PATH", "")
    if hermes_path:
        sys.path.insert(0, hermes_path)
    import tui_gateway.server as srv
    from tui_gateway.server import handle_request, resolve_skin
    _log("tui_gateway imported")

    # ── Thread-safe write_json replacement ──
    _lock = srv._stdout_lock

    def write_json(obj):
        line = json.dumps(obj, ensure_ascii=False) + "\n"
        print(f"<- {line.rstrip()}", file=sys.stderr, flush=True)
        try:
            with _lock:
                fifo_out.write(line)
                fifo_out.flush()
            return True
        except BrokenPipeError:
            return False

    # ── Monkey-patch the gateway ──
    srv.write_json = write_json
    srv._real_stdout = fifo_out

    # ── Send gateway.ready event ──
    if not write_json({
        "jsonrpc": "2.0",
        "method": "event",
        "params": {"type": "gateway.ready", "payload": {"skin": resolve_skin()}},
    }):
        sys.exit(0)

    _log("\U0001f3db  Librarian gateway ready")

    # ── Load system prompt for injection after session.create ──
    _system_prompt = ""
    if SYSTEM_PROMPT_FILE.exists():
        _system_prompt = SYSTEM_PROMPT_FILE.read_text(encoding="utf-8").strip()
        print(f"System prompt loaded ({len(_system_prompt)} chars)", file=sys.stderr, flush=True)

    # ── Read requests from input FIFO ──
    # Re-open loop: if the writer disconnects (game crash/restart), the FIFO
    # yields EOF. Re-open it so the restarted game can reconnect without
    # needing to kill the tmux session.
    # First pass uses pre-opened fd; subsequent passes re-open for reconnect.
    fifo_in = fifo_in_fd
    reopen_failures = 0
    while not _shutdown_requested:
        try:
            for raw in fifo_in:
                if _shutdown_requested:
                    break
                line = raw.strip()
                if not line:
                    continue
                print(f"-> {line}", file=sys.stderr, flush=True)
                try:
                    req = json.loads(line)
                except json.JSONDecodeError:
                    if not write_json({
                        "jsonrpc": "2.0",
                        "error": {"code": -32700, "message": "parse error"},
                        "id": None,
                    }):
                        sys.exit(0)
                    continue

                try:
                    resp = handle_request(req)
                except Exception as exc:
                    import traceback
                    traceback.print_exc(file=sys.stderr)
                    resp = {
                        "jsonrpc": "2.0",
                        "error": {"code": -32603, "message": str(exc)},
                        "id": req.get("id"),
                    }

                if resp is not None:
                    if not write_json(resp):
                        sys.exit(0)

                # After session.create or session.resume, inject system prompt
                if (req.get("method") in ("session.create", "session.resume")
                        and resp and "result" in resp
                        and _system_prompt):
                    try:
                        sid = resp["result"].get("session_id", "")
                        method = req["method"]
                        print(f"[shim] {method} done, sid={sid}", file=sys.stderr, flush=True)
                        session = srv._sessions.get(sid)
                        if session:
                            ready = session.get("agent_ready")
                            if ready:
                                print("[shim] waiting for agent_ready...", file=sys.stderr, flush=True)
                                ready.wait(timeout=60)
                                print("[shim] agent_ready signalled", file=sys.stderr, flush=True)
                            agent = session.get("agent")
                            err = session.get("agent_error")
                            print(f"[shim] agent={type(agent).__name__ if agent else None}, error={err}", file=sys.stderr, flush=True)
                            if agent is not None:
                                agent.ephemeral_system_prompt = _system_prompt
                                agent._cached_system_prompt = None
                                # Restrict to terminal tool only
                                agent.enabled_toolsets = ["terminal"]
                                # Patch terminal tool with dedup guard
                                _install_dedup_guard()
                                print(f"Injected system prompt + tool restriction + dedup guard on session {sid}", file=sys.stderr, flush=True)
                            else:
                                print(f"Agent unavailable: {err}", file=sys.stderr, flush=True)
                        else:
                            print(f"[shim] session {sid} not found in _sessions!", file=sys.stderr, flush=True)
                    except BaseException as exc:
                        import traceback
                        print(f"System prompt injection failed: {type(exc).__name__}: {exc}", file=sys.stderr, flush=True)
                        traceback.print_exc(file=sys.stderr)

            if _shutdown_requested:
                break

            # Writer disconnected (EOF) — re-open for reconnect
            fifo_in.close()
            print("FIFO writer disconnected, waiting for reconnect...", file=sys.stderr, flush=True)
            try:
                fifo_in = open(FIFO_IN, "r")
                reopen_failures = 0  # reset on successful reopen
            except OSError as e:
                reopen_failures += 1
                _log(f"FIFO reopen failed ({reopen_failures}/{MAX_FIFO_REOPEN}): {e}")
                if reopen_failures >= MAX_FIFO_REOPEN:
                    _log(f"Max FIFO reopen failures reached ({MAX_FIFO_REOPEN}), exiting.")
                    sys.exit(1)
                import time
                time.sleep(1)
        except Exception as e:
            import traceback
            print(f"FIFO loop error: {e}", file=sys.stderr, flush=True)
            traceback.print_exc(file=sys.stderr)
            reopen_failures += 1
            if reopen_failures >= MAX_FIFO_REOPEN:
                _log(f"Max FIFO loop failures reached ({MAX_FIFO_REOPEN}), exiting.")
                sys.exit(1)
            import time
            time.sleep(1)

    _log("Shutdown complete.")
    sys.exit(0)


if __name__ == "__main__":
    main()
