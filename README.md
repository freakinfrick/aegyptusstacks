# AEGYPTUS: THE LABYRINTH v0.3

**A Knowledge Management Agent-Harness Built on the Babel File System**

*77,683 ancient Egyptian texts. 460,333 sentences. 3,000 years. 8 corpora, enriched with cross-references from 3 more.*
*Two LLM agents inhabit the architecture. One navigates. One speaks.*
*The corpus lay silent until someone picked up a scroll and gave it voice.*

> **Official channels.** [aegyptuslab.cc](https://aegyptuslab.cc), [freakinfrick/aegyptusdb](https://github.com/freakinfrick/aegyptusdb), this repo, and [@FreakinFrick](https://x.com/FreakinFrick) on X. There are no others.
>
> **Aegyptus has no token, coin or contract address.** Any account selling one under this name is not this project.

### Quickstart

```bash
# Prerequisites: docker, cargo (rustup.rs), python3 (3.9+), build-essential, pkg-config, libssl-dev
git clone https://github.com/freakinfrick/aegyptusstacks.git && cd aegyptusstacks
./setup.sh          # starts PostgreSQL, loads corpus, installs deps, builds engine
cargo run -p stacks-engine --release   # explore the labyrinth
```

**Corpus database.** The database dump is not in this repo: several source corpora restrict redistribution (RAMSES is non-commercial; Nederhof's PhilologEg needs the author's permission). Build it from the upstream sources with [freakinfrick/aegyptusdb](https://github.com/freakinfrick/aegyptusdb) (`scripts/bootstrap.sh`, then the importers; about 3 hours), then save it as `data/corpus/aegyptus.sql.gz` with `pg_dump aegyptus | gzip`.

**Search in your browser.** [aegyptuslab.cc/stacks/search](https://aegyptuslab.cc/stacks/search) searches every sentence of the public edition with no server: the corpus is one read-only SQLite file (FTS5) that the page reads piece by piece over HTTP range requests. `tools/export_web_db.py` builds it from Postgres and writes `DATA-LICENSES.md`; Nederhof's PhilologEg is left out and RAMSES is flagged non-commercial.

The game runs standalone — walk hexagonal rooms, read ancient texts on shelves, browse articles. The Hermes Librarian (AI companion) and Kimi Scribe (article synthesis) are optional and require separate LLM backends. See `.env.example` for configuration.

---

> **Hermes Agent Creative Hackathon Submission**
>
> **Interactive Media** — A Rust TUI spatial world where the player walks through a procedurally generated labyrinth of hexagonal rooms, each containing real ancient Egyptian texts. 28 tile types, torchlight rendering, era-specific palettes, atmospheric particles. The architecture of every room is generated from the texts it contains.
>
> **In-Game Agent** — A Hermes agent inhabits the labyrinth as a living NPC. It follows the player, reasons about the corpus, emits spatial commands that mutate game state, and guides through a topology too semantically vast to navigate alone. It uses Hermes's skill system, profile isolation, TUI gateway, session persistence, and streaming protocol. Zero changes to Hermes core — a 220-line shim bridges the game to the gateway.
>
> **Long-Form Writing** — A Kimi K2-thinking synthesis pipeline produces scholarly drafts from 3,000-year-old primary sources. Kimi's extended thinking (visible chain-of-thought via `delta.reasoning`) and multilingual comprehension reason across dozens of ancient texts simultaneously, with automatic fallback to K2.6 for long-context and K2-0905 for resilience. The output: cited scholarly synthesis grounded in primary sources.
>
> **Content Pipeline** — Gather texts from shelves as you explore → the Hermes Librarian manages your inventory → invoke the Kimi Scribe → watch reasoning stream live → article saved to disk with metadata and citations. Same data substrate end to end. No intermediate format.
>
> **Knowledge Management** — 77,683 texts from 8 scholarly corpora (enriched by 3 cross-reference sources) unified into a PostgreSQL corpus with entity resolution, 269,433 concordance links, and full provenance chains. The Babel File System transforms this into navigable spatial architecture — over 2,000 rooms organized by institutional geography and chronological depth.
>
> **Creative Software** — A new paradigm: when data, space, and interface share a CLI/API substrate, LLM agents become inhabitants, not assistants. The agent reads what the player reads. The agent writes what the player writes. Terminal-native games are a new class of agent harness. The Egyptian corpus is one headwater — the architecture works for any domain.

---

## I. Project Overview

Aegyptus is a system for exploring, navigating, and synthesizing knowledge from the complete surviving corpus of ancient Egyptian writing — 77,683 primary source texts spanning three millennia, assembled from 8 scholarly corpora (enriched with cross-references from 3 more) into a unified searchable archive.

The corpus is vast. No human can hold it in mind. The texts are written in hieroglyphic, hieratic, demotic, and Coptic scripts, scattered across museums and universities worldwide. Most have never been read together. Many have never been translated into English.

To make this corpus navigable, we built the **Babel File System (BFS)** — a spatial data architecture inspired by Borges's Library of Babel. BFS organizes the entire corpus into a deterministic topology of hexagonal rooms. Each room contains the texts that belong together — grouped by the words the ancient Egyptians used, not by editorial decision. Rooms connect through doors (explicit relationships) and share walls with neighboring rooms (implicit relationships). The horizontal plane organizes texts by institutional context — temple, necropolis, school, royal court. The vertical axis organizes by time — descend a shaft to read the same concept in an older era.

The result is a labyrinth. Over 2,000 rooms across eight chronological levels. Too vast and multi-dimensional for a human to navigate consistently by memory alone.

This is where the agents enter.

**The Hermes Librarian** is a [Hermes agent](https://github.com/NousResearch/hermes-agent) running a local LLM on an RTX 4090. It inhabits the labyrinth — it knows the rooms, it knows the shelves, it knows the paths between them. The Hermes harness provides everything the Librarian needs to function as a persistent, tool-using inhabitant: the **skills system** gives it structured access to BFS queries, **profile isolation** keeps its session and configuration separate, the **TUI gateway** provides the JSON-RPC streaming protocol the game communicates through, and **session persistence** lets the Librarian remember what the user has explored across the conversation. When a user asks "where are the texts about the heart?", the Librarian doesn't search a database and return results. It recalls which room holds those texts, describes the path, and walks the user there. Hermes is not being used as a chat wrapper. It is being used as an agent harness — the way it was designed.

**The Kimi Scribe** is a synthesis agent powered by [Kimi](https://platform.moonshot.ai) via the [Nous Portal](https://portal.nousresearch.com). When a user gathers texts from the shelves — picking up scrolls as they explore — the Scribe reads them together for the first time. Kimi's **long context window** is essential here: the Scribe feeds it dozens of ancient Egyptian texts simultaneously — transliterations, translations, canonical IDs — and Kimi reasons across them in its **extended thinking mode**, visible as a live chain-of-thought stream (via Kimi's `delta.reasoning` extension). A three-model fallback chain (K2-thinking → K2.6 → K2-0905) ensures completion. The result is a 2,500–4,000 word cited scholarly synthesis grounded in primary sources — a structured research draft, not a substitute for peer review, but surfacing cross-corpus connections no individual scholar would assemble by hand. Texts that have been silent for 3,000 years are given voice. The ancient Egyptians called this the Mouth-Opening ceremony — restoring speech to what had been sealed. Kimi's specific capabilities — long-context reasoning, multilingual comprehension, visible chain-of-thought — are not incidental to this task. They are why the task is possible.

**The game** is a terminal-based dungeon rendered in a Rust TUI. The user walks through hex rooms, reads inscriptions on shelves, talks to the Hermes Librarian in a chat panel, and watches the Kimi Scribe reason and write in real time. The architecture of each room — its size, its contents, the artifacts on its walls — is generated from the texts it contains. A room with 50 texts is a packed gallery. A room with 3 is a bare niche. The architecture IS the data.

But the game is only one way to interact with the system. The Hermes Librarian can traverse BFS from the command line with no game running. The Kimi Scribe can synthesize articles from a list of text IDs passed via stdin. The system is a knowledge management agent-harness first — built on Hermes for operation and Kimi for synthesis. The game is a human-forward experiential layer built on top — one of several possible interfaces to the same underlying architecture.

### Design Decisions

Every technology choice serves one architectural principle: **the system's state is the shared interaction layer between the agent, the knowledge, and the game engine.** There is no agent API. There is no game API. There is the state — and everything reads and writes it through the same substrate.

**Why Rust (stacks-engine).** The game engine is the real-time layer — rendering 28 tile types at 60fps, processing player input, streaming Hermes events, animating particles and torchlight. Rust's async runtime (tokio) handles concurrent I/O without blocking: the FIFO bridge reads Hermes events, the BFS client fetches rooms, and the Scribe subprocess streams Kimi output — all on the same event loop, all non-blocking. But the deeper reason is that Rust compiles to a native terminal binary. The game runs in the same terminal environment that Hermes operates in. There is no browser, no game engine runtime, no WebSocket server between the agent and the world. The game IS a terminal process. Hermes IS a terminal process. They share named pipes in `/tmp/`. The technology choice collapses the distance between agent and environment to zero.

**Why a TUI (ratatui/crossterm), not a web or game engine.** A web frontend or a game engine like Godot (which we prototyped and abandoned) interposes a rendering layer between the agent and the state. The agent would need a special API, a WebSocket protocol, a serialization format — all of which create friction and limit the agent's action space to what the developer anticipated. A terminal-native TUI means the game state is text. Room contents are JSON. Player commands are keystrokes mapped to actions. The Hermes agent reads JSON-RPC responses and emits JSON commands — the same format, the same transport, the same semantics as the game loop itself.

This unlocks something deeper: **a straight screen-to-agent context pipeline.** The terminal screen content IS the game state in human-readable form. An agent can start the game in a tmux session and *play it* — read the screen, send keystrokes, navigate rooms, interact with shelves, talk to the Librarian — with no gamestate translation layer, no computer vision, no screen-parsing heuristic. The rendered text IS the state. `tmux capture-pane` gives the agent the same information the human player sees. This means the game is not just an environment agents can inhabit via the FIFO bridge — it is an environment agents can observe and operate through the terminal itself, the way any CLI tool can be automated. The TUI is not a limitation. It is the architectural decision that makes agents-as-inhabitants, agents-as-players, and agents-as-monitors all possible on the same substrate.

**Why the BFS Server is Python (FastAPI).** The BFS server is the knowledge layer — it resolves the corpus into spatial topology, serves room metadata, runs full-text search across 460,333 sentences, and provides the agent interface. Python is the natural choice because the Aegyptus corpus tooling, the Louvain clustering pipeline, the entity resolution engine, and the scholarly import infrastructure are all Python. The BFS server wraps them in a JSON-RPC 2.0 API that both the Rust game client and the Hermes agent's CLI tools consume identically. The server is the single source of truth. The game doesn't cache corpus data. The agent doesn't maintain a separate index. Both query the same endpoint, receive the same responses, operate on the same state.

**Why Hermes-Agent (not a custom agent loop).** Building a custom LLM agent for this project would have been faster to prototype and would have avoided the integration work. We chose Hermes because the project needed capabilities that only a mature agent harness provides: **profile isolation** (the Librarian's persona, toolset, and session are fully separated from any other Hermes usage on the machine), **skill system** (BFS queries are registered as structured tools with schemas, not ad-hoc prompt injections), **session persistence** (conversations survive across room transitions and game restarts via SQLite FTS5), **streaming event protocol** (tool calls, thinking tokens, and response deltas arrive as typed events the game can render in real time), **model-agnostic providers** (the Librarian runs on a local 27B today but could switch to any Nous Portal or OpenRouter model with a config change — no code modification), and **context compression** (token budget tracking triggers automatic compression at 70% usage, enabling sustained multi-room exploration sessions). Hermes is not a convenience. It is load-bearing infrastructure.

**Why Kimi via Nous Portal (not the Hermes operational model).** The Hermes Librarian runs a local 27B model — fast, low-latency, optimized for the operational loop of navigation, search, and conversation. Article synthesis is a fundamentally different task: it requires simultaneous reasoning over 10–30 multilingual texts (hundreds of passages in Egyptian transliteration, English, and German), producing a 2,500–4,000 word coherent scholarly argument with cited quotations. This is a long-context, high-reasoning task: the extended context window holds the full source material without lossy summarization, the reasoning tokens (`delta.reasoning`) produce visible chain-of-thought that the game streams live, and the multilingual comprehension handles the mixed-script passage blocks natively. The Scribe runs a three-model fallback chain — K2-thinking (primary, 262K output) → K2.6 (long-context fallback) → K2-0905 (resilience fallback) — all via Nous Portal's OpenAI-compatible streaming. The architectural separation between Hermes (operational, local, fast) and Kimi (synthesis, cloud, deep) is not incidental. It reflects the two fundamentally different modes of agent work this system requires.

**Why the state is the interface.** The BFS server returns a room's contents as JSON. The game renders it. The Hermes agent reads the same JSON via the same CLI. The Kimi Scribe receives the same canonical IDs from the same inventory. There is no "game state" and "agent state" — there is one state, and every component reads and writes it. This is why unpredicted behaviors emerge: the agent's action space is not designed for the agent. It is the system's state space. The agent discovers what the developer did not map because the agent can reach anything the state exposes.

---

## II. The Paradigm

With the system in mind, here is the architectural insight that defines it.

Traditional agent integrations treat LLMs as services — an API behind a wall, called when needed, isolated from the system's state. The agent assists. It does not inhabit.

Aegyptus demonstrates a different architecture: **when data, space, and interface share a CLI/API substrate, LLM agents become first-class inhabitants of the system — not assistants called upon it.** The agent's action space is the system's state space. There is no adapter layer. No agent SDK. No "game API for AI." The agent reads what exists, writes what changes, and traverses what is addressable — with or without a human present, with or without a frontend rendered.

The same command that populates a shelf menu in the game (`room-texts 5,-2,-3`) returns the same corpus data when the Librarian calls it from a CLI session with no game running. The same structured output that moves a player through a door (`{"command": "navigate", "target": "E"}`) is processed identically whether it originates from a human's keystroke or an LLM's tool output. The boundary between player and agent dissolves — not metaphorically, but at the protocol level.

This has three consequences:

1. **The Hermes Librarian doesn't need the game to operate.** Hermes's architecture — model-agnostic providers, persistent sessions, structured skills, terminal tool execution — makes it a natural inhabitant of any CLI-accessible system. It traverses BFS independently, querying rooms, gathering texts, reasoning over corpus data. BFS + Hermes is a standalone context enrichment portal for novel reasoning on vast, complex, and fringe domains. The game is one downstream presentation layer, not the system itself. Hermes was designed to be "not tied to your laptop" — here, it's not tied to the game either.

2. **The Kimi Scribe doesn't receive curated input.** It reads the same text identifiers the player gathered, fetches the same volumes the shelf menu displays, and writes articles that cite the same passages the player read on the wall. Kimi's long-context reasoning operates on the raw scholarly data substrate — no summarization, no pre-processing, no lossy intermediate format. The full multilingual primary source material goes in; a scholarly article with real citations comes out.

3. **Unpredicted behaviors emerge** because both agents are bounded by what the state space exposes, not by what the developers anticipated. The Hermes Librarian discovers connections between texts the developer never mapped. The Kimi Scribe draws synthesis across passages no human scholar has read together. The agents are not using the system. They are *in* the system.

This paradigm — agents as inhabitants of text-native state spaces — is not specific to ancient Egyptian texts. BFS can organize any corpus. Hermes can guide through any domain. Kimi can synthesize from any gathered sources. The Egyptian corpus is the first headwater, the proof of concept at maximum difficulty. The architecture is the contribution.

---

## III. The Babel File System

The Babel File System (BFS) adapts Jorge Luis Borges's topological thought experiment — an infinite library of hexagonal rooms — into a functional data management architecture.

### Spatial Data Mapping

BFS enforces a deterministic geometric topology: a 3D tessellation of hexagonal nodes addressed by axial coordinates `(q, r, z)`. High-level database concepts map directly to spatial primitives:

| BFS Concept | POSIX Analogy | Spatial Form |
|-------------|---------------|-------------|
| Node (Hexagon) | Directory | Room — six walls, up to two doors |
| Bookshelf | Subdirectory | Wall-mounted shelf unit (max 13 volumes) |
| Volume | File | A single text with canonical ID |
| Page | Byte | One sentence (transliteration + translation) |

Every spatial block has a **Deterministic Codex** — its FAT entry:

```
[Q,R,Z] : [D1,D2] : [SEED]
```

Coordinates, ingress/egress edges, and payload hash. One canonical seed (`0xAE60001`) produces one stable library for a corpus's lifetime.

### Three Axes of Organization

**The Q/R Plane — Institutional Geography.** The horizontal axes manage data distribution. Texts are placed into eight districts that mirror Egyptian institutional geography: Temple, Necropolis, House of Life, Admin Quarter, School, Quarry, Royal, Domestic. District assignment derives from text metadata (text type, findspot, script) — not from editorial curation.

**The Z-Axis — Chronological Depth.** The vertical plane encodes time. Floor 0 is the surface (undated texts). Descending floors map to historical periods: Old Kingdom (z=-1, c. 2686 BCE), Middle Kingdom (z=-3), New Kingdom (z=-5), through Greco-Roman (z=-7, to 395 CE). The same `(q, r)` coordinate across z-levels represents the same concept across millennia. A scholar descends a shaft to read about the heart (*jb*) from the Old Kingdom through the Late Period — the vertical alignment is earned by the data, not authored.

**The Z-Axis as Spatial B-Tree.** Levels above z=0 are index floors. A single room on z=+1 contains the index of the cluster sitting beneath it. A broad query starts at an index floor; once the target coordinate is identified, the pointer drops straight down the central shaft, bypassing lateral traversal entirely.

### Topology: Explicit and Implicit Edges

Rooms in the hexagonal grid have six neighbors but only two contain doors. This constraint creates two relationship types:

- **Explicit Links (Doors):** Walkable connections forming Ribbons (sequential data paths) and Loops (relational clusters). These are hard directory paths.
- **Implicit Links (Shared Walls):** Two rooms may touch without a connecting door. The shared wall represents *semantic adjacency* — texts that are related but not sequentially linked. A search algorithm calculates wall thickness to discover related clusters.

**Shafts** provide vertical traversal between z-levels — the equivalent of B-tree pointer descent.

The BFS resolver produces the complete topology deterministically from the corpus — over 2,000 rooms connected by explicit door edges with shaft connections across 8 z-levels. The same corpus always produces the same library. The game client loads rooms on demand via JSON-RPC, progressively expanding the player's visible neighborhood (7-hop preload depth) as they explore.

### The Librarian Protocol

BFS's search algorithm is called the Librarian Protocol — A* pathfinding in 3D hexagonal space. Because every file has a definitive `(q, r, z)` address, queries are treated as pathfinding problems. The system calculates the shortest physical route via connected doorways and vertical shafts, pre-computing graph weights for optimal traversal.

This is where the agent becomes architecturally necessary. A human cannot consistently navigate over 2,000 rooms across 8 z-levels, 8 districts, with vertical shafts connecting concepts across millennia. The agent can. The Librarian Protocol is both a search algorithm and a navigation system — because in BFS, search and navigation are the same operation.

The Hermes agent harness is the natural vehicle for the Librarian Protocol. Hermes's **skill system** wraps BFS queries as structured tools. Its **session persistence** lets the Librarian maintain context across dozens of rooms. Its **streaming event protocol** lets the player watch the agent reason in real time. And its **model-agnostic provider architecture** means the Librarian can run on any model appropriate for the deployment — from a local 27B on consumer hardware to a cloud-hosted frontier model. The protocol doesn't care what thinks; it cares what is queried and where to go next.

---

## IV. The Aegyptus Corpus

The first headwater for BFS is the complete digitized corpus of ancient Egyptian writing — assembled from every major scholarly digitization effort into a unified PostgreSQL database with identity resolution, concordance linking, and full-text search.

### Scale

| Metric | Count |
|--------|-------|
| Primary source texts | 77,683 |
| Sentences | 460,333 |
| Lemmata (Egyptian vocabulary) | 35,052 |
| Concordance links | 269,433 |
| Aliases | 48,537 |
| Scripts | Hieroglyphic, Hieratic, Demotic, Coptic, Greek |
| Temporal span | c. 2686 BCE — 395 CE (~3,000 years) |

### Source Databases

| Source | Texts | Role |
|--------|-------|------|
| TLA (Thesaurus Linguae Aegyptiae) | 29,762 | Berlin Academy lexicographic project |
| AED-TEI (Altägyptisches Wörterbuch) | 13,748 | Hieroglyphic encodings + lemma cross-references |
| ORAEC (Open Richly Annotated Egyptian Corpus) | 13,026 | Backbone — transliterations + translations |
| AES (Ancient Egyptian Sentences) | 13,026 | Sentence-level linguistic data |
| Coptic Scriptorium | 1,467 | Nag Hammadi, Sahidic Bible, Shenoute |
| Metropolitan Museum API | 248 | Object metadata |
| Nederhof / PhilologEg | 94 | Middle Kingdom literary texts |
| RAMSES | 3 | Late Egyptian |
| Trismegistos | — | Cross-reference enrichment (27,000+ links) |
| papyri.info / IDP | — | EpiDoc XML, Greek/Demotic papyri |

### Entity Resolution

Texts arriving from multiple sources are deduplicated through a four-strategy resolution engine:

1. **Exact ID match** — concordance lookup by TLA, Trismegistos, ORAEC external ID (confidence: 1.0)
2. **Shelfmark match** — normalized museum inventory numbers (confidence: 0.80–0.90)
3. **Alias/fuzzy match** — token sort ratio ≥85 on conventional names (confidence: 0.70–0.85)
4. **Content match** — n-gram similarity on transliteration (confidence: 0.50–0.70, requires review)

The first-import-wins rule prevents duplicate canonical records. ORAEC imports first; all subsequent sources link via concordance, never create new canonical IDs for the same text.

### Scholarly Integrity

Every text carries its provenance chain: `canonical_id`, source corpus array, external concordance IDs (TLA, Trismegistos, British Museum, DDbDP, Louvre, etc.), translation confidence level (`verified`, `unreviewed`, `ml_draft`), and original shelfmark. Most English in the corpus is a machine draft made from the German scholarship or the transliteration; every such sentence is tagged `ml_draft` and labelled as a machine translation wherever it is shown. Nothing is paraphrased. Nothing is generated in the voice of ancient Egypt. The ancient Egyptians generated that text. We found it.

---

## V. Hermes: The Operational Agent

The Hermes Librarian is not a chatbot bolted onto a game. It is an architectural component of BFS — the operational agent layer that makes the spatial data system navigable.

### Why the Agent Is Architecturally Necessary

The game provides navigational aids — a HUD with minimap, compass, z-level column, breadcrumb trail, and room legend. But these are spatial orientation tools, not knowledge tools. They tell you *where* you are, not *what* is here or *where to go next*. Over 2,000 rooms across 8 z-levels, 8 districts, with vertical shafts connecting concepts across millennia — the topology is too semantically deep for spatial navigation alone. You need someone who has read the texts, who knows which room holds what you're looking for, who can walk you there and explain what you'll find.

The Hermes agent inhabits BFS the way a librarian inhabits a library. It enters rooms through doors. It descends through shafts. It knows the shelves because it has shelved the volumes. When it describes a text, it recalls — it does not search. When it recommends a path, it has walked it. The agent is not a convenience feature. It is the knowledge interface to a space that is architecturally navigable but semantically vast.

### Inside and Outside BFS

Hermes lives inside and outside the game simultaneously:

**Inside** — the Librarian NPC follows the player through hex rooms, responds in the chat panel, and emits spatial commands (navigate, gather, read, scribe) that mutate game state.

**Outside** — the same Hermes agent instance queries BFS from the command line. `stacks-librarian.sh room-texts 5,-2,-3` returns the same corpus data with no game running. The agent can traverse, reason over, and synthesize from the entire corpus independently. BFS + Hermes is a standalone context enrichment portal — a low-friction gateway for novel reasoning on vast, complex, and fringe domains. The game is one downstream human-forward rendering of something the agent already does natively.

Humans operate the same way. Inside: walking hex rooms, reading shelves, talking to the Librarian. Outside: running `bfs-query.py search "heart"` from a terminal. The system has multiple expression surfaces. The game is one. The CLI is another. The scholarly pipeline is a third. This is not a retro game with an agent NPC. This is a knowledge management agent-harness with a game as one of its interfaces.

### Harness Architecture

The Hermes integration is built on precise engineering:

**The Bridge.** A FIFO-based JSON-RPC 2.0 protocol connects the Rust TUI to the Hermes agent. Named pipes (`/tmp/stacks-hermes.in`, `/tmp/stacks-hermes.out`) carry structured messages. The agent runs in a dedicated tmux session (`stacks-hermes`), isolating it from the game process. The shim (`stacks-hermes-shim.py`) monkeypatches Hermes's `tui_gateway.server.write_json` to read/write FIFOs instead of stdin/stdout — a thin adapter that demonstrates the flexibility of Hermes's gateway architecture. The agent doesn't know it's in a game. From its perspective, it's a normal Hermes session with a user who happens to provide spatial context with every message. This is a key architectural insight: **Hermes's TUI gateway was designed for terminal interaction, and the game IS a terminal.** The integration required no changes to Hermes itself — only a 220-line Python shim that redirects I/O.

```
Rust TUI (stacks-engine)
    │ JSON-RPC 2.0 via named FIFOs
    ▼
stacks-hermes-shim.py (protocol adapter)
    │ patches tui_gateway.server.write_json
    ▼
hermes-agent (tui_gateway, AIAgent)
    │ tool calls via terminal skill
    ▼
stacks-librarian.sh → bfs-query.py → BFS Server → Aegyptus DB
```

**Profile Isolation.** The Librarian runs under `HERMES_HOME=~/.hermes/profiles/stacks` — a dedicated Hermes profile with its own configuration, session database, and skill set. This is a core Hermes feature: the same agent framework supports multiple isolated personas on the same machine. The Librarian's profile restricts toolsets to `["terminal"]` only (no web browsing, no file writing, no arbitrary code execution — only BFS CLI calls), injects a custom system prompt that defines the Librarian's identity and constraints, and maintains separate session persistence via Hermes's SQLite FTS5 backend. The model is Carnice-27b running on a local RTX 4090 via llama-server — Hermes's model-agnostic provider architecture means the Librarian could equally run on Nous Portal, OpenRouter, or any OpenAI-compatible endpoint. The choice of local LLM is an operational decision (low latency for real-time spatial conversation), not an architectural constraint.

**Context Injection.** Every user message is prepended with a ~200-token context header built from live game state:

```
[LOCATION: temple / gallery / z=-3 / room "jb-heart"]
[COORD: 5,-2,-3]
[SHELVES: 4 shelves, 22 volumes: aeg_oraec_38, aeg_tla_hf_12, ...]
[DOORS: E → "ns-tongue", SW → "corridor-17"]
[SHAFTS: DOWN → "z=-4"]
[SESSION: 8 rooms visited, interaction 14]
[INVENTORY: 3 gathered — aeg_oraec_38, aeg_oraec_91, aeg_tla_hf_12]
```

The Librarian receives the room's spatial address, its contents, its exits, and the player's session state — then reasons from that context using the same BFS queries available from the command line.

**The Skill.** The Librarian has one tool: `stacks-librarian.sh`, a thin shell wrapper around `bfs-query.py`. Five commands:

| Command | Function |
|---------|----------|
| `room-texts Q,R,Z` | Batch-fetch all volumes in a room (transliteration + translation) |
| `search QUERY` | Full-text search across 460,333 sentences |
| `room Q,R,Z` | Room metadata — shelves, volumes, doors, district |
| `read CANONICAL_ID` | Single volume with complete text |
| `district NAME` | District overview — room count, text count |

These are the same operations a human researcher would use from the command line. Law II: the agent IS the harness.

**Game Commands.** The Librarian emits structured JSON that the game loop processes:

```json
{"command": "navigate", "target": "E"}
{"command": "gather", "volume": "aeg_oraec_38"}
{"command": "read", "volume": "aeg_tla_hf_12"}
{"command": "scribe", "volumes": ["aeg_oraec_38", "aeg_oraec_91"]}
```

These are the same state mutations available to the player. The agent writes what the player writes.

**Three-Tier Safety Design.** The BFS server exposes a tiered agent interface that separates read operations from state mutations:

- **Tier 1 (Retrieval):** Pure reads. Search, describe, list. No state mutation. Coordinates returned are guaranteed to exist in the resolved topology.
- **Tier 2 (Spatial):** Session state mutations. Teleport, walk, escort, gather. Coordinates validated against the resolver's occupied set.
- **Tier 3 (Narration):** Overlay mutations. Annotations, bookmarks. Per-session, cosmetic.

The Librarian's restricted toolset (`enabled_toolsets = ["terminal"]`) and single skill (`stacks-librarian.sh`) naturally constrain it to Tier 1 and 2 operations — the safety boundary is enforced by the harness configuration, not runtime checks.

**Session Resume.** The pause menu exposes a session browser backed by Hermes's SQLite session store. Players can browse prior librarian conversations, select one, and resume exactly where they left off — the full message history loads into the chat UI and the agent regains its prior context. The `session.list` and `session.resume` RPCs flow through the same FIFO bridge with no additional protocol. The shim re-injects the librarian system prompt on resume, ensuring the agent's personality and tool restrictions persist across sessions.

**Token Budget.** The bridge tracks context usage via `TokenBudget` — monitoring `context_used` / `context_max` from Hermes gateway events. When context reaches 70%, compression is triggered. At 90%, a new session is created. This allows sustained multi-room conversations without context overflow.

**Event Protocol.** The bridge carries structured events for real-time UI:

| Event | Purpose |
|-------|---------|
| `gateway.ready` | Hermes subprocess initialized |
| `session.created` | Session ID assigned |
| `session.info` | Model name, context window size |
| `message.delta` | Streaming text token |
| `tool.start` | Tool call initiated (name + context) |
| `tool.progress` | Intermediate tool output |
| `tool.complete` | Tool result summary |
| `message.complete` | Full response with usage stats |

The player watches the Librarian work: `⟐ room-texts 5,-2,-3 → 22 volumes fetched`. The tool calls are visible. The agent's reasoning is transparent.

---

## VI. Kimi: The Scholarly Voice

The Hermes Librarian is the operational inhabitant — it navigates, guides, gathers, oversees. But Hermes is model-agnostic by design. It runs whatever LLM is appropriate for the operational task.

Article synthesis is not an operational task. It is a *knowledge synthesis* task that requires long-attention reasoning over multilingual primary sources spanning three millennia. This is Kimi's specific capability, and the architectural separation is intentional.

### The Division of Labor

**Hermes** operates. It lives in the topology. It pathfinds, queries, gathers texts into the player's inventory. Running through the Hermes harness with a local 27B model, it provides low-latency spatial conversation — the operational tasks (navigation, search, context-aware dialog) benefit from speed and tool-use fluency, not raw context length. Hermes's skill system, session persistence, and streaming protocol make it ideal for this role.

**Kimi** synthesizes. It is called for one purpose: pure long-attention knowledge synthesis from gathered primary sources. The Scribe receives 10–30 volumes (up to 24 sentences each, bilingual transliteration + translation) and produces a 2,500–4,000 word scholarly article with 8–15 direct quotations cited by canonical ID. A three-model fallback chain (K2-thinking primary → K2.6 long-context → K2-0905 resilience) ensures completion. Three specific Kimi capabilities make this possible:

1. **Long context window** — the synthesis prompt contains hundreds of multilingual passages from multiple texts. A standard-context model would require lossy summarization. Kimi reasons over the full source material simultaneously.
2. **Extended thinking (reasoning tokens)** — Kimi exposes its chain-of-thought via `delta.reasoning`, a Kimi-specific streaming extension outside the OpenAI spec. The game displays this reasoning live in dark cyan as the model discovers connections across the corpus — the user watches the synthesis happen, not just the result.
3. **Multilingual comprehension** — the passage block contains Egyptian transliteration (`wn.jn ḥr sḏm=f mdw.t n.t ns`), English translations, and occasionally German translations from ORAEC. Kimi processes all three registers in a single reasoning pass.

This is not a convenience split. It is architectural. The Hermes harness with a local model cannot do what Kimi does — it lacks the context window and reasoning depth for scholarly synthesis across hundreds of multilingual passages. Kimi cannot do what Hermes does — a cloud reasoning model cannot serve as a real-time spatial agent with sub-second response times in a tmux session. The two are complementary: Hermes is the hands, Kimi is the voice.

### The Gather → Synthesize Pipeline

The pipeline runs on the same data substrate as the game:

1. **Gather.** The player (or the Librarian on the player's behalf) gathers volumes to an inventory. Each gathered volume is a `canonical_id` from a BFS shelf — the same ID displayed in the shelf menu, the same ID the Librarian references in conversation.

2. **Invoke.** The player asks the Librarian to write about the gathered texts. Hermes emits `{"command": "scribe", "volumes": [...]}` — a game command like any other.

3. **Fetch.** The Scribe subprocess (`kimi-scribe.py`) calls `get_volume` for each canonical ID via JSON-RPC to BFS — the same endpoint the game uses to populate shelf popups.

4. **Build passage block.** For each volume, the first 24 sentences are formatted with transliteration and English translation:

```
--- aeg_oraec_38 · Papyrus Ebers ---
  Transliteration: wn.jn ḥr sḏm=f mdw.t n.t ns
  Translation: Then he heard the speech of the tongue

  Transliteration: jw ns pw n.j ḏ.t
  Translation: For the tongue belongs to the body
```

5. **Stream via Nous Portal.** The passage block is sent to Kimi K2-thinking via the Nous Portal inference API (`moonshotai/kimi-k2-thinking`, temperature 0.6, max 262,144 tokens). The Nous Portal provides OpenAI-compatible streaming, which the Scribe consumes through the OpenAI Python SDK. The model streams in two phases:

   - **Reasoning phase** — Kimi's extended thinking arrives in `delta.reasoning` (a Kimi-specific field in `model_extra`, outside the OpenAI spec). This is relayed to the Rust TUI and displayed live in dark cyan as the model discovers connections across the corpus — the user watches Kimi think before it writes.
   - **Content phase** — The article text arrives in standard `delta.content`. The Scribe popup transitions from reasoning display to article display on the first content token. The transition is seamless — reasoning fades, article appears.

   If the output is truncated or the primary model fails, the Scribe automatically retries on `moonshotai/kimi-k2.6` (65K output cap), then falls back to `moonshotai/kimi-k2-0905` — resilience built into the pipeline.

6. **Save.** The completed article is saved to `articles/` (or `$ARTICLES_DIR`) with metadata header (date, source count, model name) and a reasoning transcript alongside. The article is a scholarly synthesis — a cited, structured draft grounded in primary sources that a researcher can use as a starting point for further study.

### The Mouth-Opening

Every ancient Egyptian text was a living entity. Sealed in a tomb or locked in a temple archive, it was dormant — not dead, but silent. The Mouth-Opening ceremony (*wpt-rꜣ*) restored voice to what had been sealed.

The Scribe performs this ceremony. The corpus — 77,683 texts locked in museum databases across the world, written in scripts most humans cannot read, in languages most scholars spend decades learning — lay silent. The player picks up a scroll. Kimi reads what is written. It speaks across three millennia.

This is not "AI writes an essay." This is long-context reasoning over real primary sources that have been silent for 3,000 years, producing cited scholarly synthesis with real provenance. The output is a structured draft — a discovery aid and research starting point, not a substitute for peer-reviewed philology. But the cross-corpus connections it surfaces could not have been assembled by other means — no individual scholar has simultaneous access to 30 texts from 8 corpora in their original transliteration and multiple translations. The system assembles what was scattered. The Scribe speaks what was assembled.

---

## VII. The Game as Agent-Harness

The Rust TUI is one expression surface for BFS + Hermes + Kimi. The system functions without it — BFS resolves, Hermes navigates, Kimi synthesizes, articles are written, all from the command line. The game wraps the knowledge management system in an experiential layer that does something the CLI cannot: it provokes emotional and spatial connection to raw data.

### Architecture Becomes Information

You walk into a room in the Old Kingdom (z=-1). The room is large — 34 volumes, packed shelves, colonnades of text. The architecture tells you: this was a major concern of Egyptian thought. Nine thousand texts survived from this era. The halls are vast because the writing was vast, because stone endures.

You descend to the First Intermediate Period (z=-2). The room is small. 150 texts for the entire era. Bare walls. A few ostraka scattered on the floor. The architecture tells you: almost nothing survived. The silence between inscriptions is information. The gap is architecture.

This is not a design choice. It is the corpus. Where the corpus is dense, the halls are dense. Where it is sparse, the walls are bare.

### The Rendering Pipeline

The TUI renders the corpus through a Rust engine built on ratatui and crossterm:

- **28 tile types** — structural (Wall, Floor, Door, Shaft, Column, Sconce, Railing, Lectern, Mirror, ClosetPanel), passage (CorridorFloor, VestibuleFloor, Staircase), and 14 Egyptian kit-of-parts artifacts (PapyrusTablet, WallInscription, ScrollPedestal, FalseDoor, TombAlcove, Vessel, Ostrakon, Stele, Sculpture, Coffin, RockFace, Pool, Brazier, Portal). Each tile carries passability, opacity, and light-emission properties.
- **Three-pass rendering** — walls first (with contiguous extension for solid `██` runs), non-wall tiles second, actors third (always at full brightness, never dimmed by lighting)
- **Torchlight system** — radial dimming from eight light source types: player torch (radius 12), braziers (8), sconces (5), shafts (3), portals (3), lecterns (2), pools (2). Ambient floor at 30% brightness. Color temperature shifts blend warm orange near braziers, cool blue near pools.
- **Particle effects** — six deterministic atmospheric particle types (Dust, Sand, PapyrusWisp, WaterDrop, EmberSpark, DeepChill) spawned by position hash and room metadata, not randomness. Particle density scales with room depth and content.
- **Tick-based animation** — brazier flicker (7 ticks), pool ripple (12 ticks), shaft pulse (16 ticks), portal glow cycle (20 ticks), player breathing (24 ticks), sconce torch (10 ticks). All deterministic from global tick counter.
- **Era palettes** — seven distinct color identities from sandstone gold (Old Kingdom) through syncretic teal (Greco-Roman), with district tint modifiers (temple warm gold, necropolis cool purple, domestic terracotta)
- **Kit-of-parts room composition** — room interior architecture (colonnades, coffin alcoves, scroll pedestals, stele gardens) generated from the text types assigned to that room by BFS. A `RoomComposition` struct tallies physical types from BFS metadata and drives deterministic placement.
- **HUD system** — cycles through four modes (Off → Standard → Full → ChatSidebar): minimap showing loaded room topology with player/NPC positions, compass with adjacent room names, z-level column showing current era, breadcrumb trail of recently visited rooms, and glyph legend. The chat sidebar mode provides a persistent conversation panel alongside the game view.
- **Atmospheric entry text** — on room entry, a brief evocative line appears based on room district, era, type, and volume count. "Incense residue lingers between the columns." "The false door faces west. Beyond it: silence." Derived from room metadata, not invented narrative.

### Surfacing Agent Work

The game's rendering pipeline is designed to make the agents' work visible and embodied:

- **Hermes tool calls stream in real time** — when the Librarian queries BFS, the player sees `⟐ room-texts 5,-2,-3 → 22 volumes fetched` in the chat panel. The agent's reasoning is not hidden behind a loading spinner. The player watches Hermes work.
- **Kimi's reasoning streams as live text** — when the Scribe synthesizes, the player watches Kimi's chain-of-thought unfold in dark cyan before the article appears. The two-phase transition (thinking → writing) is a UX feature that demonstrates Kimi's reasoning capabilities in situ.
- **The Hermes Librarian NPC is physically present** — it follows the player through rooms using a breadcrumb trail algorithm, faces toward the player, and adjusts its conversational register based on session depth (formal early, terse later). It is not a chat window. It is a character in the space, powered by Hermes.
- **The Kimi Scribe NPC appears when invoked** — a second character that follows the Librarian, visually distinct (cyan vs. magenta), representing the synthesis agent waiting to be called upon.

### The Egyptian Convergence

The Egyptian worldview predicts this architecture. The ontological principle — word = thing = body = building = cosmos — is not imposed on the system. It emerges from faithfulness to the corpus:

- The text IS the room (Law I: corpus defines the space)
- The agent IS the harness (Law II: same tools, same database)
- The room IS the corpus (the map cannot be wrong, only incomplete)
- The game IS the data system wrapped in experience

When the Egyptian said the spine IS the Djed pillar IS the cosmic axis, they were not speaking in metaphor. The preposition *m* means "to transform into," not "like." The body-temple-land-cosmos identity was not a literary device. It was how knowledge was organized — spatially, architecturally, by walking through it.

The Labyrinth is the Duat. The player walks through what was written.

---

## VIII. Architecture & Stack

```
┌─────────────────────────────────────────────────────────────────┐
│                     AEGYPTUS: THE LABYRINTH                     │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌──────────────┐    JSON-RPC     ┌──────────────────────────┐  │
│  │ stacks-engine│◄──────────────►│  BFS Server (Python)      │  │
│  │ (Rust TUI)   │   port 8081    │  FastAPI + JSON-RPC 2.0   │  │
│  │              │                │                            │  │
│  │  Rendering   │                │  ┌────────────────────┐   │  │
│  │  Navigation  │                │  │ Resolver (L3)      │   │  │
│  │  Interaction │                │  │ Topology Graph (L4)│   │  │
│  │  Agent UI    │                │  │ Index Service (L5) │   │  │
│  └──────┬───────┘                │  │ Agent Interface    │   │  │
│         │                        │  └────────────────────┘   │  │
│    FIFO Bridge                   │           │               │  │
│    (JSON-RPC)                    │           │ SQL           │  │
│         │                        │           ▼               │  │
│  ┌──────┴───────┐                │  ┌────────────────────┐   │  │
│  │ Hermes Agent │                │  │ Aegyptus Corpus    │   │  │
│  │  (Librarian) │                │  │ PostgreSQL         │   │  │
│  │ Carnice-27b  │                │  │ 77,683 texts       │   │  │
│  │ RTX 4090     │                │  │ 460,333 sentences  │   │  │
│  └──────────────┘                │  │ 35,052 lemmata     │   │  │
│                                  │  └────────────────────┘   │  │
│  ┌──────────────┐                └──────────────────────────┘  │
│  │ Kimi Scribe  │                                               │
│  │ K2-thinking  │  ◄── Nous Portal API (long-context synthesis) │
│  │ Nous Portal  │                                               │
│  └──────────────┘                                               │
└─────────────────────────────────────────────────────────────────┘
```

### Component Summary

| Component | Language | LOC | Purpose |
|-----------|----------|-----|---------|
| stacks-engine | Rust | 7,200 | TUI game engine — rendering, navigation, interaction, agent UI |
| bfs-client | Rust | 450 | JSON-RPC client for BFS server |
| hex2d-stacks | Rust | 1,300 | Hexagonal coordinate math (forked from dpc/hex2d-rs) |
| BFS Server | Python | 4,700 | Spatial resolver, topology graph, index service, agent interface |
| Aegyptus Corpus | Python | 33,700 | Import pipeline, entity resolution, CLI, API, Oracle tools |
| Librarian Shim | Python | 220 | FIFO bridge adapter for Hermes agent |
| Kimi Scribe | Python | 370 | Article synthesis subprocess |
| **Total** | | **~48,000** | |

### Key Dependencies & Credits

| Dependency | Source | License | Role |
|------------|--------|---------|------|
| hermes-agent v0.10.0 | [NousResearch/hermes-agent](https://github.com/NousResearch/hermes-agent) | MIT | Agent harness — skills, memory, session management |
| Kimi K2.6 | Moonshot AI via [Nous Portal](https://portal.nousresearch.com) | — | Long-context reasoning model for article synthesis |
| hex2d-rs | [dpc/hex2d-rs](https://github.com/dpc/hex2d-rs) | MIT | Hexagonal coordinate library (forked + extended with z-axis) |
| ratatui 0.28 | [ratatui-org/ratatui](https://github.com/ratatui-org/ratatui) | MIT | TUI rendering framework |
| crossterm 0.28 | Open source | MIT | Terminal I/O |
| tokio 1.x | Open source | MIT | Async runtime |

### Corpus Source Attribution

The Aegyptus corpus is assembled from open scholarly databases. We acknowledge and credit:

- **Thesaurus Linguae Aegyptiae (TLA)** — Berlin-Brandenburgische Akademie der Wissenschaften
- **ORAEC** — Open Richly Annotated Egyptian Corpus
- **AES** — Ancient Egyptian Sentences project
- **AED-TEI** — Altägyptisches Wörterbuch, Berlin
- **Coptic Scriptorium** — Georgetown University / University of the Pacific (CC BY 4.0)
- **Trismegistos** — KU Leuven
- **papyri.info / Integrating Digital Papyrology** — Duke/Columbia (CC BY 3.0)
- **Nederhof / PhilologEg** — University of St Andrews
- **RAMSES** — Université de Liège (CC BY-NC-SA)
- **Metropolitan Museum of Art** — Open Access program

---

## IX. The Three Laws

These are not design principles. They are identity constraints. Violate one and it becomes a different project.

**Law I. The corpus IS the world.**
The map is not authored. Rooms emerge from co-occurrence patterns in 3,000-year-old writing. There is no `chambers.json` that a designer wrote. The corpus generates the architecture. The map cannot be wrong, only incomplete.

**Law II. The agent IS the harness.**
The Librarian does not consult a simplified search tool written for the game. It calls `room-texts 5,-2,-3` — the same command a researcher uses from the terminal. Same tools. Same database. Same power. The agent is bounded only by what the corpus contains and the CLI exposes.

**Law III. The texts carry their provenance.**
Every inscription on a shelf is a real `canonical_id`. Every translation carries its register — `verified` (peer-reviewed scholarly translation), `unreviewed` (community contribution), or `ml_draft` (synthetic translation by Gemma/Haiku, never claiming scholarly authority). The original transliteration is always available. We do not paraphrase. We do not generate text in the voice of ancient Egypt. The ancient Egyptians generated that text. We found it and marked how we read it.

---

*This is the Library of Alexandria — found, not rebuilt — and you can walk in it.*

---

**Built for the [Nous Research](https://nousresearch.com) × [Kimi](https://platform.moonshot.ai) Creative Hackathon, April–May 2026**

### Hermes Track

The Librarian demonstrates Hermes agent as an **architectural inhabitant** of a spatial data system — not a chatbot, not an assistant, but a persistent agent that lives inside the Babel File System. Hermes features used: **skill system** (BFS query tools), **profile isolation** (`~/.hermes/profiles/stacks`), **TUI gateway** (JSON-RPC streaming via FIFO bridge — 220-line shim, zero changes to Hermes core), **session persistence and resume** (SQLite-backed sessions with full conversation recall — the pause menu lets players browse and resume prior librarian conversations, loading the complete message history back into the chat UI and agent context), **model-agnostic providers** (local Carnice-27b via llama-server), **restricted toolsets** (terminal-only for safety), **streaming event protocol** (real-time tool call visibility in game UI), **context compression** (token budget tracking for sustained exploration). The Librarian is architecturally necessary for navigating 77,683 texts across a labyrinth of over 2,000 rooms — and it operates with or without the game running.

### Kimi Track

The Scribe demonstrates Kimi as a **scholarly synthesis engine** — producing cited research drafts from 3,000-year-old primary sources that have never been read together before. Kimi capabilities used: **long context window** (simultaneous reasoning over 10–30 multilingual volumes), **extended thinking / reasoning tokens** (visible chain-of-thought via `delta.reasoning` streamed live in the game UI), **multilingual comprehension** (Egyptian transliteration + English + German in a single reasoning pass), **Nous Portal API** (OpenAI-compatible streaming with three-model fallback chain). The Scribe is not interchangeable with a standard-context model. The synthesis task — cited scholarly drafts from real ancient texts — specifically requires Kimi's long-attention reasoning capabilities. The Mouth-Opening: giving voice to what has been silent for 3,000 years.
