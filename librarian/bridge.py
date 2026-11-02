"""Librarian Bridge — connects game client to hermes-agent with context injection."""

from __future__ import annotations

import asyncio
import json
import os
from pathlib import Path
from typing import Any

from .context import build_context


class LibrarianBridge:
    """Thin relay between Godot (or CLI) and hermes-agent.

    Listens on a Unix socket for JSON-RPC messages, injects BFS location
    context into each user message, forwards to the hermes-agent process,
    parses spatial commands from the response, and returns everything to
    the caller.
    """

    def __init__(
        self,
        bfs_socket: str | None = None,
        librarian_socket: str | None = None,
    ):
        _runtime_dir = os.environ.get("XDG_RUNTIME_DIR", "/tmp")
        self.bfs_socket = bfs_socket or f"{_runtime_dir}/bfs.sock"
        self.librarian_socket = librarian_socket or f"{_runtime_dir}/librarian.sock"

    # ------------------------------------------------------------------
    # Public interface
    # ------------------------------------------------------------------

    async def handle_message(self, user_msg: str, session_id: str) -> dict[str, Any]:
        """Process user message: inject context, send to agent, parse response."""

        # 1. Get current session/room from BFS
        context = await self._get_context(session_id)

        # 2. Build augmented message with context header and role separation
        augmented = (
            f"<system_context>{context}</system_context>\n"
            f"<user_message>{user_msg}</user_message>"
        )

        # 3. Send to hermes-agent
        response_text = await self._send_to_agent(augmented)

        # 4. Parse for spatial commands embedded in response
        text, commands = self._parse_response(response_text)

        return {"text": text, "commands": commands}

    # ------------------------------------------------------------------
    # Server lifecycle
    # ------------------------------------------------------------------

    async def serve(self) -> None:
        """Start the Unix socket server for Godot connections."""
        sock_path = Path(self.librarian_socket)
        if sock_path.exists():
            sock_path.unlink()

        server = await asyncio.start_unix_server(
            self._handle_connection, path=str(sock_path)
        )
        async with server:
            await server.serve_forever()

    async def _handle_connection(
        self,
        reader: asyncio.StreamReader,
        writer: asyncio.StreamWriter,
    ) -> None:
        """Handle a single JSON-RPC connection from Godot."""
        try:
            while True:
                raw = await reader.readline()
                if not raw:
                    break
                request = json.loads(raw.decode())
                params = request.get("params", {})
                result = await self.handle_message(
                    params.get("message", ""),
                    params.get("session_id", "default"),
                )
                response = json.dumps({"id": request.get("id"), "result": result})
                writer.write(response.encode() + b"\n")
                await writer.drain()
        finally:
            writer.close()
            await writer.wait_closed()

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

    async def _get_context(self, session_id: str) -> str:
        """Query BFS for current location context via Unix socket."""
        request = json.dumps({
            "method": "get_context",
            "params": {"session_id": session_id},
        })
        response = await self._bfs_request(request)

        session = response.get("session", {})
        room = response.get("room", {})
        return build_context(session, room)

    async def _bfs_request(self, request: str) -> dict[str, Any]:
        """Send a JSON-RPC request to the BFS service socket."""
        reader, writer = await asyncio.open_unix_connection(self.bfs_socket)
        try:
            writer.write(request.encode() + b"\n")
            await writer.drain()
            raw = await reader.readline()
            return json.loads(raw.decode()) if raw else {}
        finally:
            writer.close()
            await writer.wait_closed()

    async def _send_to_agent(self, message: str) -> str:
        """Send message to hermes-agent and return its response.

        In production this pipes to the hermes-agent subprocess or calls
        its local HTTP API. Placeholder until hermes-agent integration.
        """
        # TODO: wire to hermes-agent process/API
        return ""

    def _parse_response(self, response: str) -> tuple[str, list[dict[str, Any]]]:
        """Extract dialogue text and spatial commands from agent output.

        The agent may embed JSON command blocks delimited by ```json ... ```
        within its response. These are extracted as spatial commands (navigate,
        gather, etc.) and forwarded to Godot for execution.
        """
        if not response:
            return "", []

        commands: list[dict[str, Any]] = []
        text_parts: list[str] = []
        in_json_block = False
        json_lines: list[str] = []

        for line in response.split("\n"):
            if line.strip().startswith("```json"):
                in_json_block = True
                json_lines = []
            elif line.strip() == "```" and in_json_block:
                in_json_block = False
                try:
                    cmd = json.loads("\n".join(json_lines))
                    if isinstance(cmd, dict) and "type" in cmd:
                        commands.append(cmd)
                except json.JSONDecodeError:
                    pass
            elif in_json_block:
                json_lines.append(line)
            else:
                text_parts.append(line)

        return "\n".join(text_parts).strip(), commands
