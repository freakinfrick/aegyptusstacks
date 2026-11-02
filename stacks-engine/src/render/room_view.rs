use std::collections::HashMap;

use hex2d_stacks::Coordinate;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use crate::game::{ActorKind, Tile, WorldTile, HexFace};
use crate::game::world::LoadedRoom;
use super::hex_viewport::HexViewport;
use super::palette::Palette;
use super::lighting::{LightSource, apply_lighting};
use super::particles::{RoomParticles, particle_glyph, particle_visible};
use super::ritual::RitualRing;
use super::gather_vortex::GatherVortex;

/// Renders the world map centered on the player.
/// Each hex cell is 2 chars wide, 1 row tall.
pub struct RoomView<'a> {
    pub tiles: &'a HashMap<Coordinate<i32>, WorldTile>,
    pub actors: &'a [(Coordinate<i32>, ActorKind)],
    pub player_pos: Coordinate<i32>,
    pub palette: Palette,
    pub rooms: &'a HashMap<[i32; 3], LoadedRoom>,
    pub light_sources: &'a [LightSource],
    pub tick: u64,
    pub room_particles: Option<&'a RoomParticles>,
    pub ritual_ring: Option<&'a RitualRing>,
    pub gather_vortices: &'a [GatherVortex],
}

impl<'a> RoomView<'a> {
    /// Resolve palette for a tile based on its room's district + z.
    /// Falls back to the global palette if room not found.
    fn palette_for(&self, room_coord: &[i32; 3]) -> Palette {
        self.rooms.get(room_coord)
            .map(|r| Palette::from_room(r.info.z, &r.info.district))
            .unwrap_or(self.palette)
    }
}

/// Position-deterministic hash for glyph texture variation (System 6).
fn tile_variant_hash(x: i32, y: i32, room_q: i32, room_r: i32) -> u8 {
    let h = (x.wrapping_mul(374761393) ^ y.wrapping_mul(668265263)
             ^ room_q.wrapping_mul(2147483647) ^ room_r.wrapping_mul(1500450271)) as u32;
    let h = h.wrapping_mul(2654435761);
    (h >> 24) as u8
}

/// Wall glyph variant based on seed and district.
fn wall_glyph_variant(seed: u8, district: &str) -> &'static str {
    if district == "quarry" {
        match seed {
            0..=191 => "██",
            192..=220 => "▓█",
            221..=245 => "█▓",
            _ => "▓▓",
        }
    } else {
        match seed {
            0..=229 => "██",
            230..=245 => "▓█",
            _ => "█▓",
        }
    }
}

/// Clamp-add a signed offset to an RGB color channel.
fn ch_add(base: u8, offset: i16) -> u8 {
    (base as i16 + offset).clamp(0, 255) as u8
}

impl<'a> Widget for RoomView<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let viewport = HexViewport::new(self.player_pos, area.width, area.height);
        let coords = viewport.visible_coords();

        // Draw order: corridors first (background), content rooms on top.
        // Content room tiles overwrite corridor tiles at shared boundaries,
        // making content rooms visually dominant.

        let is_corridor_tile = |wt: &WorldTile| -> bool {
            self.rooms.get(&wt.room_coord)
                .map(|r| r.info.room_type == "corridor")
                .unwrap_or(false)
        };

        // Pass 1: Corridor walls (darkest layer)
        for &coord in &coords {
            if let Some(wt) = self.tiles.get(&coord) {
                if wt.tile == Tile::Wall && is_corridor_tile(wt) {
                    let pal = self.palette_for(&wt.room_coord);
                    let district = self.rooms.get(&wt.room_coord)
                        .map(|r| r.info.district.as_str())
                        .unwrap_or("unknown");
                    self.draw_wall(coord, &viewport, area, buf, pal.corridor_wall_color(), district);
                }
            }
        }

        // Pass 2: Corridor non-wall tiles
        for &coord in &coords {
            if let Some(wt) = self.tiles.get(&coord) {
                if wt.tile != Tile::Wall && is_corridor_tile(wt) {
                    if let Some((col, row)) = viewport.world_to_screen(coord) {
                        let pal = self.palette_for(&wt.room_coord);
                        let district = self.rooms.get(&wt.room_coord)
                            .map(|r| r.info.district.as_str())
                            .unwrap_or("unknown");
                        let variant_seed = tile_variant_hash(coord.x, coord.y, wt.room_coord[0], wt.room_coord[1]);
                        let (glyph, base_color) = tile_glyph(&wt.tile, wt.face, &pal, self.tick, variant_seed, district);
                        let lit_color = apply_lighting(base_color, coord, self.light_sources);
                        let x = area.x + col;
                        let y = area.y + row;
                        if x + 2 <= area.x + area.width && y < area.y + area.height {
                            buf.set_string(x, y, glyph, Style::default().fg(lit_color));
                        }
                    }
                }
            }
        }

        // Pass 3: Content room walls (drawn over corridors)
        for &coord in &coords {
            if let Some(wt) = self.tiles.get(&coord) {
                if wt.tile == Tile::Wall && !is_corridor_tile(wt) {
                    let pal = self.palette_for(&wt.room_coord);
                    let district = self.rooms.get(&wt.room_coord)
                        .map(|r| r.info.district.as_str())
                        .unwrap_or("unknown");
                    self.draw_wall(coord, &viewport, area, buf, pal.wall_color(), district);
                }
            }
        }

        // Pass 4: Content room non-wall tiles (drawn over everything)
        for &coord in &coords {
            if let Some(wt) = self.tiles.get(&coord) {
                if wt.tile != Tile::Wall && !is_corridor_tile(wt) {
                    if let Some((col, row)) = viewport.world_to_screen(coord) {
                        let pal = self.palette_for(&wt.room_coord);
                        let district = self.rooms.get(&wt.room_coord)
                            .map(|r| r.info.district.as_str())
                            .unwrap_or("unknown");
                        let variant_seed = tile_variant_hash(coord.x, coord.y, wt.room_coord[0], wt.room_coord[1]);
                        let (glyph, base_color) = tile_glyph(&wt.tile, wt.face, &pal, self.tick, variant_seed, district);
                        let lit_color = apply_lighting(base_color, coord, self.light_sources);
                        let x = area.x + col;
                        let y = area.y + row;
                        if x + 2 <= area.x + area.width && y < area.y + area.height {
                            buf.set_string(x, y, glyph, Style::default().fg(lit_color));
                        }
                    }
                }
            }
        }

        // Pass 4.25: Ritual ring overlay (glyph ring around Scribe NPC)
        if let Some(ref ritual) = self.ritual_ring {
            for (coord, glyph, color) in ritual.render_overlays(self.tick) {
                if let Some((col, row)) = viewport.world_to_screen(coord) {
                    // Only overlay on passable tiles (skip walls, shelves)
                    let passable = self.tiles.get(&coord)
                        .map(|wt| wt.tile.is_passable())
                        .unwrap_or(false);
                    if passable {
                        let x = area.x + col;
                        let y = area.y + row;
                        if x + 2 <= area.x + area.width && y < area.y + area.height {
                            buf.set_string(x, y, glyph, Style::default().fg(color));
                        }
                    }
                }
            }
        }

        // Pass 4.3: Gather vortex overlays
        for vortex in self.gather_vortices {
            for (coord, glyph, color) in vortex.render_overlays(self.player_pos, self.tick) {
                if let Some((col, row)) = viewport.world_to_screen(coord) {
                    let x = area.x + col;
                    let y = area.y + row;
                    if x + 2 <= area.x + area.width && y < area.y + area.height {
                        buf.set_string(x, y, glyph, Style::default().fg(color));
                    }
                }
            }
        }

        // Pass 4.5: Particles
        if let Some(rp) = self.room_particles {
            for particle in &rp.particles {
                if !particle_visible(particle, self.tick) {
                    continue;
                }
                if let Some((col, row)) = viewport.world_to_screen(particle.pos) {
                    let (glyph, base_color) = particle_glyph(particle, self.tick, &self.palette);
                    let lit_color = apply_lighting(base_color, particle.pos, self.light_sources);
                    let x = area.x + col;
                    let y = area.y + row;
                    if x + 1 <= area.x + area.width && y < area.y + area.height {
                        buf.set_string(x, y, glyph, Style::default().fg(lit_color));
                    }
                }
            }
        }

        // Pass 5: Actors on top of everything
        for (coord, kind) in self.actors {
            if let Some((col, row)) = viewport.world_to_screen(*coord) {
                let (glyph, fg) = actor_glyph(kind, self.tick);
                let style = Style::default().fg(fg).add_modifier(Modifier::BOLD);
                let x = area.x + col;
                let y = area.y + row;
                if x + 2 <= area.x + area.width && y < area.y + area.height {
                    buf.set_string(x, y, glyph, style);
                }
            }
        }
    }
}

impl<'a> RoomView<'a> {
    /// Draw a wall tile with extension logic for contiguous walls.
    fn draw_wall(
        &self,
        coord: Coordinate<i32>,
        viewport: &HexViewport,
        area: Rect,
        buf: &mut Buffer,
        base_fg: Color,
        district: &str,
    ) {
        if let Some((col, row)) = viewport.world_to_screen(coord) {
            let x = area.x + col;
            let y = area.y + row;

            let rc = self.tiles.get(&coord).map(|wt| wt.room_coord).unwrap_or([0, 0, 0]);
            let seed = tile_variant_hash(coord.x, coord.y, rc[0], rc[1]);
            let glyph = wall_glyph_variant(seed, district);
            let lit = apply_lighting(base_fg, coord, self.light_sources);

            let e_neighbor = coord + Coordinate::new(1, -1);
            let has_wall_east = self.tiles.get(&e_neighbor)
                .map(|t| t.tile == Tile::Wall)
                .unwrap_or(false);

            if has_wall_east && x + 4 <= area.x + area.width && y < area.y + area.height {
                let e_rc = self.tiles.get(&e_neighbor).map(|wt| wt.room_coord).unwrap_or(rc);
                let e_seed = tile_variant_hash(e_neighbor.x, e_neighbor.y, e_rc[0], e_rc[1]);
                let e_glyph = wall_glyph_variant(e_seed, district);
                let e_lit = apply_lighting(base_fg, e_neighbor, self.light_sources);
                buf.set_string(x, y, glyph, Style::default().fg(lit));
                buf.set_string(x + 2, y, e_glyph, Style::default().fg(e_lit));
            } else if x + 2 <= area.x + area.width && y < area.y + area.height {
                buf.set_string(x, y, glyph, Style::default().fg(lit));
            }
        }
    }
}

fn tile_glyph(tile: &Tile, _face: Option<HexFace>, palette: &Palette, tick: u64, variant_seed: u8, district: &str) -> (&'static str, Color) {
    use crate::game::tile::ShaftKind;
    match tile {
        // ── Animated tiles ──────────────────────────────────────────

        Tile::Brazier => {
            let frame = (tick / 7) % 4;
            let glyph = match frame {
                0 => "♨♨",
                1 => "♦♨",
                2 => "♨♦",
                _ => "♨♨",
            };
            let Color::Rgb(r, g, b) = palette.brazier_color() else {
                return (glyph, palette.brazier_color());
            };
            let color = match frame {
                1 => Color::Rgb(ch_add(r, 15), ch_add(g, 7), b),
                2 => Color::Rgb(ch_add(r, -10), ch_add(g, -5), b),
                _ => Color::Rgb(r, g, b),
            };
            (glyph, color)
        }

        Tile::Pool => {
            let frame = (tick / 12) % 4;
            let glyph = match frame {
                0 => "≋≋",
                1 => "≈≈",
                2 => "~≈",
                _ => "≈~",
            };
            let Color::Rgb(r, g, b) = palette.pool_color() else {
                return (glyph, palette.pool_color());
            };
            let offset: i16 = match frame { 0 => 0, 1 => 8, 2 => -8, _ => 4 };
            (glyph, Color::Rgb(r, g, ch_add(b, offset)))
        }

        Tile::Shaft(kind) => {
            let frame = (tick / 16) % 4;
            let glyph = match kind {
                ShaftKind::Up => "↑↑",
                ShaftKind::Down => "↓↓",
                ShaftKind::Both => "↕↕",
            };
            let Color::Rgb(r, g, b) = palette.shaft_color() else {
                return (glyph, palette.shaft_color());
            };
            let pulse: i16 = match frame { 0 => 0, 1 => 20, 2 => 10, _ => -10 };
            (glyph, Color::Rgb(ch_add(r, pulse), ch_add(g, pulse), ch_add(b, pulse)))
        }

        Tile::Portal => {
            // Rotating bar swirl — 8 cardinal/diagonal positions, clockwise.
            // Each frame has equal visual weight (4 sub-pixels filled).
            // Per-tile phase offset (via variant_seed) creates a coordinated
            // vortex effect across the multi-tile portal cluster.
            const SWIRL: [&str; 8] = [
                "▀▀",  // ── top bar
                "▜▝",  //  ╲ top-right
                "▐▐",  //  │ right bar
                "▗▟",  //  ╱ bottom-right
                "▄▄",  // ── bottom bar
                "▖▙",  //  ╲ bottom-left
                "▌▌",  //  │ left bar
                "▘▛",  //  ╱ top-left
            ];
            let phase = variant_seed as u64 % 8;
            let frame = ((tick / 5) + phase) % 8;
            let color_phase = ((tick / 10) + phase) % 6;
            let color = match color_phase {
                0 => Color::Rgb(200, 50, 200),   // magenta
                1 => Color::Rgb(180, 60, 230),   // magenta-violet
                2 => Color::Rgb(120, 100, 240),  // violet
                3 => Color::Rgb(50, 180, 230),   // cyan
                4 => Color::Rgb(80, 200, 200),   // cyan-teal
                _ => Color::Rgb(160, 80, 210),   // violet-magenta
            };
            (SWIRL[frame as usize], color)
        }

        Tile::Sconce => {
            let frame = (tick / 10) % 4;
            let glyph = match frame {
                0 => "✦✦",
                1 => "✦·",
                2 => "·✦",
                _ => "✦✦",
            };
            let Color::Rgb(r, g, b) = palette.sconce_color() else {
                return (glyph, palette.sconce_color());
            };
            let offset: i16 = match frame { 1 => 5, 2 => -5, _ => 0 };
            (glyph, Color::Rgb(ch_add(r, offset), ch_add(g, offset), ch_add(b, offset)))
        }

        Tile::Lectern => ("△△", palette.lectern_color()),
        Tile::ArticleDisplay => ("≋≋", palette.article_display_color()),

        // ── Textured tiles (position-hashed variant) ────────────────

        Tile::Floor => {
            let glyph = match district {
                "necropolis" => match variant_seed {
                    0..=140 => "::",
                    141..=190 => ":.",
                    191..=230 => "..",
                    _ => ". ",
                },
                "quarry" => match variant_seed {
                    0..=153 => "::",
                    154..=204 => ":·",
                    205..=240 => "·:",
                    _ => ":.",
                },
                "domestic" => match variant_seed {
                    0..=191 => "::",
                    192..=229 => ":.",
                    _ => "··",
                },
                _ => match variant_seed {
                    0..=178 => "::",
                    179..=204 => ":.",
                    205..=229 => ".:",
                    _ => "··",
                },
            };
            (glyph, palette.floor_color())
        }

        Tile::CorridorFloor => {
            let glyph = match variant_seed {
                0..=204 => "· ",
                205..=240 => ". ",
                _ => "  ",
            };
            (glyph, palette.corridor_floor_color())
        }

        // ── Wall (handled here for non-draw_wall paths, e.g. tile_glyph fallback) ──
        Tile::Wall => {
            let glyph = wall_glyph_variant(variant_seed, district);
            (glyph, palette.wall_color())
        }

        // ── Static tiles (unchanged) ───────────────────────────────

        Tile::Door(true) => ("::", palette.floor_color()),
        Tile::Door(false) => ("╡╞", palette.door_color()),
        Tile::Shelf => ("≡≡", palette.shelf_color()),
        Tile::Column => ("◉◉", palette.column_color()),
        Tile::Railing => ("~~", palette.railing_color()),
        Tile::VestibuleFloor => ("··", palette.vestibule_floor_color()),
        Tile::Staircase => ("{}", palette.staircase_color()),
        Tile::ClosetPanel => ("[]", palette.closet_color()),
        Tile::Mirror => ("<>", palette.mirror_color()),
        // Egyptian Kit-of-Parts — each has a unique fixed color
        Tile::PapyrusTablet => ("││", palette.papyrus_color()),
        Tile::WallInscription => ("══", palette.inscription_color()),
        Tile::ScrollPedestal => ("()", palette.scroll_color()),
        Tile::FalseDoor => ("╔╗", palette.false_door_color()),
        Tile::TombAlcove => ("▼▼", palette.tomb_alcove_color()),
        Tile::Vessel => ("♦♦", palette.vessel_color()),
        Tile::Ostrakon => (",,", palette.ostrakon_color()),
        Tile::Stele => ("▐▌", palette.stele_color()),
        Tile::Sculpture => ("☥☥", palette.sculpture_color()),
        Tile::Coffin => ("⊏⊐", palette.coffin_color()),
        Tile::RockFace => ("▓▓", palette.rock_face_color()),
    }
}

fn actor_glyph(kind: &ActorKind, tick: u64) -> (&'static str, Color) {
    match kind {
        ActorKind::Player => {
            // Breathing pulse: green brightness oscillates
            let phase = (tick / 24) % 4;
            let pulse: i16 = match phase { 0 => 0, 1 => 20, 2 => 10, _ => -10 };
            let Color::Rgb(r, g, b) = kind.color() else { return (kind.glyph(), kind.color()) };
            let color = Color::Rgb(
                ch_add(r, pulse / 2),
                ch_add(g, pulse),
                ch_add(b, pulse / 2),
            );
            (kind.glyph(), color)
        }
        ActorKind::Scribe => {
            let cyan = Color::Rgb(0, 210, 210);
            (kind.glyph(), cyan)
        }
        _ => (kind.glyph(), kind.color()),
    }
}
