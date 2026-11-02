#!/usr/bin/env python3
"""BFS command-line interface for The Stacks."""
import json
import os
import socket

import click

BFS_URL = os.environ.get("BFS_URL", "http://localhost:8081/rpc")


def send_rpc(method: str, params: dict, socket_path: str = "/tmp/bfs.sock") -> dict:
    """Send a JSON-RPC 2.0 request to the BFS server."""
    request = {"jsonrpc": "2.0", "method": method, "params": params, "id": 1}
    try:
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        sock.connect(socket_path)
        sock.sendall(json.dumps(request).encode() + b"\n")
        response = b""
        while True:
            chunk = sock.recv(4096)
            if not chunk:
                break
            response += chunk
            # Try to parse — complete JSON means we're done
            try:
                result = json.loads(response)
                sock.close()
                return result
            except (json.JSONDecodeError, UnicodeDecodeError):
                continue
        sock.close()
        return json.loads(response)
    except (ConnectionRefusedError, FileNotFoundError):
        import httpx

        resp = httpx.post(BFS_URL, json=request)
        return resp.json()


@click.group()
def cli():
    """BFS command-line interface for The Stacks."""


@cli.command()
@click.option("--query", required=True, help="Search query")
@click.option(
    "--type",
    "search_type",
    default="lexical",
    type=click.Choice(["semantic", "lexical", "metadata"]),
)
@click.option("--limit", default=10, help="Max results")
@click.option("--json", "as_json", is_flag=True, help="JSON output")
def search(query: str, search_type: str, limit: int, as_json: bool) -> None:
    """Search the library."""
    result = send_rpc(
        "search", {"query": query, "type": search_type, "limit": limit}
    )
    if as_json:
        click.echo(json.dumps(result.get("result", []), indent=2))
    else:
        hits = result.get("result", [])
        if not hits:
            click.echo("No results found.")
            return
        for hit in hits:
            name = hit.get("conventional_name") or hit.get("canonical_id", "unknown")
            click.echo(f"  {name}")
            if hit.get("district"):
                click.echo(
                    f"    District: {hit['district']}, z={hit.get('z_band', '?')}"
                )
            click.echo()


@cli.command()
@click.argument("action", type=click.Choice(["walk", "teleport", "escort"]))
@click.option(
    "--to", "destination", required=True, help="Destination (q,r,z or landmark name)"
)
@click.option("--session", required=True, help="Session ID")
@click.option("--json", "as_json", is_flag=True)
def navigate(action: str, destination: str, session: str, as_json: bool) -> None:
    """Navigate the library."""
    try:
        parts = [int(x.strip()) for x in destination.split(",")]
        params = {
            "action": action,
            "session_id": session,
            "q": parts[0],
            "r": parts[1],
            "z": parts[2],
        }
    except (ValueError, IndexError):
        params = {"action": action, "session_id": session, "landmark": destination}
    result = send_rpc("navigate", params)
    output = result.get("result", result)
    if as_json:
        click.echo(json.dumps(output, indent=2))
    else:
        click.echo(f"Navigated via {action}.")


@cli.command()
@click.option("--room", help="Room coordinates as q,r,z")
@click.option("--district", help="District name")
@click.option("--volume", help="Volume canonical_id")
@click.option("--json", "as_json", is_flag=True)
def describe(
    room: str | None, district: str | None, volume: str | None, as_json: bool
) -> None:
    """Describe a room, district, or volume."""
    if room:
        parts = [int(x) for x in room.split(",")]
        result = send_rpc("get_room", {"q": parts[0], "r": parts[1], "z": parts[2]})
    elif district:
        result = send_rpc("district_info", {"district": district})
    elif volume:
        result = send_rpc("get_volume", {"canonical_id": volume})
    else:
        click.echo("Specify --room, --district, or --volume")
        return
    output = result.get("result", result)
    click.echo(json.dumps(output, indent=2))


@cli.command()
@click.option("--session", required=True)
@click.option("--canonical-id", required=True)
@click.option("--json", "as_json", is_flag=True)
def gather(session: str, canonical_id: str, as_json: bool) -> None:
    """Gather a volume to workspace."""
    result = send_rpc(
        "gather", {"session_id": session, "canonical_id": canonical_id}
    )
    output = result.get("result", result)
    if as_json:
        click.echo(json.dumps(output, indent=2))
    else:
        click.echo(f"Gathered {canonical_id} to workspace.")


@cli.command("read")
@click.option("--canonical-id", required=True)
@click.option("--json", "as_json", is_flag=True)
def read_volume(canonical_id: str, as_json: bool) -> None:
    """Read a volume's content."""
    result = send_rpc("get_volume", {"canonical_id": canonical_id})
    output = result.get("result", result)
    click.echo(json.dumps(output, indent=2))


@cli.command()
@click.option("--topic", required=True)
@click.option("--session", required=True)
@click.option("--json", "as_json", is_flag=True)
def article(topic: str, session: str, as_json: bool) -> None:
    """Write article from workspace sources."""
    result = send_rpc("article", {"topic": topic, "session_id": session})
    output = result.get("result", result)
    if as_json:
        click.echo(json.dumps(output, indent=2))
    else:
        click.echo(output)


@cli.command()
def status() -> None:
    """Check BFS service health."""
    try:
        result = send_rpc("status", {})
        click.echo("BFS Service: UP")
        data = result.get("result", {})
        if isinstance(data, dict):
            for k, v in data.items():
                click.echo(f"  {k}: {v}")
    except Exception as e:
        click.echo(f"BFS Service: DOWN ({e})")


if __name__ == "__main__":
    cli()
