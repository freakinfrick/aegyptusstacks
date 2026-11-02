//! Scribe summoning ritual — glyph ring overlay animation.
//!
//! A ring of Egyptian-evocative glyphs appears on floor tiles around the
//! Scribe NPC when an article is being written. The ring progresses through
//! phases tied to the Scribe pipeline: Summoning → Channeling → Contracting
//! → Releasing (or Fizzling on error).
//!
//! This is a rendering overlay — it does NOT mutate the tile map.

use hex2d_stacks::{Coordinate, Spin};
use ratatui::style::Color;

/// Ritual glyphs — Egyptian-evocative 2-char symbols.
const GLYPHS: [&str; 6] = ["⊹⊹", "◇◇", "⊛⊛", "◆◆", "⊕⊕", "✧✧"];

/// Phase of the summoning ritual.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RitualPhase {
    /// Ring appears at r=3, glyphs fade in (~40 ticks / 2s)
    Summoning,
    /// Ring pulses brightness, glyphs cycle (Reasoning phase — indefinite)
    Channeling,
    /// Ring shrinks r=3→2→1 (Writing phase — ~60 ticks per step)
    Contracting,
    /// Burst outward r→4→5, glyphs fade (~40 ticks)
    Releasing,
    /// Error: glyphs turn red, fade out (~30 ticks)
    Fizzling,
}

/// Active scribe summoning ritual.
pub struct RitualRing {
    pub center: Coordinate<i32>,
    pub phase: RitualPhase,
    pub current_radius: i32,
    pub born_tick: u64,
    pub phase_start_tick: u64,
    pub room_coord: [i32; 3],
}

impl RitualRing {
    pub fn new(center: Coordinate<i32>, room_coord: [i32; 3], tick: u64) -> Self {
        RitualRing {
            center,
            phase: RitualPhase::Summoning,
            current_radius: 3,
            born_tick: tick,
            phase_start_tick: tick,
            room_coord,
        }
    }

    /// Transition to a new phase.
    pub fn advance_to(&mut self, phase: RitualPhase, tick: u64) {
        // Don't go backwards (except to Fizzling which overrides anything)
        if phase == RitualPhase::Fizzling || phase as u8 > self.phase as u8 {
            self.phase = phase;
            self.phase_start_tick = tick;
        }
    }

    /// Advance animation by one tick. Returns false when the ritual is done.
    pub fn tick(&mut self, tick: u64) -> bool {
        let elapsed = tick.saturating_sub(self.phase_start_tick);

        match self.phase {
            RitualPhase::Summoning => {
                // Auto-transition to Channeling after ~40 ticks
                if elapsed >= 40 {
                    self.phase = RitualPhase::Channeling;
                    self.phase_start_tick = tick;
                }
                true
            }
            RitualPhase::Channeling => {
                // Indefinite — wait for advance_to(Contracting)
                true
            }
            RitualPhase::Contracting => {
                // Shrink radius every ~60 ticks
                if elapsed >= 60 && self.current_radius > 1 {
                    self.current_radius -= 1;
                    self.phase_start_tick = tick;
                }
                // At r=1 after another 60 ticks, transition to Releasing
                if self.current_radius <= 1 && elapsed >= 60 {
                    self.current_radius = 0;
                    self.phase = RitualPhase::Releasing;
                    self.phase_start_tick = tick;
                }
                true
            }
            RitualPhase::Releasing => {
                // Expand outward: radius increases each ~10 ticks
                let burst_radius = (elapsed / 10) as i32;
                self.current_radius = burst_radius;
                // Done after expanding to r=5
                burst_radius <= 5
            }
            RitualPhase::Fizzling => {
                // Fade over ~40 ticks
                elapsed < 40
            }
        }
    }

    /// Get hex coordinates for the current ring.
    fn ring_positions(&self) -> Vec<Coordinate<i32>> {
        if self.current_radius <= 0 {
            return vec![self.center];
        }
        self.center
            .ring_iter(self.current_radius, Spin::CW(hex2d_stacks::XY))
            .collect()
    }

    /// Produce (coord, glyph, color) overlays for rendering this tick.
    pub fn render_overlays(&self, tick: u64) -> Vec<(Coordinate<i32>, &'static str, Color)> {
        let positions = self.ring_positions();
        let elapsed = tick.saturating_sub(self.phase_start_tick);

        let mut overlays = Vec::with_capacity(positions.len());

        for (i, coord) in positions.iter().enumerate() {
            let glyph = self.glyph_for(i, tick);
            let color = self.color_for(elapsed, tick);
            overlays.push((*coord, glyph, color));
        }

        overlays
    }

    /// Select glyph for a ring position, cycling during Channeling.
    fn glyph_for(&self, ring_index: usize, tick: u64) -> &'static str {
        let cycle_offset = match self.phase {
            RitualPhase::Channeling | RitualPhase::Contracting => (tick / 12) as usize,
            _ => 0,
        };
        GLYPHS[(ring_index + cycle_offset) % GLYPHS.len()]
    }

    /// Compute color for the current phase and elapsed time.
    fn color_for(&self, elapsed: u64, tick: u64) -> Color {
        match self.phase {
            RitualPhase::Summoning => {
                // Fade in: brightness ramps from 0 to full over 40 ticks
                let t = (elapsed.min(40) as f32) / 40.0;
                let r = (220.0 * t) as u8;
                let g = (180.0 * t) as u8;
                let b = (50.0 * t) as u8;
                Color::Rgb(r, g, b)
            }
            RitualPhase::Channeling => {
                // Pulse between bright and dim gold
                if (tick / 8) % 2 == 0 {
                    Color::Rgb(220, 180, 50)
                } else {
                    Color::Rgb(150, 120, 35)
                }
            }
            RitualPhase::Contracting => {
                // Gold → white as radius decreases
                let t = 1.0 - (self.current_radius as f32 / 3.0);
                let r = lerp_u8(220, 240, t);
                let g = lerp_u8(180, 230, t);
                let b = lerp_u8(50, 200, t);
                Color::Rgb(r, g, b)
            }
            RitualPhase::Releasing => {
                // White fading out
                let t = 1.0 - (elapsed.min(50) as f32 / 50.0);
                let v = (240.0 * t) as u8;
                Color::Rgb(v, v, (200.0 * t) as u8)
            }
            RitualPhase::Fizzling => {
                // Red fading out
                let t = 1.0 - (elapsed.min(40) as f32 / 40.0);
                let r = (200.0 * t) as u8;
                let g = (50.0 * t) as u8;
                let b = (50.0 * t) as u8;
                Color::Rgb(r, g, b)
            }
        }
    }
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let t = t.clamp(0.0, 1.0);
    (a as f32 + (b as f32 - a as f32) * t) as u8
}
