//! Gather vortex animation — visual feedback when volumes are collected.
//!
//! When a volume is gathered (manually or by Hermes), the fixture tile
//! briefly swirls like a miniature portal, then the particle spirals
//! around the room — contracting through rings r=3 → r=2 → r=1 before
//! absorbing into the player. Hermes gathers do a full orbit at r=1.
//! Toast fires after absorption.
//!
//! All glyphs use unicode block quadrants for visual coherence with
//! the hex tile renderer. This is a rendering overlay — no tile mutation.

use hex2d_stacks::{Coordinate, Direction, Spin};
use ratatui::style::Color;

use crate::render::hud::glyph_for_physical_type;

// ── Block-quad glyph tables ───────────────────────────────────────

/// Portal-style swirl frames — 8-frame clockwise rotation.
const SWIRL: [&str; 8] = [
    "▀▀", "▜▝", "▐▐", "▗▟", "▄▄", "▖▙", "▌▌", "▘▛",
];

/// Leading particle sparkle — rapid block-quad shimmer.
/// Dense patterns that flicker to create a twinkling effect.
const SPARK: [&str; 8] = [
    "▜▛", "▚▞", "▙▟", "▞▚",
    "▛▜", "▞▚", "▟▙", "▚▞",
];

/// Trail glyphs — progressively sparser block patterns.
const TRAIL: [&str; 6] = [
    "▚▞",  // dense sparkle (closest to leader)
    "▘▗",  // diagonal corners
    "▝▖",  // opposite diagonal corners
    "▘ ",  // single corner
    " ▖",  // single corner far
    "  ",  // ghost (barely visible via color)
];

/// Burst glyphs — expanding starburst patterns for absorb phase.
const BURST: [&str; 6] = [
    "██", "▓▓", "▒▒", "░░", "▘▗", "  ",
];

/// Neighbor sparkle motes — sparse glints around the leader.
const MOTES: [&str; 8] = [
    "▘ ", " ▝", "▗ ", " ▖",
    " ▘", "▝ ", " ▗", "▖ ",
];

/// Color palette the particle cycles through.
const PALETTE: [(u8, u8, u8); 8] = [
    (255, 220, 120), // warm gold
    (255, 180, 80),  // amber
    (240, 140, 160), // rose
    (200, 120, 220), // orchid
    (140, 160, 255), // periwinkle
    (100, 220, 220), // cyan
    (120, 230, 160), // seafoam
    (220, 240, 130), // chartreuse
];

// ── Timing constants ──────────────────────────────────────────────

const SWIRL_SPEED: u64 = 3;
const SWIRL_DURATION: u64 = 16;
const SPIRAL_STEP_TICKS: u64 = 4;
const ABSORB_DURATION: u64 = 10;
const TRAIL_LEN: usize = 5;

// ── Types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VortexPhase {
    FixtureSwirl,
    RoomSwirl,
    Absorb,
}

#[derive(Clone, Debug)]
pub struct DeferredToast {
    pub name: String,
    pub physical_type: String,
}

pub enum VortexStatus {
    Running,
    Complete(DeferredToast),
}

pub struct GatherVortex {
    pub source: Coordinate<i32>,
    pub room_coord: [i32; 3],
    pub phase: VortexPhase,
    phase_start_tick: u64,
    hermes: bool,
    spiral_path: Vec<Coordinate<i32>>,
    spiral_step: usize,
    toast: DeferredToast,
    tint: Color,
}

impl GatherVortex {
    pub fn new_player(
        source: Coordinate<i32>,
        player_pos: Coordinate<i32>,
        room_coord: [i32; 3],
        name: String,
        physical_type: String,
        tick: u64,
    ) -> Self {
        let (_, tint) = glyph_for_physical_type(&physical_type);
        GatherVortex {
            source,
            room_coord,
            phase: VortexPhase::FixtureSwirl,
            phase_start_tick: tick,
            hermes: false,
            spiral_path: compute_spiral_path(source, player_pos, false),
            spiral_step: 0,
            toast: DeferredToast { name, physical_type },
            tint: brighten(tint),
        }
    }

    pub fn new_hermes(
        source: Coordinate<i32>,
        player_pos: Coordinate<i32>,
        room_coord: [i32; 3],
        name: String,
        physical_type: String,
        tick: u64,
    ) -> Self {
        let (_, tint) = glyph_for_physical_type(&physical_type);
        GatherVortex {
            source,
            room_coord,
            phase: VortexPhase::FixtureSwirl,
            phase_start_tick: tick,
            hermes: true,
            spiral_path: compute_spiral_path(source, player_pos, true),
            spiral_step: 0,
            toast: DeferredToast { name, physical_type },
            tint: brighten(tint),
        }
    }

    pub fn tick(&mut self, player_pos: Coordinate<i32>, tick: u64) -> VortexStatus {
        let elapsed = tick.saturating_sub(self.phase_start_tick);
        match self.phase {
            VortexPhase::FixtureSwirl => {
                if elapsed >= SWIRL_DURATION {
                    self.phase = VortexPhase::RoomSwirl;
                    self.phase_start_tick = tick;
                    self.spiral_step = 0;
                    self.spiral_path = compute_spiral_path(self.source, player_pos, self.hermes);
                }
                VortexStatus::Running
            }
            VortexPhase::RoomSwirl => {
                if elapsed > 0 && elapsed % SPIRAL_STEP_TICKS == 0 {
                    self.spiral_step += 1;
                }
                if self.spiral_step >= self.spiral_path.len() {
                    self.phase = VortexPhase::Absorb;
                    self.phase_start_tick = tick;
                }
                VortexStatus::Running
            }
            VortexPhase::Absorb => {
                if elapsed >= ABSORB_DURATION {
                    VortexStatus::Complete(self.toast.clone())
                } else {
                    VortexStatus::Running
                }
            }
        }
    }

    // ── Rendering ─────────────────────────────────────────────────

    /// Cycling color blended from base tint + rotating palette.
    fn sparkle_color(&self, tick: u64) -> Color {
        let idx = ((tick / 3) % PALETTE.len() as u64) as usize;
        let (cr, cg, cb) = PALETTE[idx];
        let Color::Rgb(tr, tg, tb) = self.tint else { return self.tint };
        Color::Rgb(
            lerp_u8(tr, cr, 0.45),
            lerp_u8(tg, cg, 0.45),
            lerp_u8(tb, cb, 0.45),
        )
    }

    /// Produce (coord, glyph, color) overlays for this tick.
    pub fn render_overlays(&self, player_pos: Coordinate<i32>, tick: u64) -> Vec<(Coordinate<i32>, &str, Color)> {
        let elapsed = tick.saturating_sub(self.phase_start_tick);
        let mut out = Vec::new();

        match self.phase {
            VortexPhase::FixtureSwirl => {
                // Swirling block-quad on the fixture — fades in/out
                let frame = ((tick / SWIRL_SPEED) % 8) as usize;
                let t = elapsed.min(SWIRL_DURATION) as f32 / SWIRL_DURATION as f32;
                let brightness = 1.0 - (2.0 * t - 1.0).abs();
                out.push((self.source, SWIRL[frame], scale(self.sparkle_color(tick), brightness)));

                // Glinting motes on 2-3 neighbors (rotating)
                for (i, n) in self.source.neighbors().iter().enumerate() {
                    if (tick + i as u64) % 3 == 0 {
                        let mote = MOTES[(tick as usize / 2 + i) % MOTES.len()];
                        let c = self.sparkle_color(tick.wrapping_add(i as u64 * 5));
                        out.push((*n, mote, scale(c, brightness * 0.6)));
                    }
                }
            }

            VortexPhase::RoomSwirl => {
                let step = self.spiral_step.min(self.spiral_path.len().saturating_sub(1));
                let pos = self.spiral_path[step];

                // ── Leading particle: rapid block-quad shimmer ──
                let spark = SPARK[((tick / 2) % SPARK.len() as u64) as usize];
                let color = self.sparkle_color(tick);
                out.push((pos, spark, color));

                // ── Sparkling trail ──
                for trail_i in 1..=TRAIL_LEN {
                    if step >= trail_i {
                        let tp = self.spiral_path[step - trail_i];
                        let fade = 1.0 - (trail_i as f32 / (TRAIL_LEN as f32 + 0.5));
                        let tg = TRAIL[(trail_i - 1).min(TRAIL.len() - 1)];
                        let tc = self.sparkle_color(tick.wrapping_sub(trail_i as u64 * 5));
                        out.push((tp, tg, scale(tc, fade)));
                    }
                }

                // ── Neighbor motes around the leader (1-2 at a time) ──
                for (i, n) in pos.neighbors().iter().enumerate() {
                    if (tick + i as u64) % 4 == 0 {
                        let mote = MOTES[(tick as usize + i) % MOTES.len()];
                        let c = self.sparkle_color(tick.wrapping_add(i as u64 * 3));
                        out.push((*n, mote, scale(c, 0.45)));
                    }
                }

                // ── Fixture echo: fading swirl at source ──
                let swirl_fade = 1.0 - (elapsed.min(24) as f32 / 24.0);
                if swirl_fade > 0.05 {
                    let frame = ((tick / SWIRL_SPEED) % 8) as usize;
                    out.push((self.source, SWIRL[frame], scale(self.sparkle_color(tick), swirl_fade * 0.3)));
                }
            }

            VortexPhase::Absorb => {
                let t = elapsed.min(ABSORB_DURATION) as f32 / ABSORB_DURATION as f32;
                let brightness = 1.0 - t;
                if brightness < 0.05 { return out; }

                // Center: dense block shimmer
                let spark = SPARK[((tick / 2) % SPARK.len() as u64) as usize];
                out.push((player_pos, spark, scale(Color::Rgb(255, 250, 230), brightness)));

                // Ring 1: block-quad burst (all 6 neighbors)
                if t < 0.7 {
                    let r1_fade = brightness * (1.0 - t / 0.7);
                    let burst_frame = (elapsed as usize).min(BURST.len() - 1);
                    for (i, n) in player_pos.neighbors().iter().enumerate() {
                        let bg = BURST[(burst_frame + i) % BURST.len()];
                        let c = self.sparkle_color(tick.wrapping_add(i as u64 * 2));
                        out.push((*n, bg, scale(c, r1_fade)));
                    }
                }

                // Ring 2: scattered motes (brief flash)
                if t < 0.35 {
                    let r2_fade = brightness * (1.0 - t / 0.35) * 0.5;
                    let r2: Vec<Coordinate<i32>> = player_pos
                        .ring_iter(2, Spin::CW(Direction::XY))
                        .collect();
                    for (i, c) in r2.iter().enumerate() {
                        if (i + tick as usize) % 3 == 0 {
                            let mote = MOTES[(tick as usize + i) % MOTES.len()];
                            let mc = self.sparkle_color(tick.wrapping_add(i as u64 * 4));
                            out.push((*c, mote, scale(mc, r2_fade)));
                        }
                    }
                }
            }
        }

        out
    }

    pub fn light_position(&self, player_pos: Coordinate<i32>) -> Option<Coordinate<i32>> {
        match self.phase {
            VortexPhase::FixtureSwirl => Some(self.source),
            VortexPhase::RoomSwirl => {
                let step = self.spiral_step.min(self.spiral_path.len().saturating_sub(1));
                self.spiral_path.get(step).copied()
            }
            VortexPhase::Absorb => Some(player_pos),
        }
    }
}

// ── Spiral path computation ───────────────────────────────────────

fn compute_spiral_path(
    source: Coordinate<i32>,
    player_pos: Coordinate<i32>,
    hermes: bool,
) -> Vec<Coordinate<i32>> {
    let mut path = Vec::new();

    // Approach: fixture → nearest point on r=3 ring
    let r3_ring: Vec<Coordinate<i32>> = player_pos
        .ring_iter(3, Spin::CW(Direction::XY))
        .collect();
    if r3_ring.is_empty() {
        for c in source.line_to_iter(player_pos) {
            if path.last() != Some(&c) { path.push(c); }
        }
        return path;
    }

    let entry_idx = r3_ring.iter().enumerate()
        .min_by_key(|(_, c)| hex_distance(**c, source))
        .map(|(i, _)| i)
        .unwrap_or(0);

    for c in source.line_to_iter(r3_ring[entry_idx]) {
        if path.last() != Some(&c) { path.push(c); }
    }

    // r=3: half ring
    let r3_len = r3_ring.len();
    for i in 1..=r3_len / 2 {
        let c = r3_ring[(entry_idx + i) % r3_len];
        if path.last() != Some(&c) { path.push(c); }
    }
    let exit = *path.last().unwrap_or(&source);

    // r=2: half ring
    let r2_ring: Vec<Coordinate<i32>> = player_pos
        .ring_iter(2, Spin::CW(Direction::XY))
        .collect();
    let exit = if !r2_ring.is_empty() {
        let ei = nearest_idx(&r2_ring, exit);
        if path.last() != Some(&r2_ring[ei]) { path.push(r2_ring[ei]); }
        let r2_len = r2_ring.len();
        for i in 1..=r2_len / 2 {
            let c = r2_ring[(ei + i) % r2_len];
            if path.last() != Some(&c) { path.push(c); }
        }
        *path.last().unwrap_or(&source)
    } else { exit };

    // r=1: half (player) or full (hermes)
    let r1_ring: Vec<Coordinate<i32>> = player_pos
        .ring_iter(1, Spin::CW(Direction::XY))
        .collect();
    if !r1_ring.is_empty() {
        let ei = nearest_idx(&r1_ring, exit);
        if path.last() != Some(&r1_ring[ei]) { path.push(r1_ring[ei]); }
        let r1_len = r1_ring.len();
        let steps = if hermes { r1_len } else { r1_len / 2 };
        for i in 1..=steps {
            let c = r1_ring[(ei + i) % r1_len];
            if path.last() != Some(&c) { path.push(c); }
        }
    }

    if path.last() != Some(&player_pos) { path.push(player_pos); }
    path
}

fn nearest_idx(ring: &[Coordinate<i32>], target: Coordinate<i32>) -> usize {
    ring.iter().enumerate()
        .min_by_key(|(_, c)| hex_distance(**c, target))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

// ── Helpers ───────────────────────────────────────────────────────

fn hex_distance(a: Coordinate<i32>, b: Coordinate<i32>) -> i32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    let dz = -(dx + dy);
    dx.abs().max(dy.abs()).max(dz.abs())
}

fn scale(c: Color, factor: f32) -> Color {
    match c {
        Color::Rgb(r, g, b) => Color::Rgb(
            (r as f32 * factor).min(255.0) as u8,
            (g as f32 * factor).min(255.0) as u8,
            (b as f32 * factor).min(255.0) as u8,
        ),
        other => other,
    }
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let t = t.clamp(0.0, 1.0);
    (a as f32 + (b as f32 - a as f32) * t) as u8
}

fn brighten(c: Color) -> Color {
    match c {
        Color::Rgb(r, g, b) => Color::Rgb(
            (r as u16 + 40).min(255) as u8,
            (g as u16 + 30).min(255) as u8,
            (b as u16 + 20).min(255) as u8,
        ),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spiral_path_reaches_player() {
        let src = Coordinate::new(0, 0);
        let dst = Coordinate::new(6, -3);
        let path = compute_spiral_path(src, dst, false);
        assert!(!path.is_empty());
        assert_eq!(*path.last().unwrap(), dst);
        assert!(path.len() > hex_distance(src, dst) as usize);
    }

    #[test]
    fn hermes_spiral_longer_than_player() {
        let src = Coordinate::new(0, 0);
        let dst = Coordinate::new(5, -2);
        let player_path = compute_spiral_path(src, dst, false);
        let hermes_path = compute_spiral_path(src, dst, true);
        assert!(hermes_path.len() > player_path.len());
    }

    #[test]
    fn spiral_path_same_position() {
        let src = Coordinate::new(3, 3);
        let path = compute_spiral_path(src, src, false);
        assert!(!path.is_empty());
        assert_eq!(*path.last().unwrap(), src);
    }

    #[test]
    fn spiral_path_adjacent() {
        let src = Coordinate::new(0, 0);
        let dst = Coordinate::new(1, 0);
        let path = compute_spiral_path(src, dst, false);
        assert!(!path.is_empty());
        assert_eq!(*path.last().unwrap(), dst);
    }

    #[test]
    fn spiral_no_consecutive_duplicates() {
        let src = Coordinate::new(0, 0);
        let dst = Coordinate::new(5, -2);
        let path = compute_spiral_path(src, dst, true);
        for w in path.windows(2) {
            assert_ne!(w[0], w[1], "consecutive duplicate in spiral path");
        }
    }
}
