#!/usr/bin/env python3
"""kimi-scribe — Kimi K2.6 article synthesis from gathered corpus texts.

Fetches volume texts from BFS server, builds an Egyptian-ontology synthesis
prompt, streams Kimi K2.6 via Nous Portal API, saves the completed article.

Output: JSON-lines to stdout for consumption by Rust ScribeBridge.
"""

import argparse
import json
import os
import re
import sys
import urllib.request
from datetime import datetime
from pathlib import Path

SCRIBE_API_BASE = os.environ.get("SCRIBE_API_BASE", "https://inference-api.nousresearch.com/v1")
DEFAULT_MODEL = os.environ.get("SCRIBE_MODEL", "moonshotai/kimi-k2-thinking")
LONG_CONTEXT_MODEL = "moonshotai/kimi-k2.6"
LONG_CONTEXT_MAX_TOKENS = 65536
FALLBACK_MODEL = "moonshotai/kimi-k2-0905"
DEFAULT_BFS_URL = os.environ.get("BFS_URL", "http://localhost:8081/rpc")
DEFAULT_OUTPUT_DIR = Path(os.environ.get("ARTICLES_DIR", str(Path(__file__).parent.parent / "articles")))
DEFAULT_MAX_TOKENS = 262144


def emit(obj):
    """Write one JSON-line to stdout."""
    print(json.dumps(obj, ensure_ascii=False), flush=True)


def load_api_key():
    """Load SCRIBE_API_KEY from env, falling back to .env in repo root."""
    from dotenv import load_dotenv

    repo_root = Path(__file__).parent.parent
    load_dotenv(repo_root / ".env")

    key = os.environ.get("SCRIBE_API_KEY", "")
    if not key:
        emit({"type": "error", "text": "SCRIBE_API_KEY not set — add to .env or export it"})
        sys.exit(1)
    return key


def rpc(bfs_url, method, params):
    """JSON-RPC 2.0 call to BFS server."""
    payload = {"jsonrpc": "2.0", "method": method, "params": params, "id": 1}
    req = urllib.request.Request(
        bfs_url,
        data=json.dumps(payload).encode(),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=15) as r:
        resp = json.loads(r.read())
    err = resp.get("error")
    if err:
        msg = err.get("message", str(err)) if isinstance(err, dict) else str(err)
        raise RuntimeError(msg)
    return resp.get("result")


def fetch_volumes(bfs_url, canonical_ids):
    """Fetch full text for each volume from BFS."""
    volumes = []
    for cid in canonical_ids:
        try:
            vol = rpc(bfs_url, "get_volume", {"canonical_id": cid})
            volumes.append(vol)
        except Exception as e:
            emit({"type": "status", "text": f"Could not fetch {cid}: {e}"})
    return volumes


def build_passage_block(volumes):
    """Format volume texts into a passage block for the synthesis prompt.

    Returns (passage_block_str, passage_count).
    """
    passages = []
    total_passages = 0
    for vol in volumes:
        cid = vol.get("canonical_id", "?")
        name = vol.get("conventional_name") or cid
        sentences = vol.get("sentences", [])

        lines = []
        for s in sentences[:24]:
            translit = s.get("transliteration") or ""
            trans = s.get("translation", {})
            en = trans.get("en") or trans.get("de") or ""
            if translit or en:
                total_passages += 1
                if translit and en:
                    lines.append(f"  Transliteration: {translit}")
                    lines.append(f"  Translation: {en}")
                    lines.append("")
                elif translit:
                    lines.append(f"  Transliteration: {translit}")
                    lines.append("")
                elif en:
                    lines.append(f"  Translation: {en}")
                    lines.append("")

        passage = f"--- {cid} \u00b7 {name} ---\n" + "\n".join(lines)
        passages.append(passage)

    return "\n\n".join(passages), total_passages


def stream_kimi(messages, api_key, model=DEFAULT_MODEL, max_tokens=DEFAULT_MAX_TOKENS):
    """Stream Kimi synthesis via OpenAI SDK.

    Yields (phase, text) tuples:
      ("model", model_name) — active model changed (for UI updates)
      ("reasoning", chunk)  — model thinking
      ("content", chunk)    — article output
      ("truncated", "")     — article hit output token limit

    Fallback chain: primary model → kimi-k2-thinking (262K output) → kimi-k2-0905.
    If the primary model truncates (finish_reason=length), automatically retries
    on kimi-k2-thinking which has a 262K output cap.
    """
    from openai import OpenAI

    client = OpenAI(api_key=api_key, base_url=SCRIBE_API_BASE)

    def _iter_stream(mdl, mtokens):
        stream = client.chat.completions.create(
            model=mdl, messages=messages, stream=True,
            max_tokens=mtokens, temperature=0.6,
        )
        finish_reason = None
        for chunk in stream:
            c = chunk.choices[0] if chunk.choices else None
            if not c:
                continue
            if c.finish_reason:
                finish_reason = c.finish_reason
            # Content phase — standard delta.content
            if c.delta.content:
                yield ("content", c.delta.content)
                continue
            # Reasoning phase — Kimi sends reasoning in model_extra
            extras = getattr(c.delta, "model_extra", None) or {}
            reasoning = extras.get("reasoning", "")
            if reasoning:
                yield ("reasoning", reasoning)
        if finish_reason == "length":
            yield ("truncated", "")

    # Primary model
    yield ("model", model)
    truncated = False
    try:
        for phase, text in _iter_stream(model, max_tokens):
            if phase == "truncated":
                truncated = True
                break
            yield (phase, text)
    except Exception as e:
        emit({"type": "status", "text": f"Primary model failed: {e}. Trying {LONG_CONTEXT_MODEL}..."})
        truncated = True  # treat failure as reason to try long-context model

    # If truncated, retry on kimi-k2-thinking with 262K output
    if truncated:
        emit({"type": "status", "text": f"Output truncated — retrying on {LONG_CONTEXT_MODEL} (262K output)..."})
        yield ("model", LONG_CONTEXT_MODEL)
        try:
            for phase, text in _iter_stream(LONG_CONTEXT_MODEL, LONG_CONTEXT_MAX_TOKENS):
                if phase == "truncated":
                    emit({"type": "status", "text": "⚠ Article truncated even on long-context model"})
                    break
                yield (phase, text)
        except Exception as e:
            emit({"type": "status", "text": f"Long-context model failed: {e}. Trying {FALLBACK_MODEL}..."})
            yield ("model", FALLBACK_MODEL)
            yield from _iter_stream(FALLBACK_MODEL, LONG_CONTEXT_MAX_TOKENS)


def slugify(title):
    """Convert article title to filename slug."""
    slug = re.sub(r'["\'\u201c\u201d\u2018\u2019]', '', title.lower())
    slug = re.sub(r'[^a-z0-9]+', '-', slug).strip('-')
    return slug[:80]


def save_article(text, title, volume_count, model, output_dir):
    """Save completed article to markdown file with metadata header. Returns file path."""
    date_str = datetime.now().strftime("%Y-%m-%d")
    header = (
        f"{text.split(chr(10), 1)[0]}\n"  # keep original # title line
        f"### Synthesized from the Aegyptus Corpus \u00b7 The Stacks v3\n\n"
        f"*Written: {date_str} \u00b7 Sources: {volume_count} texts \u00b7 Model: {model}*\n\n"
        f"---\n\n"
    )
    # Replace the first line (# title) with the enriched header
    body = text.split("\n", 1)[1] if "\n" in text else ""
    full_text = header + body

    slug = slugify(title)
    if not slug:
        slug = "untitled"
    path = output_dir / f"{slug}.md"

    counter = 1
    while path.exists():
        path = output_dir / f"{slug}-{counter}.md"
        counter += 1

    output_dir.mkdir(parents=True, exist_ok=True)
    path.write_text(full_text, encoding="utf-8")
    return str(path)


def extract_title(text):
    """Extract title from article text (first # heading)."""
    for line in text.split("\n"):
        line = line.strip()
        if line.startswith("# "):
            return line[2:].strip().strip('"').strip("'")
        if line.startswith("## "):
            return line[3:].strip().strip('"').strip("'")
    return "untitled"


def main():
    parser = argparse.ArgumentParser(
        description="Kimi K2.5 article synthesis from gathered Aegyptus corpus texts."
    )
    parser.add_argument(
        "--volumes", required=True,
        help="Comma-separated canonical IDs (e.g. aeg_oraec_15,aeg_oraec_2)"
    )
    parser.add_argument(
        "--topic", default="",
        help="Synthesis topic/focus (optional — derived from texts if omitted)"
    )
    parser.add_argument(
        "--bfs-url", default=DEFAULT_BFS_URL,
        help=f"BFS server JSON-RPC endpoint (default: {DEFAULT_BFS_URL})"
    )
    parser.add_argument(
        "--model", default=DEFAULT_MODEL,
        help=f"Model to use (default: {DEFAULT_MODEL})"
    )
    parser.add_argument(
        "--max-tokens", type=int, default=DEFAULT_MAX_TOKENS,
        help=f"Max tokens for synthesis (default: {DEFAULT_MAX_TOKENS})"
    )
    parser.add_argument(
        "--output-dir", type=Path, default=DEFAULT_OUTPUT_DIR,
        help=f"Directory for saved articles (default: {DEFAULT_OUTPUT_DIR})"
    )
    args = parser.parse_args()

    volume_ids = [v.strip() for v in args.volumes.split(",") if v.strip()]
    if not volume_ids:
        emit({"type": "error", "text": "No volume IDs provided"})
        sys.exit(1)

    # Load API key (exits with error if missing)
    api_key = load_api_key()

    # Fetch volumes from BFS
    emit({"type": "status", "text": f"Fetching {len(volume_ids)} volumes from the library..."})
    volumes = []
    for i, cid in enumerate(volume_ids, 1):
        emit({"type": "status", "text": f"Fetching volume {i}/{len(volume_ids)}: {cid}"})
        try:
            vol = rpc(args.bfs_url, "get_volume", {"canonical_id": cid})
            volumes.append(vol)
        except Exception as e:
            emit({"type": "status", "text": f"Could not fetch {cid}: {e}"})
    if not volumes:
        emit({"type": "error", "text": "No volumes could be fetched from BFS"})
        sys.exit(1)

    # Build passage block
    emit({"type": "status", "text": f"Building passage block from {len(volumes)} volumes..."})
    passage_block, passage_count = build_passage_block(volumes)

    # Build context
    topic_line = f"Topic: {args.topic}" if args.topic else "Topic: derive the central theme from the passages below"
    context = (
        f"The player has gathered {len(volumes)} texts from the library.\n"
        f"{topic_line}\n\n"
        f"=== GATHERED PASSAGES ===\n\n"
        f"{passage_block}\n\n"
        f"=== END PASSAGES ===\n\n"
        f"Think carefully about how these texts relate to each other. Identify "
        f"cross-references, shared vocabulary, and ontological connections between "
        f"passages before writing.\n\n"
        f"Write the article now. Follow the system prompt rules exactly. "
        f"Quote generously from the source material — every major claim should "
        f"be anchored to a specific passage with both transliteration and translation."
    )

    # Load system prompt
    prompt_path = Path(__file__).parent / "scribe_prompt.md"
    if not prompt_path.exists():
        emit({"type": "error", "text": f"System prompt not found: {prompt_path}"})
        sys.exit(1)
    scribe_prompt = prompt_path.read_text(encoding="utf-8")

    emit({"type": "status", "text": f"Prompt ready \u2014 {passage_count} passages, {len(context.split())} words context"})

    # Build messages
    messages = [
        {"role": "system", "content": scribe_prompt},
        {"role": "user", "content": context},
    ]

    # Emit meta
    meta_title = args.topic or "synthesis from gathered texts"
    emit({
        "type": "meta",
        "title": meta_title,
        "volume_count": len(volumes),
        "passage_count": passage_count,
    })

    # Stream synthesis
    active_model = args.model
    emit({"type": "status", "text": f"Calling {args.model} via Nous Portal \u2014 waiting for first token..."})
    full_text = []
    reasoning_text = []
    reasoning_started = False
    content_started = False
    try:
        for phase, chunk in stream_kimi(messages, api_key, model=args.model, max_tokens=args.max_tokens):
            if phase == "model":
                active_model = chunk
                emit({"type": "model", "name": chunk})
                # Reset state on model switch (retry after truncation)
                if content_started:
                    full_text.clear()
                    reasoning_text.clear()
                    reasoning_started = False
                    content_started = False
                continue
            if phase == "reasoning":
                if not reasoning_started:
                    emit({"type": "status", "text": "Model is reasoning..."})
                    reasoning_started = True
                reasoning_text.append(chunk)
                emit({"type": "reasoning", "text": chunk})
            elif phase == "content":
                if not content_started:
                    emit({"type": "status", "text": "Writing article..."})
                    content_started = True
                full_text.append(chunk)
                emit({"type": "delta", "text": chunk})
    except Exception as e:
        emit({"type": "error", "text": f"Synthesis failed: {e}"})
        sys.exit(1)

    article = "".join(full_text)
    if not article.strip():
        emit({"type": "error", "text": "Synthesis produced empty output — model may have exhausted tokens during reasoning"})
        sys.exit(1)

    # Extract title and save
    title = extract_title(article)
    word_count = len(article.split())
    file_path = save_article(article, title, len(volumes), active_model, args.output_dir)

    # Save reasoning transcript alongside article for prompt analysis
    if reasoning_text:
        reasoning_dir = args.output_dir / "reasoning"
        reasoning_dir.mkdir(parents=True, exist_ok=True)
        slug = slugify(title) or "untitled"
        reasoning_path = reasoning_dir / f"{slug}.reasoning.md"
        reasoning_content = "".join(reasoning_text)
        reasoning_path.write_text(
            f"# Reasoning: {title}\n\n"
            f"*Model: {active_model} · Volumes: {len(volumes)} · "
            f"{datetime.now().strftime('%Y-%m-%d %H:%M')}*\n\n---\n\n"
            f"{reasoning_content}\n",
            encoding="utf-8",
        )
        emit({"type": "status", "text": f"Reasoning transcript saved: {reasoning_path}"})

    emit({
        "type": "complete",
        "text": article,
        "word_count": word_count,
        "file": file_path,
    })


if __name__ == "__main__":
    main()
