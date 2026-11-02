use hex2d_stacks::Coordinate;
use ratatui::style::Color;
use std::collections::HashMap;

use crate::game::tile::Tile;
use crate::game::world::WorldTile;
use super::palette::Palette;

// ── Hex directions (flat-top axial) ────────────────────────────────

const HEX_DIRS: [(i32, i32); 6] = [
    (0, -1),  // 0: NW  (y decreases — "upward")
    (1, -1),  // 1: NE  (y decreases — "upward")
    (1, 0),   // 2: E
    (0, 1),   // 3: SW  (y increases — "downward")
    (-1, 1),  // 4: SE  (y increases — "downward")
    (-1, 0),  // 5: W
];

/// Bias direction sets
const DOWNWARD_DIRS: [usize; 2] = [3, 4]; // Sand falls
const UPWARD_DIRS: [usize; 2] = [0, 1];   // Embers rise

// ── Types ──────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleKind {
    Dust,        // '.' dim gray, z <= -3
    Sand,        // ',' ochre, near shafts
    PapyrusWisp, // '~' pale gold, volume >= 40
    WaterDrop,   // '·' blue, near pools (stationary, blinks)
    EmberSpark,  // '*' orange, near braziers
    DeepChill,   // '\'' pale cyan, z <= -6
}

impl ParticleKind {
    /// Tick divisor controlling movement speed (higher = slower).
    fn speed(self) -> u64 {
        match self {
            ParticleKind::Dust => 15,
            ParticleKind::Sand => 8,
            ParticleKind::PapyrusWisp => 20,
            ParticleKind::WaterDrop => 1, // stationary
            ParticleKind::EmberSpark => 4,
            ParticleKind::DeepChill => 25,
        }
    }

    /// Default lifetime in ticks.
    fn default_lifetime(self) -> u16 {
        match self {
            ParticleKind::Dust => 50,
            ParticleKind::Sand => 25,
            ParticleKind::PapyrusWisp => 60,
            ParticleKind::WaterDrop => 20,
            ParticleKind::EmberSpark => 8,
            ParticleKind::DeepChill => 70,
        }
    }
}

pub struct Particle {
    pub pos: Coordinate<i32>,
    pub kind: ParticleKind,
    pub seed: u32,
    pub lifetime: u16,
    pub born_tick: u64,
}

pub struct RoomParticles {
    pub particles: Vec<Particle>,
    pub target_density: u8,
    pub active_kinds: Vec<ParticleKind>,
    pub room_coord: [i32; 3],
    /// Floor tiles in this room (for spawning)
    floor_tiles: Vec<Coordinate<i32>>,
    /// Trigger tiles: (position, trigger kind)
    trigger_tiles: Vec<(Coordinate<i32>, ParticleKind)>,
    /// Round-robin index for spawning kinds
    spawn_cursor: usize,
}

// ── Helpers ────────────────────────────────────────────────────────

fn hex_distance(a: Coordinate<i32>, b: Coordinate<i32>) -> i32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    let dz = -(dx + dy);
    dx.abs().max(dy.abs()).max(dz.abs())
}

fn coord_hash(c: Coordinate<i32>) -> u32 {
    let x = c.x as u32;
    let y = c.y as u32;
    x.wrapping_mul(2654435761).wrapping_add(y.wrapping_mul(2246822519))
}

fn spawn_near(
    trigger_pos: Coordinate<i32>,
    floor_tiles: &[Coordinate<i32>],
    seed: u32,
) -> Option<Coordinate<i32>> {
    let candidates: Vec<_> = floor_tiles
        .iter()
        .filter(|ft| hex_distance(**ft, trigger_pos) <= 2)
        .collect();
    if candidates.is_empty() {
        return None;
    }
    let idx = seed as usize % candidates.len();
    Some(*candidates[idx])
}

fn spawn_on_floor(floor_tiles: &[Coordinate<i32>], seed: u32) -> Option<Coordinate<i32>> {
    if floor_tiles.is_empty() {
        return None;
    }
    let idx = seed as usize % floor_tiles.len();
    Some(floor_tiles[idx])
}

/// Scale an RGB color by pct/100.
fn scale_color(c: Color, pct: u8) -> Color {
    match c {
        Color::Rgb(r, g, b) => Color::Rgb(
            (r as u16 * pct as u16 / 100) as u8,
            (g as u16 * pct as u16 / 100) as u8,
            (b as u16 * pct as u16 / 100) as u8,
        ),
        other => other,
    }
}

// ── Public API ─────────────────────────────────────────────────────

/// Initialize particle state for a room.
///
/// * `room_coord` — [Q, R, Z] of the room
/// * `tiles` — all world tiles (we filter by room_coord)
/// * `z` — z-level (same as room_coord[2])
/// * `_district` — district string (reserved for future biome tinting)
/// * `volume_count` — number of text-bearing fixtures in the room
pub fn init_room_particles(
    room_coord: [i32; 3],
    tiles: &HashMap<Coordinate<i32>, WorldTile>,
    z: i32,
    _district: &str,
    volume_count: usize,
) -> RoomParticles {
    let mut floor_tiles = Vec::new();
    let mut trigger_tiles: Vec<(Coordinate<i32>, ParticleKind)> = Vec::new();
    let mut shaft_count = 0u32;
    let mut brazier_count = 0u32;
    let mut has_pool = false;

    // Scan tiles belonging to this room
    for (coord, wt) in tiles.iter() {
        if wt.room_coord != room_coord {
            continue;
        }
        if wt.tile.is_passable() {
            floor_tiles.push(*coord);
        }
        match wt.tile {
            Tile::Shaft(_) => {
                shaft_count += 1;
                trigger_tiles.push((*coord, ParticleKind::Sand));
            }
            Tile::Pool => {
                has_pool = true;
                trigger_tiles.push((*coord, ParticleKind::WaterDrop));
            }
            Tile::Brazier => {
                brazier_count += 1;
                trigger_tiles.push((*coord, ParticleKind::EmberSpark));
            }
            _ => {}
        }
    }

    // Determine active kinds and per-kind density
    let mut active_kinds = Vec::new();
    let mut total_density: u8 = 0;

    // z <= -3 → Dust
    if z <= -3 {
        let d: u8 = if z <= -5 { 3 } else { 2 };
        for _ in 0..d {
            active_kinds.push(ParticleKind::Dust);
        }
        total_density += d;
    }

    // Shaft → Sand
    if shaft_count > 0 {
        let d: u8 = if shaft_count > 1 { 2 } else { 1 };
        for _ in 0..d {
            active_kinds.push(ParticleKind::Sand);
        }
        total_density += d;
    }

    // volume_count >= 40 → PapyrusWisp
    if volume_count >= 40 {
        active_kinds.push(ParticleKind::PapyrusWisp);
        total_density += 1;
    }

    // Pool → WaterDrop
    if has_pool {
        active_kinds.push(ParticleKind::WaterDrop);
        total_density += 1;
    }

    // Brazier → EmberSpark
    if brazier_count > 0 {
        let d: u8 = if brazier_count > 1 { 2 } else { 1 };
        for _ in 0..d {
            active_kinds.push(ParticleKind::EmberSpark);
        }
        total_density += d;
    }

    // z <= -6 → DeepChill
    if z <= -6 {
        active_kinds.push(ParticleKind::DeepChill);
        total_density += 1;
    }

    // Cap at 4
    let target_density = total_density.min(4);

    // Deduplicate active_kinds to unique set for round-robin
    let mut unique_kinds = Vec::new();
    for k in &active_kinds {
        if !unique_kinds.contains(k) {
            unique_kinds.push(*k);
        }
    }

    // Spawn initial particles up to target_density
    let mut particles = Vec::new();
    let mut cursor = 0usize;
    for i in 0..target_density {
        if unique_kinds.is_empty() {
            break;
        }
        let kind = unique_kinds[cursor % unique_kinds.len()];
        cursor += 1;
        let seed = coord_hash(Coordinate::new(room_coord[0], room_coord[1]))
            .wrapping_add(i as u32 * 7919);
        let pos = spawn_particle_pos(kind, &trigger_tiles, &floor_tiles, seed);
        if let Some(pos) = pos {
            particles.push(Particle {
                pos,
                kind,
                seed,
                lifetime: kind.default_lifetime(),
                born_tick: 0,
            });
        }
    }

    RoomParticles {
        particles,
        target_density,
        active_kinds: unique_kinds,
        room_coord,
        floor_tiles,
        trigger_tiles,
        spawn_cursor: cursor,
    }
}

/// Pick a spawn position for a particle kind.
fn spawn_particle_pos(
    kind: ParticleKind,
    trigger_tiles: &[(Coordinate<i32>, ParticleKind)],
    floor_tiles: &[Coordinate<i32>],
    seed: u32,
) -> Option<Coordinate<i32>> {
    match kind {
        ParticleKind::Sand | ParticleKind::WaterDrop | ParticleKind::EmberSpark => {
            // Find a trigger tile matching this kind
            let triggers: Vec<_> = trigger_tiles
                .iter()
                .filter(|(_, k)| *k == kind)
                .map(|(c, _)| *c)
                .collect();
            if let Some(&trig) = triggers.get(seed as usize % triggers.len().max(1)) {
                spawn_near(trig, floor_tiles, seed)
            } else {
                spawn_on_floor(floor_tiles, seed)
            }
        }
        ParticleKind::Dust | ParticleKind::PapyrusWisp | ParticleKind::DeepChill => {
            spawn_on_floor(floor_tiles, seed)
        }
    }
}

/// Advance particle simulation by one tick.
pub fn tick_particles(
    rp: &mut RoomParticles,
    tiles: &HashMap<Coordinate<i32>, WorldTile>,
    tick: u64,
) {
    // Decrement lifetime & move
    for p in rp.particles.iter_mut() {
        if p.lifetime == 0 {
            continue;
        }
        p.lifetime = p.lifetime.saturating_sub(1);

        // WaterDrop is stationary
        if p.kind == ParticleKind::WaterDrop {
            continue;
        }

        let speed = p.kind.speed();
        if speed == 0 || tick % speed != 0 {
            continue;
        }

        let dir_index = compute_direction(p, tick);
        let (dx, dy) = HEX_DIRS[dir_index];
        let candidate = Coordinate::new(p.pos.x + dx, p.pos.y + dy);

        // Move only if candidate is passable
        if let Some(wt) = tiles.get(&candidate) {
            if wt.tile.is_passable() {
                p.pos = candidate;
            }
        }
    }

    // Remove expired
    rp.particles.retain(|p| p.lifetime > 0);

    // Spawn if under target_density
    if (rp.particles.len() as u8) < rp.target_density && !rp.active_kinds.is_empty() {
        let kind = rp.active_kinds[rp.spawn_cursor % rp.active_kinds.len()];
        rp.spawn_cursor = rp.spawn_cursor.wrapping_add(1);
        let seed = coord_hash(Coordinate::new(rp.room_coord[0], rp.room_coord[1]))
            .wrapping_add(tick as u32)
            .wrapping_mul(2654435761);
        if let Some(pos) = spawn_particle_pos(kind, &rp.trigger_tiles, &rp.floor_tiles, seed) {
            rp.particles.push(Particle {
                pos,
                kind,
                seed,
                lifetime: kind.default_lifetime(),
                born_tick: tick,
            });
        }
    }
}

/// Compute movement direction for a particle at a given tick.
fn compute_direction(p: &Particle, tick: u64) -> usize {
    let speed = p.kind.speed();
    let phase = p.seed as u64 + tick / speed;

    match p.kind {
        ParticleKind::Sand => {
            let bias_check = (p.seed.wrapping_mul(tick as u32 + 7)) % 10;
            if bias_check < 6 {
                // 60% downward bias
                DOWNWARD_DIRS[(phase % 2) as usize]
            } else {
                (phase % 6) as usize
            }
        }
        ParticleKind::EmberSpark => {
            let bias_check = (p.seed.wrapping_mul(tick as u32 + 7)) % 10;
            if bias_check < 6 {
                // 60% upward bias
                UPWARD_DIRS[(phase % 2) as usize]
            } else {
                (phase % 6) as usize
            }
        }
        _ => (phase % 6) as usize,
    }
}

/// Returns (glyph, color) for rendering a particle.
pub fn particle_glyph<'a>(particle: &Particle, tick: u64, palette: &Palette) -> (&'a str, Color) {
    match particle.kind {
        ParticleKind::Dust => (".", scale_color(palette.wall_color(), 30)),
        ParticleKind::Sand => (",", scale_color(palette.floor_color(), 60)),
        ParticleKind::PapyrusWisp => ("~", scale_color(palette.papyrus_color(), 40)),
        ParticleKind::WaterDrop => {
            if (tick.wrapping_sub(particle.born_tick)) % 30 >= 15 {
                (" ", Color::Reset)
            } else {
                ("\u{00b7}", scale_color(palette.pool_color(), 70))
            }
        }
        ParticleKind::EmberSpark => ("*", palette.brazier_color()),
        ParticleKind::DeepChill => ("'", scale_color(palette.shaft_color(), 20)),
    }
}

/// Whether a particle should be drawn this tick.
pub fn particle_visible(particle: &Particle, tick: u64) -> bool {
    if particle.lifetime == 0 {
        return false;
    }
    match particle.kind {
        ParticleKind::WaterDrop => {
            (tick.wrapping_sub(particle.born_tick)) % 30 < 15
        }
        _ => true,
    }
}
