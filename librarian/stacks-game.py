#!/usr/bin/env python3
"""stacks-game — Game command bridge for the Librarian agent.

Called as a terminal tool by the Hermes agent. Connects to the game's
Unix domain socket, sends a command, reads the ack.

Usage:
    python3 stacks-game.py navigate Q,R,Z
    python3 stacks-game.py read CANONICAL_ID
    python3 stacks-game.py gather ID1 [ID2 ID3 ...]
    python3 stacks-game.py scribe TOPIC ID1 [ID2 ...]
"""

import argparse
import json
import socket
import sys

SOCK_PATH = "/tmp/stacks-game.sock"
TIMEOUT = 10


def send_command(cmd: dict) -> dict:
    """Connect to game socket, send command, return ack."""
    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    sock.settimeout(TIMEOUT)
    sock.connect(SOCK_PATH)
    try:
        payload = json.dumps(cmd, ensure_ascii=False) + "\n"
        sock.sendall(payload.encode("utf-8"))
        data = b""
        while True:
            chunk = sock.recv(4096)
            if not chunk:
                break
            data += chunk
            # Try to parse — complete JSON means we're done
            try:
                return json.loads(data.decode("utf-8").strip())
            except (json.JSONDecodeError, UnicodeDecodeError):
                continue
        return json.loads(data.decode("utf-8").strip())
    finally:
        sock.close()


def _fix_negative_coords(argv):
    """Insert '--' before negative-looking coordinates so argparse doesn't
    treat them as option flags.  e.g. ['navigate', '-16,16,-7'] becomes
    ['navigate', '--', '-16,16,-7']."""
    if "--" in argv:
        return argv
    fixed = list(argv)
    for i, arg in enumerate(fixed):
        if i > 0 and arg.startswith("-") and "," in arg:
            fixed.insert(i, "--")
            break
    return fixed


def main():
    parser = argparse.ArgumentParser(description="Game command bridge")
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_nav = sub.add_parser("navigate", help="Teleport to room")
    p_nav.add_argument("coord", help="Q,R,Z coordinates")

    p_read = sub.add_parser("read", help="Open text on lectern")
    p_read.add_argument("id", help="Canonical ID")

    p_gather = sub.add_parser("gather", help="Gather texts to collection")
    p_gather.add_argument("ids", nargs="+", help="Canonical IDs")

    p_scribe = sub.add_parser("scribe", help="Send texts to the Scribe")
    p_scribe.add_argument("topic", help="Article topic")
    p_scribe.add_argument("volumes", nargs="*", help="Canonical IDs")

    args = parser.parse_args(_fix_negative_coords(sys.argv[1:]))

    if args.cmd == "navigate":
        parts = args.coord.split(",")
        if len(parts) != 3:
            print("fail: coord must be Q,R,Z", file=sys.stderr)
            sys.exit(1)
        coord = [int(p.strip()) for p in parts]
        cmd = {"cmd": "navigate", "coord": coord}

    elif args.cmd == "read":
        cmd = {"cmd": "read", "id": args.id}

    elif args.cmd == "gather":
        if len(args.ids) == 1:
            cmd = {"cmd": "gather", "id": args.ids[0]}
        else:
            cmd = {"cmd": "gather", "ids": args.ids}

    elif args.cmd == "scribe":
        cmd = {"cmd": "scribe", "topic": args.topic, "volumes": args.volumes}

    else:
        parser.print_help()
        sys.exit(1)

    try:
        ack = send_command(cmd)
    except FileNotFoundError:
        print("fail: game not running")
        sys.exit(1)
    except ConnectionRefusedError:
        print("fail: game not running")
        sys.exit(1)
    except socket.timeout:
        print("fail: timeout")
        sys.exit(1)
    except Exception as e:
        print(f"fail: {e}")
        sys.exit(1)

    status = ack.get("status", "unknown")
    reason = ack.get("reason", "")
    if status == "ok":
        print(f"ok: {reason}" if reason else "ok")
    else:
        print(f"fail: {reason}" if reason else "fail")
        sys.exit(1)


if __name__ == "__main__":
    main()
