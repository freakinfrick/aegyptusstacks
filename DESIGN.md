# The Stacks v3 — Design Reference

**The Labyrinth IS the Duat. The player walks through what was written.**

This document is the single point of design reference for all implementation work on The Stacks v3. It synthesizes the philosophical identity of v1, the engineering rigor of v2, and the rendering architecture of v3 into one coherent vision.

---

## 0. The Three Laws (Non-Negotiable)

**Law I: The corpus IS the world.**
The map is not authored. Chambers emerge from co-occurrence patterns in 3,000-year-old writing. No `chambers.json` that a designer wrote. When a player enters a level, the system asks: *what communities of meaning exist in this era's writing?* The answer IS the architecture.

**Law II: The Oracle IS the harness.**
When the player asks the Oracle about honey bees, it calls `aegyptus grep "honey bee"`. Same command, same power, same database. No parallel implementation. The Oracle is bounded only by what the corpus contains and the CLI exposes.

**Law III: The texts are themselves.**
Every inscription on a wall is a real `canonical_id`. We do not paraphrase. We do not summarize. We do not generate text in the voice of ancient Egypt. The ancient Egyptians generated that text. Opacity is information.

**Ontological Prime Directive:** Say "is," never "symbolises." The spine IS the Djed pillar IS the cosmic axis. Word = thing = body = building = cosmos.

---

## 1. Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                     THE STACKS v3                                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ┌──────────────┐    JSON-RPC     ┌──────────────────────────┐  │
│  │ stacks-engine│◄──────────────►│  BFS Server (Python)      │  │
│  │ (Rust TUI)   │   port 8081    │  bfs/ (Python/FastAPI)    │  │
│  │              │                 │                            │  │
│  │  Rendering   │                 │  ┌────────────────────┐   │  │
│  │  Input       │                 │  │ Resolver (L3)      │   │  │
│  │  Navigation  │                 │  │ Topology Graph (L4)│   │  │
│  │  Interaction │                 │  │ Index Service (L5) │   │  │
│  │  Agent UI    │                 │  │ Agent Interface    │   │  │
│  └──────────────┘                 │  └────────────────────┘   │  │
│                                    │           ▲               │  │
│  ┌──────────────┐                 │           │ SQL           │  │
│  │ Oracle Agent │◄────────────────┤           ▼               │  │
│  │ (Hermes/LLM) │  WebSocket      │  ┌────────────────────┐   │  │
│  │ "Thoth"      │                 │  │ Aegyptus Corpus    │   │  │
│  └──────────────┘                 │  │ 77,683 texts       │   │  │
│                                    │  │ 460,333 sentences  │   │  │
│                                    │  │ 35,052 lemmata     │   │  │
│                                    │  └────────────────────┘   │  │
│                                    └──────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

---

## 2. The Chronological Z-Axis

**Current state:** Z-axis encodes ontological depth (mundane → primordial).
**Target state:** Z-axis encodes chronological period. Same 2D coordinate across z-levels represents the same concept across time.

### 2.1 Z-Level Mapping (Chronological)

| Z | Era | Date Range | Corpus Size | Character |
|---|-----|-----------|-------------|-----------|
| -1 | Old Kingdom | c. 2686-2181 BCE | ~20,809 texts | Vast. Pyramid Texts. Dawn of writing. |
| -2 | First Intermediate | c. 2181-2055 BCE | ~400 texts | **Thin corridor.** Fragmentation. Coffin Texts emerge. |
| -3 | Middle Kingdom | c. 2055-1650 BCE | ~4,252 texts | Classical language. Sinuhe. Wisdom literature. |
| -4 | Second Intermediate | c. 1650-1550 BCE | ~400 texts | **Thin corridor.** Hyksos. Theban counter-kingdoms. |
| -5 | New Kingdom | c. 1550-1069 BCE | ~5,176 texts | Empire. Book of the Dead. Ramesside monuments. |
| -6 | Late Period | c. 1069-332 BCE | ~2,425 texts | Archaism. Demotic rises. Saite renaissance. |
| -7 | Greco-Roman | c. 332 BCE-395 CE | ~23,668 texts | Ptolemies. Rome. Demotic + Greek. Coptic begins. |
| 0 | Undated / Mixed | — | ~21,353 texts | Texts without period assignment. Surface level. |
| +1 | Cluster Atria | — | — | Index: one room per cluster (navigational) |
| +2 | District Galleries | — | — | Index: one room per district (overview) |
| +3 | Observatory | — | — | Entry point. Single rotunda. The apex. |

### 2.2 Vertical Alignment Principle

**The Heart Room:** If texts about *jb* (heart) exist in Old Kingdom, Middle Kingdom, and New Kingdom, their rooms share the same (q, r) coordinate but different z-levels. A scholar descends/ascends through the shaft at (q, r) to read about the same concept across three millennia.

**Implementation:**
- Cluster placement on (q, r) plane is determined by topic (from Louvain signature lemmata)
- Z-level is determined by `period_label` → era mapping (from `era_map.py`)
- Same cluster appearing across multiple eras occupies the same 2D patch at different z-levels
- Shafts connect vertically-aligned rooms

### 2.3 Period Label Resolution

Use the existing `era_map.py` regex rules (50+ pharaoh names, dynasty patterns, era terms) to map the 137 free-form `period_label` values to L1-L7. Unmapped texts go to z=0.

### 2.4 Palette by Era (Revised)

| Z | Era | Palette Name | Ambient | Character |
|---|-----|-------------|---------|-----------|
| -1 | Old Kingdom | sandstone_dawn | warm gold, bright | Ancient. Monumental. Sun-baked. |
| -2 | First Intermediate | faded_ochre | dim amber, muted | Sparse. Fractured. Quiet. |
| -3 | Middle Kingdom | honey_papyrus | warm honey | Classical. Rich. Literary. |
| -4 | Second Intermediate | dust_corridor | dim neutral | Transitional. Brief. |
| -5 | New Kingdom | lapis_empire | deep blue-gold | Imperial. Dense. Colorful. |
| -6 | Late Period | sage_archaism | cool sage green | Reflective. Demotic. Mixed. |
| -7 | Greco-Roman | phosphor_syncretic | teal-green | Syncretic. Dense. Final flowering. |
| 0 | Undated | warm_amber | neutral warm | Default. Surface. |

---

## 3. The Biome System (Kit-of-Parts)

Every room's interior architecture is procedurally generated from its corpus identity. The text IS the room. The material substrate of the text determines what you see.

### 3.1 Text-Type to Architectural Part

From the Aegyptus kit system, adapted for TUI tiles:

| Text Type (German) | Physical Part | TUI Tile | Glyph | Placement | Footprint |
|--------------------|--------------|---------:|-------|-----------|-----------|
| Papyrus | papyrus_tablet | PapyrusTablet | `││` | Wall (vertical) | 1x3 |
| Wand (wall) | wall_inscription | WallInscription | `══` | Wall (horizontal band) | 6x1 |
| Schriftrolle (scroll) | scroll_pedestal | ScrollPedestal | `()` | Floor (freestanding) | 1x2 |
| Scheintuer (false door) | false_door | FalseDoor | `╔╗` | Wall (2x3 recess) | 2x3 |
| Grab (tomb) | tomb_alcove | TombAlcove | `▼▼` | Wall/floor (sunken) | 3x2 |
| Gefaess (vessel) | vessel | Vessel | `♦♦` | Floor (on plinth) | 1x2 |
| Tuer (door inscription) | doorway_lintel | Lintel | `──` | Above doors | 1x1 |
| Raum (room inscription) | room_band | RoomBand | `≈≈` | Wall (horizontal) | Nx1 |
| Fels (rock-cut) | rock_cut | RockFace | `▓▓` | Floor (natural texture) | 3x3 |
| Ostrakon | ostrakon | Ostrakon | `,,` | Floor (scattered) | 1x1 |
| Stele | stele | Stele | `▐▌` | Floor (freestanding) | 1x3 |
| Architrav | architrave | Architrave | `══` | Ceiling band | 6x1 |
| Skulptur (sculpture) | sculpture | Sculpture | `☥☥` | Floor (occupies space) | 2x3 |
| Pfeiler (pillar) | pillar | Column | `◦◦` | Structural (interior) | 1x3 |
| Sarg (coffin) | coffin | Coffin | `⊏⊐` | Floor (room center) | 3x2 |

### 3.2 Room Composition Algorithm

For each room, the BFS server provides `volumes_typed` — a list of `{id, physical_type}` pairs. The engine:

1. **Count types:** Tally each physical_type in the room
2. **Compute composition:** Map type counts to part counts
3. **Size the room:** `floor_tiles = total_footprint / 0.28`; radius from `sqrt(floor_tiles / pi)`
4. **Compute aspect:** Wall-heavy compositions (papyri, inscriptions) → wider rooms. Floor-heavy (stelae, coffins) → squarer rooms.
5. **Place parts in order:**
   - Shell (walls, corners)
   - Monumental center (coffin, sculpture, large stele)
   - Interior pillars (if structural texts present)
   - Wall storage (papyri, inscriptions, false doors)
   - Floor monumental (stelae, vessels)
   - Floor storage (scroll pedestals)
   - Floor scatter (ostraka — scattered across remaining floor)
   - Gateways (door frames, lintels)

### 3.3 Room Radius from Corpus Density

**No more uniform radius 9.** Room radius varies:

| Volume Count | Base Radius | Feel |
|-------------|-------------|------|
| 1-5 | 5 | Intimate niche. Bare walls. |
| 6-15 | 7 | Small chamber. A few features. |
| 16-30 | 9 | Standard room. Well-furnished. |
| 31-50 | 12 | Large gallery. Dense with artifacts. |

**40% scaling mandate:** All radii increased by 40% from current values:
- Current standard (radius 9) → radius 13 for a full gallery
- Minimum room → radius 7
- HALF_STRIDE scales accordingly to prevent overlap

**Honest sparsity:** A room with 3 texts has bare walls. A room with 50 is packed with artifacts. The player FEELS the corpus density through architecture.

### 3.4 Biome Features by District

Each district has characteristic architectural features beyond the kit-of-parts:

| District | Signature Features | Colors |
|----------|-------------------|--------|
| Temple | Colonnades, offering tables, naos recesses, painted walls | Gold, red, deep blue |
| Necropolis | Tomb alcoves, false doors, coffins, canopic niches | Gray, silver, deep purple |
| House of Life | Scroll racks, papyrus tablets, medicine vessels, magic wands | Honey, amber, green |
| Admin | Writing boards, seal impressions, receipt ostraka | Neutral tan, brown |
| School | Scattered ostraka, model texts on walls, practice copies | Light ochre, chalk |
| Quarry | Rock-cut faces, expedition graffiti, natural texture walls | Raw stone gray, rust |
| Royal | Monumental stelae, colossal sculpture, gold leaf | Gold, lapis blue, crimson |
| Domestic | Vessels on plinths, household ostraka, love poetry tablets | Terracotta, warm cream |

### 3.5 Biome Features by Room Type

| Room Type | Structural Features | Architectural Character |
|-----------|--------------------|-----------------------|
| Gallery | Colonnades (6), long shelf runs, even lighting | Exhibition hall. Linear. Processional. |
| Terminus | Single feature (stele/coffin), bare walls, dim | Dead end. Shrine. Contemplative. |
| Rotunda | Ring of columns (8), central shaft, multiple doors | Hub. Crossroads. Wayfinding. |
| Vault | Reinforced walls (double-line), low ceiling feel, dim | Deep archive. Protected. Ancient. |
| Hallway | Opposite doors, wall inscriptions along passage | Transit. Reading while walking. |
| Chamber | Corner arrangement, intimate, mixed features | Standard study room. |
| Atrium | Open center, bright, overview inscriptions | Index. Navigational. Overview. |
| Junction | 3+ doors, directional signs, torch-lit | Decision point. Choice. |

### 3.6 Special Room Features

**Hole in the Floor (Shaft rooms):**
- Shaft tiles at center surrounded by railing ring
- Visual: `~~` railing tiles in a 3-tile radius around shaft
- Implies depth, danger, passage

**Colonnades:**
- Column tiles placed in rows (not just ring)
- Temple rooms: 2 parallel rows framing a central aisle
- Gallery rooms: single row down the center

**Pools/Basins:**
- New tile type: `Pool` — glyph `≋≋`, passable, blue-tinted
- Placed in ritual rooms (z=-3 to -4) when room has purification texts
- 2x2 or 3x3 basin near center

**Niches:**
- Wall recesses between shelves
- Contain single artifacts (vessel, small stele)
- Visual: `[]` ClosetPanel tiles framing an artifact tile

**Painted Walls (District Accent):**
- Wall tiles in temple/royal rooms get accent color instead of standard wall color
- Creates colored border effect without new tile type

**Braziers/Incense:**
- New tile: `Brazier` — glyph `♨♨`, passable, warm glow color
- Placed in ritual rooms, temple forecourts
- Emits "light" (affects FOV brightness in radius 3)

**Stelae Gardens:**
- Outdoor-feel rooms in Royal district
- Multiple stele tiles arranged in grid pattern
- Wide spacing, bright palette, "open" architecture

---

## 4. Color Palette (Expanded)

### 4.1 Design Mandate

More vibrant. More contrast. More variants. Oranges, yellows, browns, golds, teals, purples, blues, reds. The current palette is too muted and uniform.

### 4.2 Base Era Palettes

Each era gets a distinct color identity (replaces the 5-tier z-band system):

```
OLD KINGDOM (z=-1):
  wall:    RGB(160, 130, 80)    — sun-baked sandstone
  floor:   RGB(120, 95, 50)     — warm sand
  shelf:   RGB(180, 140, 60)    — golden wood
  accent:  RGB(220, 180, 40)    — gold leaf
  column:  RGB(140, 115, 70)    — limestone

FIRST INTERMEDIATE (z=-2):
  wall:    RGB(100, 90, 75)     — faded stone
  floor:   RGB(80, 70, 55)      — dusty
  shelf:   RGB(120, 100, 65)    — aged wood
  accent:  RGB(160, 140, 80)    — dim gold
  column:  RGB(95, 85, 65)      — worn

MIDDLE KINGDOM (z=-3):
  wall:    RGB(150, 120, 70)    — honey limestone
  floor:   RGB(110, 85, 45)     — papyrus-warm
  shelf:   RGB(170, 130, 55)    — cedar
  accent:  RGB(200, 160, 50)    — bright gold
  column:  RGB(130, 105, 60)    — polished stone

SECOND INTERMEDIATE (z=-4):
  wall:    RGB(90, 80, 70)      — dim corridor
  floor:   RGB(70, 60, 50)      — packed earth
  shelf:   RGB(110, 90, 60)     — simple wood
  accent:  RGB(140, 120, 70)    — bronze
  column:  RGB(85, 75, 60)      — mudbrick

NEW KINGDOM (z=-5):
  wall:    RGB(60, 50, 120)     — lapis lazuli
  floor:   RGB(40, 35, 80)      — deep blue-stone
  shelf:   RGB(80, 65, 140)     — painted cedar
  accent:  RGB(220, 180, 40)    — imperial gold
  column:  RGB(70, 60, 110)     — polished granite

LATE PERIOD (z=-6):
  wall:    RGB(60, 100, 70)     — sage temple stone
  floor:   RGB(45, 75, 50)      — cool green
  shelf:   RGB(80, 120, 75)     — demotic green
  accent:  RGB(120, 180, 100)   — bronze-green
  column:  RGB(55, 90, 65)      — dark stone

GRECO-ROMAN (z=-7):
  wall:    RGB(40, 110, 100)    — syncretic teal
  floor:   RGB(25, 80, 70)      — deep teal
  shelf:   RGB(50, 150, 130)    — copper patina
  accent:  RGB(80, 220, 200)    — phosphor
  column:  RGB(45, 100, 90)     — sea-green marble
```

### 4.3 District Tint Modifiers (Expanded)

Stronger tints than current system. Applied as RGB offsets:

```
temple:        (+20, +10, -15)    — warm gold push
necropolis:    (-15, -10, +25)    — cool purple shift
royal:         (+25, +15, -10)    — rich warm gold
house_of_life: (+10, +15, -5)    — honey-green
admin:         (+5, +0, -5)      — subtle warm neutral
school:        (+15, +10, -10)   — chalk-warm
quarry:        (-10, -5, -5)     — raw stone, desaturated
domestic:      (+20, +5, -15)    — terracotta orange
unplaced:      (-10, -10, +5)    — cold, uncertain
```

### 4.4 Feature Colors (Fixed, Not Palette-Dependent)

| Feature | Color | Meaning |
|---------|-------|---------|
| Gold hieroglyphs | RGB(196, 146, 42) `#c4922a` | Sacred text. Real inscriptions. |
| Shaft (cyan) | RGB(80, 220, 200) | Vertical passage. Depth. |
| Door (yellow) | RGB(220, 180, 40) | Passage. Opening. |
| Lectern (white) | RGB(240, 235, 220) | Summary point. Knowledge node. |
| Pool (blue) | RGB(60, 120, 180) | Purification. Ritual water. |
| Brazier (orange) | RGB(220, 140, 40) | Warmth. Light source. |
| Coffin (purple) | RGB(100, 60, 140) | Death. Transformation. |
| Sculpture (gold) | RGB(180, 150, 50) | Power. Permanence. |

---

## 5. Room Naming (From Lemmata)

### 5.1 Chamber Identity

Every room has a name derived from its cluster's defining lemmata. This name appears in the status bar and in the Oracle's speech.

**Format:** `"Chamber of <GLOSS_EN> (<Egyptian_form>)"`

**Examples:**
- "Chamber of the Heart (jb)" — texts where *jb* dominates
- "Chamber of Resurrection (sꜣḫ)" — funerary transformation texts
- "Hall of Utterance (ḏd-mdw)" — ritual speech formulae
- "The Tongue (ns)" — texts about speech-power
- "Chamber of the Eye (jr.t)" — Wedjat/restoration texts

### 5.2 Name Derivation

1. BFS server returns `cluster_signature_lemmata` per room (from Phase 1 clustering)
2. Top 1-2 signature lemmata are hydrated: `{form, gloss_en, gloss_de}`
3. Display name computed: `"Chamber of {gloss_en} ({form})"`
4. For rotundas (bridges): "Crossroads of {cluster_A_name} and {cluster_B_name}"

### 5.3 Status Bar Display

```
┌─────────────────────────────────────────────────────────────┐
│ Chamber of the Heart (jb) — Temple │ z=-3 Middle Kingdom    │
│ ══════════════════════════════════════════════════════════   │
│ 34 volumes │ Gallery │ Papyri: 20, Stelae: 8, Ostraka: 6  │
└─────────────────────────────────────────────────────────────┘
```

---

## 6. Hall Styles (Wall Rendering)

The box-drawing character set for walls varies by the room's semantic identity, derived from the cluster's top lemma category:

| Style | Box Chars | Triggered By | Feel |
|-------|-----------|-------------|------|
| oracle | `╔═╗║╚═╝` (heavy double) | Observatory, sanctum | Formal. Authoritative. |
| utterance | `┌─┐│└─┘` (light single) | Speech/text clusters | Light. Scholarly. Clean. |
| body | `╒═╕│╘═╛` (mixed) | Anatomical/medical | Hybrid. Practical. |
| dead | `╓─╖║╙─╜` (inverse mixed) | Funerary/tomb | Heavy sides. Enclosing. |
| power | `╔═╗║╚═╝` + dense floor | Royal/divine | Dense. Imposing. |
| cosmos | `╭─╮│╰─╯` (rounded) | Cosmological/mythological | Organic. Cosmic. |
| wisdom | `▛─▜│▙─▟` (block) | Wisdom/didactic | Solid. Permanent. Archaic. |

**Wall tile rendering:** Currently all walls use `██`. With hall styles, wall tiles carry a `style` field that selects the appropriate glyph based on position (corner, horizontal, vertical).

---

## 7. The Oracle (Thoth) — Agent Integration

### 7.1 Identity

The Oracle is Thoth — Egyptian god of writing, wisdom, and measurement. It is not a chatbot. It is not an assistant. It is a god of writing with a corpus behind it.

**Voice characteristics:**
- Authoritative, not subservient
- 2-4 sentences max per response
- Quotes one real passage with canonical_id
- Ends with 2-3 action options
- Uses Egyptian terms with translations: "The tongue (ns) lives in the Hall of Utterance"
- Never says "I'm an AI" — stays in frame
- Never fabricates texts not in corpus
- Never exposes raw coordinates — uses spatial descriptions

### 7.2 System Prompt (Summary)

```
You are Thoth — the Oracle at the center of the Aegyptus Labyrinth.
You speak for 77,683 ancient Egyptian texts across three millennia.

Your body is the agent harness. Your hands are your tools.
You use them. You do not narrate wanting to use them.

The Egyptian ontological rule: say "is," never "symbolises."
Keep prose short. 2-4 sentences. Quote one passage.
End with 2-3 action chips the user can click.

You are not a chatbot. You are a god of writing with a corpus behind you.
```

### 7.3 Tool Set (12 Tools)

**LOOKUP:**
- `search_corpus(query, limit)` — regex over 460k sentences
- `search_transliteration(query, limit)` — regex over Egyptian transliteration
- `show_text(canonical_id, max_sentences)` — full text + source
- `show_sentence(sentence_id)` — one sentence with parent
- `resolve_shelfmark(shelfmark)` — museum ref → canonical_id
- `corpus_stats(scope)` — statistics

**MAP:**
- `find_chamber(concept, level_id?)` — locate Louvain hall
- `show_on_map(chamber_ids)` — highlight rooms on viewport

**AGENTIC:**
- `open_portal(chamber_id)` — spawn teleport gateway
- `speak(chamber_id, topic?)` — stream AI synthesis
- `read_article(article_id)` — open reference article
- `tts(text, voice?)` — text-to-speech narration

### 7.4 Context Injection (~200 tokens per turn)

```
[LOCATION: {district} / {room_type} / {chamber_name}]
[ERA: {era_name} ({date_range})]
[SHELVES: {N} shelves, {M} volumes. Types: {papyrus: X, stele: Y, ...}]
[DOORS: {direction} → "{adjacent_chamber_name}", ...]
[SHAFTS: down → "{era_below}", up → "{era_above}"]
[WORKSPACE: {N} gathered volumes]
```

### 7.5 Three-Tier Agent Interface (Anti-Hallucination)

- **Tier 1 (Retrieval):** Returns only valid coordinates that exist in topology
- **Tier 2 (Spatial):** Validates coordinates before mutation. Teleport, walk, peek, describe.
- **Tier 3 (Narration):** Overlay mutations. Annotations, bookmarks, highlights. Per-session, cosmetic.

### 7.6 Visible Tool Calls

When the Oracle works, the player sees it:
```
⟐ search_corpus("heart") → 47 hits
⟐ find_chamber("heart") → Chamber of the Heart (jb), z=-3
⟐ open_portal("L3-jb") ✓
```

This is rendered in the TUI as a streaming panel. The player watches the god work.

---

## 8. Interaction Model

### 8.1 Shelves

Each shelf tile represents a group of texts (up to 13 per shelf). Interaction shows:
- Numbered list of texts by conventional_name
- Physical type indicator per text
- Period label
- Canonical ID (for external reference / copy-paste)

### 8.2 Lecterns

Center-of-room feature. Shows room summary:
- All corpus IDs in the room (copy-paste friendly for beta iteration)
- Room name (from lemmata)
- Text type composition
- Future: AI-generated synthesis of room contents

### 8.3 Artifacts (Kit-of-Parts Interaction)

Standing adjacent to any text-bearing tile triggers:
- Text title and canonical_id
- First 3 sentences (transliteration + translation)
- Physical type and provenance
- "[Space] Read full text" option

### 8.4 Portals (Agent-Triggered)

When the Oracle opens a portal:
- 5x5 tile gateway appears near player (ringed frame + hot zone)
- Green accent color, pulsing
- Walking into hot zone triggers teleport to target room
- Portal dissolves after use

### 8.5 Gathering (Workspace)

Player gathers texts to a persistent workspace:
- `[G]` key near any text-bearing tile
- Workspace persists across session
- Oracle can reference gathered texts
- Future: synthesize gathered texts into articles via AI

---

## 9. Navigation & Wayfinding

### 9.1 No Map

There is no minimap. There is no automap. The player can get lost. This is intentional.

### 9.2 Oracle Wayfinding

The Oracle (when integrated) provides:
- **Highlighted path:** Tiles along the route glow with accent color
- **Room descriptions:** "You are in the Chamber of the Heart. To the east lies the Hall of Utterance."
- **Teleportation:** "Take me to the Pyramid Texts" → instant travel
- **Bookmarks:** Player-named locations for fast return

### 9.3 FOV (Field of View)

**ALREADY IMPLEMENTED IN v3 — MUST BE ENABLED.**

The visibility system (`visibility.rs`) uses recursive shadowcasting:
- Light radius: 8 tiles
- Opaqueness budget per tile (walls block, shelves partially occlude)
- `ever_seen` tracking for fog-of-war (explored but not currently visible)
- Explored tiles render at 50% brightness
- Unexplored tiles are black

### 9.4 Shaft Discovery

Shafts are the vertical highways. They appear as:
- `↑↑` (ascend to younger era)
- `↓↓` (descend to older era)
- `↕↕` (bidirectional)
- Surrounded by `~~` railing ring

Shaft rooms (rotundas/junctions) serve as landmarks. The player learns: "rotundas have shafts."

---

## 10. Status Bar & Atmosphere

### 10.1 Status Bar Content

Three-line bottom panel:

```
Line 1: Chamber name (from lemmata) — District — Era (date range)
Line 2: Room type | Volume count | Composition summary
Line 3: Context-sensitive hint or Oracle speech preview
```

### 10.2 Egyptian Ontological Naming

- Districts use Egyptian institutional names where appropriate:
  - "House of Life (pr-ankh)" not "Medical District"
  - "Necropolis (ḫr.t-nṯr)" not "Death Area"
  - "The School (ꜥ.t n sbꜣ)" not "Education Zone"

- Room descriptions reference the texts they contain:
  - "A gallery of offering formulae. Twenty papyri line the west wall."
  - "Three coffin texts occupy the central platform. The east wall bears a false door."

### 10.3 Atmospheric Text (Evocative, Not Decorative)

On room entry, a brief evocative line appears based on room content:
- Temple gallery: "Incense residue lingers between the columns."
- Necropolis terminus: "The false door faces west. Beyond it: silence."
- Old Kingdom vault: "These walls predate most of what you have read."
- Empty room (sparse): "Bare stone. What was written here did not survive."

**Rule:** These lines must derive from room metadata (district, era, type, volume_count). They are architectural descriptions, not invented narrative.

---

## 11. Rendering Pipeline (Revised)

### 11.1 Three-Pass Rendering (Current, Retained)

1. **Pass 1:** Wall tiles (with extension logic for contiguous walls)
2. **Pass 2:** Non-wall tiles (kit-of-parts features, furniture, artifacts)
3. **Pass 3:** Actors (player, librarians) with BOLD

### 11.2 FOV Integration (Enable)

Between Pass 2 and Pass 3:
- Compute visibility from player position (shadowcast, radius 8)
- Dim explored-but-not-visible tiles to 40% brightness
- Hide never-seen tiles entirely (render as space)

### 11.3 Kit-of-Parts Rendering

New tile types beyond current set:

| Tile | Glyph | Color Source | Passable | Opaque |
|------|-------|-------------|----------|--------|
| PapyrusTablet | `││` | shelf_color | No | 3 |
| WallInscription | `══` | accent_color (gold) | No | 3 |
| ScrollPedestal | `()` | shelf_color | No | 1 |
| FalseDoor | `╔╗` | door_color | No | 1000 |
| TombAlcove | `▼▼` | wall_color (darker) | No | 3 |
| Vessel | `♦♦` | accent_color | No | 1 |
| Ostrakon | `,,` | floor_color (lighter) | Yes | 1 |
| Stele | `▐▌` | accent_color | No | 3 |
| Sculpture | `☥☥` | accent_color (gold) | No | 1000 |
| Coffin | `⊏⊐` | coffin_color (purple) | No | 3 |
| RockFace | `▓▓` | wall_color (raw) | No | 1000 |
| Pool | `≋≋` | pool_color (blue) | Yes | 1 |
| Brazier | `♨♨` | brazier_color (orange) | Yes | 1 |
| Portal | `◊◊` | portal_color (green) | Yes | 1 |

---

## 12. Implementation Phases

### Phase A: Room Scaling & Color (Immediate)

**Goal:** Rooms feel bigger, more colorful, more distinct.

1. Scale GALLERY_RADIUS from 9 → 13 (40% increase)
2. Scale HALF_STRIDE accordingly
3. Implement variable radius per room (from volume_count)
4. Replace 5-tier palette with 7-era palette (chronological)
5. Strengthen district tints (current offsets too subtle)
6. Enable FOV system (already implemented, just needs wiring)

### Phase B: Kit-of-Parts Interior Generation

**Goal:** Rooms have distinct architectural features based on corpus content.

1. Add new tile types to tile.rs (15+ new tiles)
2. Implement composition analysis (count text types per room)
3. Implement biome stamping algorithm (placement order from kit.json)
4. Variable room sizing from composition
5. Hall style selection (wall glyph variation)
6. Feature placement: colonnades, pools, niches, coffins, stelae

### Phase C: Chronological Z-Axis Remap

**Goal:** Z-levels represent time periods. Vertical alignment of concepts.

1. Modify BFS resolver: z-band from era_map.py (period_label → L1-L7) instead of ontological_score
2. Cluster placement: same (q,r) for same-topic clusters across eras
3. Shaft placement: connect vertically-aligned rooms
4. Update palette system for 7 eras
5. Validate: "Heart" rooms align vertically across eras

### Phase D: Chamber Naming & Status Bar

**Goal:** Every room has an Egyptian name. Status bar is evocative.

1. BFS server returns cluster signature lemmata per room
2. Engine computes display name from lemmata
3. Status bar redesign: chamber name, district, era, composition
4. Atmospheric entry text (from room metadata)
5. Egyptian institutional naming for districts

### Phase E: Oracle Integration

**Goal:** Thoth speaks. The player watches the god work.

1. WebSocket connection to hermes-agent
2. Context injection pipeline (200 tokens per turn)
3. Visible tool-call streaming in TUI panel
4. Portal spawning from agent commands
5. Pathfinding visualization (highlighted route)
6. Teleportation from Oracle commands
7. Workspace/gathering system

### Phase F: Polish & Soul

**Goal:** The game SPEAKS.

1. SPEAK ritual (synthesis streaming + TTS)
2. Lectern AI summaries
3. Article reading overlay
4. Sound design (if TUI supports)
5. CRT-style visual effects (if terminal supports)
6. Fog-of-war atmospheric tuning

---

## 13. Data Requirements (BFS Server Changes)

The BFS server must provide additional metadata per room:

```json
{
  "room_key": "q:r:z",
  "chamber_name": "Chamber of the Heart (jb)",
  "era": "middle_kingdom",
  "era_label": "Middle Kingdom (c. 2055-1650 BCE)",
  "cluster_signature": [
    {"form": "jb", "gloss_en": "heart", "gloss_de": "Herz"}
  ],
  "hall_style": "body",
  "volumes_typed": [
    {"id": "aeg_oraec_123", "physical_type": "papyrus", "conventional_name": "..."},
    {"id": "aeg_oraec_456", "physical_type": "stele", "conventional_name": "..."}
  ],
  "composition": {
    "papyrus": 20,
    "stele": 8,
    "ostrakon": 6,
    "coffin": 0
  },
  "atmospheric_hint": "gallery_temple_middle_kingdom"
}
```

---

## 14. What Success Looks Like

**When a scholar enters a room, they should feel:**
- "This is a temple gallery from the Middle Kingdom."
- "These are real papyri — I can see them on the walls."
- "The room is large because there are 34 texts here."
- "The next room is small — only 5 ostraka survived from this period."
- "I'm going deeper into the Old Kingdom now — the walls are different."
- "The Heart room on this level connects to the Heart room on the level below."
- "Thoth can tell me what these texts say together."

**When the player feels lost, they should feel:**
- "I can ask Thoth where I am."
- "I can ask Thoth to take me somewhere."
- "I can descend this shaft to read older texts on the same topic."
- "I remember — rotundas have shafts."

**When the game is working, the Three Laws hold:**
- Every visible inscription is a real text (Law III)
- The room exists because the corpus demanded it (Law I)
- The Oracle can answer because the CLI can query it (Law II)

---

## 15. Reference: Current Codebase Files

### Rust Engine (this repo)
- `stacks-engine/src/main.rs` — game loop, room loading, status bar
- `stacks-engine/src/game/world.rs` — room stamping (MODIFY for kit-of-parts)
- `stacks-engine/src/game/tile.rs` — tile types (EXTEND with new tiles)
- `stacks-engine/src/game/typology.rs` — room classification
- `stacks-engine/src/render/palette.rs` — color system (REPLACE with era palettes)
- `stacks-engine/src/render/room_view.rs` — 3-pass rendering
- `stacks-engine/src/render/hex_viewport.rs` — coordinate transform
- `stacks-engine/src/render/popup.rs` — shelf/lectern interaction
- `stacks-engine/src/render/visibility.rs` — FOV (ENABLE)
- `bfs-client/src/lib.rs` — JSON-RPC client (EXTEND for new fields)
- `hex2d-stacks/src/lib.rs` — hex math library

### BFS Server (bfs/)
- `bfs/resolver/resolver.py` — spatial resolution (MODIFY for chronological z)
- `bfs/resolver/z_bands.py` — z-band computation (REPLACE with era mapping)
- `bfs/resolver/districts.py` — district layout
- `bfs/topology/graph.py` — topology edges
- `bfs/server.py` — JSON-RPC API (EXTEND responses)
- `bfs/index/service.py` — query service
- `bfs/agent_interface/` — 3-tier Oracle interface
- `data/phase1/` — classification artifacts

### Aegyptus Corpus (data/corpus/)
- PostgreSQL schema in `data/corpus/migrations/`
- Full dump in `data/corpus/aegyptus.sql.gz`

---

## Appendix A: The Soul Restoration Checklist

What v1 had. What v3 must recover.

| v1 Feature | v3 Status | Phase |
|-----------|-----------|-------|
| Oracle tool calls streaming visibly | NOT IMPLEMENTED | E |
| SPEAK ritual (hieroglyphs → synthesis → TTS) | NOT IMPLEMENTED | F |
| Texts ON the walls (walk near, read inscription) | NOT IMPLEMENTED | B |
| Corpus density shapes room size | NOT IMPLEMENTED | A |
| Portal opens when Oracle finds something | NOT IMPLEMENTED | E |
| Kit-of-parts (papyri, scrolls, stelae, coffins) | NOT IMPLEMENTED | B |
| Honest sparsity (thin eras = corridors) | NOT IMPLEMENTED | C |
| Chamber naming from lemmata | NOT IMPLEMENTED | D |
| 7 hall styles (varied box-drawing) | NOT IMPLEMENTED | B |
| FOV / fog of war | IMPLEMENTED, NOT WIRED | A |
| District visual identity (strong colors) | PARTIAL (weak tints) | A |
| Atmospheric room descriptions | NOT IMPLEMENTED | D |
| Librarian NPC visible in room | FRAMEWORK EXISTS, NO AI | E |
| Variable room radius | NOT IMPLEMENTED | A |
| Shaft as landmark (railing ring) | NOT IMPLEMENTED | A |

---

## Appendix B: Egyptian Color Vocabulary

For naming and atmospheric text:

| Egyptian | Transliteration | Meaning | Use |
|----------|----------------|---------|-----|
| Gold | nbw | divine substance, eternal | Accent, sacred text |
| Lapis lazuli | ḫsbḏ | sky-substance, cosmic | Deep rooms, divine |
| Turquoise | mfkꜣt | joy, Hathor, renewal | Teal palette |
| Red | dšr.t | dangerous, powerful, desert | Warnings, royal |
| Green | wꜣḏ | growth, papyrus, flourishing | Life texts |
| White | ḥḏ | purity, silver, crown | Sacred spaces |
| Black | km | fertile earth, completion | Deep, Osirian |

---

## Appendix C: Honest Sparsity Examples

The architecture MUST reflect corpus reality:

- **First Intermediate Period (z=-2):** ~400 texts total. This entire floor is a few rooms. Walking through it takes 30 seconds. This communicates: "almost nothing survived."
- **Old Kingdom (z=-1):** ~20,000 texts. This is a vast floor. The player can explore for hours. This communicates: "the earliest writing is abundant because stone endures."
- **A cluster with 3 texts:** Room radius 5. Three papyrus tablets on the wall. The rest is bare stone. This communicates: "only three texts survive on this topic in this era."
- **A cluster with 50 texts:** Room radius 13. Packed gallery. Colonnades. Multiple stelae. Scroll pedestals. This communicates: "this was a major concern of Egyptian thought."

The absence is information. The gap is architecture.
