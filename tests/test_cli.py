"""Tests for bfs-cli.py — verify CLI structure and help output."""
import subprocess
import sys
from pathlib import Path

CLI = "tools/bfs-cli.py"


def run_cli(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, CLI, *args],
        capture_output=True,
        text=True,
        cwd=str(Path(__file__).parent.parent),
    )


def test_main_help():
    r = run_cli("--help")
    assert r.returncode == 0
    assert "BFS command-line interface" in r.stdout


def test_search_help():
    r = run_cli("search", "--help")
    assert r.returncode == 0
    assert "--query" in r.stdout
    assert "--type" in r.stdout
    assert "--limit" in r.stdout


def test_navigate_help():
    r = run_cli("navigate", "--help")
    assert r.returncode == 0
    assert "walk" in r.stdout
    assert "--to" in r.stdout
    assert "--session" in r.stdout


def test_describe_help():
    r = run_cli("describe", "--help")
    assert r.returncode == 0
    assert "--room" in r.stdout
    assert "--district" in r.stdout
    assert "--volume" in r.stdout


def test_gather_help():
    r = run_cli("gather", "--help")
    assert r.returncode == 0
    assert "--session" in r.stdout
    assert "--canonical-id" in r.stdout


def test_read_help():
    r = run_cli("read", "--help")
    assert r.returncode == 0
    assert "--canonical-id" in r.stdout


def test_article_help():
    r = run_cli("article", "--help")
    assert r.returncode == 0
    assert "--topic" in r.stdout
    assert "--session" in r.stdout


def test_status_help():
    r = run_cli("status", "--help")
    assert r.returncode == 0
    assert "health" in r.stdout


def test_subcommands_listed():
    r = run_cli("--help")
    assert r.returncode == 0
    for cmd in ["search", "navigate", "describe", "gather", "read", "article", "status"]:
        assert cmd in r.stdout, f"Subcommand '{cmd}' not listed in --help"
