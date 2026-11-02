mod compass;
mod z_column;
pub mod minimap;
mod breadcrumb;
mod legend;

use std::collections::{BTreeSet, VecDeque};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use crate::game::Engine;
use crate::game::tile::{Tile, ShaftKind};
use crate::game::actor::ActorKind;
use crate::game::world::WorldMap;
use super::palette::Palette;
use compass::NearbyItem;

/// Pre-computed display strings for the breadcrumb panel.
pub struct HudData {
    pub room_name: String,
    pub district: String,
    pub era: String,
}

/// HUD display mode: Standard → ChatSidebar → Off → Standard
/// Compass is always rendered regardless of mode.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HudMode {
    Off,
    Standard,
    /// Minimized chat panel on left side — read-only, movement allowed
    ChatSidebar,
}

impl HudMode {
    /// Cycle to the next mode.
    pub fn next(self) -> Self {
        match self {
            HudMode::Standard => HudMode::ChatSidebar,
            HudMode::ChatSidebar => HudMode::Off,
            HudMode::Off => HudMode::Standard,
        }
    }
}

// Layout constants
const Z_COL_W: u16 = 6;
const MAP_W: u16 = 35;
const ROOM_H: u16 = 8; // border + name(2) + district + era + coords + blank + border
const MAP_H: u16 = 17;

/// Render compass + nearby context — always on, regardless of HUD mode.
pub fn render_compass_always(
    world: &WorldMap,
    engine: &Engine,
    palette: Palette,
    area: Rect,
    buf: &mut Buffer,
) {
    let accent = palette.accent_color();
    let border_color = hud_border_color(palette.wall_color());
    let nearby = collect_nearby(engine, world, &palette);
    compass::render_compass(engine.player.facing(), accent, border_color, &nearby, area, buf);
}

/// Render all HUD overlay elements onto the viewport buffer.
pub fn render_hud(
    world: &WorldMap,
    engine: &Engine,
    breadcrumb: &VecDeque<[i32; 3]>,
    palette: Palette,
    hud_data: &HudData,
    mode: HudMode,
    area: Rect,
    buf: &mut Buffer,
) {
    let accent = palette.accent_color();
    let border_color = hud_border_color(palette.wall_color());

    // Legend: always on, top-right
    legend::render_legend(palette, border_color, area, buf);

    // Compute z-column height from discovered levels
    let num_z: u16 = world.rooms.values()
        .map(|r| r.bfs_coord[2])
        .collect::<BTreeSet<_>>()
        .len() as u16;
    let z_col_h = (num_z + 2).max(4); // min 4 rows for border

    // Bottom row height = max(map, z-column)
    let bottom_h = MAP_H.max(z_col_h);
    let total_w = Z_COL_W + MAP_W;
    let total_h = ROOM_H + bottom_h - 1; // -1: shared border between room panel and bottom row

    if area.width < total_w + 2 || area.height < total_h + 2 {
        return;
    }

    // Anchor entire composition to bottom-right
    let x0 = area.right().saturating_sub(total_w + 1);
    let y0 = area.bottom().saturating_sub(total_h + 1);

    // Sub-rects
    let room_rect = Rect::new(x0, y0, total_w, ROOM_H);
    let z_rect = Rect::new(x0, y0 + ROOM_H - 1, Z_COL_W, bottom_h);
    let map_rect = Rect::new(x0 + Z_COL_W, y0 + ROOM_H + 1, MAP_W, bottom_h.saturating_sub(2));

    breadcrumb::render_breadcrumb(breadcrumb, engine.current_room, hud_data, accent, border_color, palette, room_rect, buf);
    z_column::render_z_column(world, engine.current_room, accent, border_color, palette, z_rect, buf);
    minimap::render_minimap(world, engine.current_room, palette, map_rect, buf);
}

/// Scan tiles and actors near the player, return up to 3 notable items.
fn collect_nearby(engine: &Engine, world: &WorldMap, palette: &Palette) -> Vec<NearbyItem> {
    let player = engine.player.coord();
    let mut items: Vec<NearbyItem> = Vec::new();

    // NPCs: check if librarian or scribe are within range 2
    for lib in &engine.librarians {
        let dist = hex_distance(player, lib.coord());
        if dist <= 2 {
            items.push(NearbyItem {
                glyph: ActorKind::Librarian.glyph(),
                label: "Librarian: Hermes".to_string(),
                color: ActorKind::Librarian.color(),
            });
        }
    }
    for scribe in &engine.scribes {
        let dist = hex_distance(player, scribe.coord());
        if dist <= 2 {
            items.push(NearbyItem {
                glyph: ActorKind::Scribe.glyph(),
                label: "Scribe: Kimi".to_string(),
                color: ActorKind::Scribe.color(),
            });
        }
    }

    // Tiles within range 1 of player — collect notable ones
    let mut seen_shaft = false;
    let mut seen_fixture: Option<(&'static str, &'static str, Color)> = None;
    let mut seen_lectern = false;
    let mut seen_staircase = false;

    for &dir in hex2d_stacks::Direction::all() {
        let coord = player + dir;
        if let Some(wt) = world.tiles.get(&coord) {
            match wt.tile {
                Tile::Shaft(kind) if !seen_shaft => {
                    seen_shaft = true;
                    let (glyph, label) = match kind {
                        ShaftKind::Up => ("↑↑", "Shaft: UP"),
                        ShaftKind::Down => ("↓↓", "Shaft: DOWN"),
                        ShaftKind::Both => ("↕↕", "Shaft: UP/DOWN"),
                    };
                    items.push(NearbyItem {
                        glyph,
                        label: label.to_string(),
                        color: palette.shaft_color(),
                    });
                }
                Tile::Lectern if !seen_lectern => {
                    seen_lectern = true;
                    items.push(NearbyItem {
                        glyph: "△△",
                        label: "Lectern".to_string(),
                        color: palette.lectern_color(),
                    });
                }
                Tile::ArticleDisplay => {
                    items.push(NearbyItem {
                        glyph: "≋≋",
                        label: "Article".to_string(),
                        color: palette.article_display_color(),
                    });
                }
                Tile::Staircase if !seen_staircase => {
                    seen_staircase = true;
                    items.push(NearbyItem {
                        glyph: "{}",
                        label: "Staircase".to_string(),
                        color: palette.staircase_color(),
                    });
                }
                tile if tile.is_text_bearing() && seen_fixture.is_none() => {
                    let (glyph, label, color) = fixture_info(tile, palette);
                    seen_fixture = Some((glyph, label, color));
                    items.push(NearbyItem {
                        glyph,
                        label: label.to_string(),
                        color,
                    });
                }
                _ => {}
            }
        }
    }

    // Also check the tile the player is standing on
    if let Some(wt) = world.tiles.get(&player) {
        match wt.tile {
            Tile::Shaft(kind) if !seen_shaft => {
                let (glyph, label) = match kind {
                    ShaftKind::Up => ("↑↑", "Shaft: UP"),
                    ShaftKind::Down => ("↓↓", "Shaft: DOWN"),
                    ShaftKind::Both => ("↕↕", "Shaft: UP/DOWN"),
                };
                items.push(NearbyItem {
                    glyph,
                    label: label.to_string(),
                    color: palette.shaft_color(),
                });
            }
            Tile::Lectern if !seen_lectern => {
                items.push(NearbyItem {
                    glyph: "△△",
                    label: "Lectern".to_string(),
                    color: palette.lectern_color(),
                });
            }
            Tile::ArticleDisplay => {
                items.push(NearbyItem {
                    glyph: "≋≋",
                    label: "Article".to_string(),
                    color: palette.article_display_color(),
                });
            }
            Tile::Staircase if !seen_staircase => {
                items.push(NearbyItem {
                    glyph: "{}",
                    label: "Staircase".to_string(),
                    color: palette.staircase_color(),
                });
            }
            _ => {}
        }
    }

    // Cap at 3 items (compass has 3 rows)
    items.truncate(3);
    items
}

/// Map a text-bearing tile to its (glyph, label, color).
fn fixture_info(tile: Tile, palette: &Palette) -> (&'static str, &'static str, Color) {
    match tile {
        Tile::PapyrusTablet   => ("││", "Papyrus",          palette.papyrus_color()),
        Tile::WallInscription => ("══", "Wall Inscription",  palette.inscription_color()),
        Tile::ScrollPedestal  => ("()", "Scroll Rack",       palette.scroll_color()),
        Tile::FalseDoor       => ("╔╗", "False Door",        palette.false_door_color()),
        Tile::TombAlcove      => ("▼▼", "Tomb Alcove",       palette.tomb_alcove_color()),
        Tile::Vessel          => ("♦♦", "Vessel",            palette.vessel_color()),
        Tile::Ostrakon        => (",,", "Ostrakon",          palette.ostrakon_color()),
        Tile::Stele           => ("▐▌", "Stele",             palette.stele_color()),
        Tile::Sculpture       => ("☥☥", "Sculpture",         palette.sculpture_color()),
        Tile::Coffin          => ("⊏⊐", "Coffin",            palette.coffin_color()),
        Tile::RockFace        => ("▓▓", "Rock Face",         palette.rock_face_color()),
        Tile::Shelf           => ("≡≡", "Shelf",             palette.shelf_color()),
        _                     => ("≡≡", "Fixture",           palette.shelf_color()),
    }
}

/// Map a physical_type string to a fixture glyph and color.
/// Used by gather toasts and inventory rendering.
pub fn glyph_for_physical_type(ptype: &str) -> (&'static str, Color) {
    match ptype {
        "papyrus"                          => ("││", Color::Rgb(196, 160, 80)),
        "wall_inscription" | "wall"        => ("══", Color::Rgb(196, 146, 42)),
        "scroll" | "writing_tablet" | "writing_board" | "writing_medium"
                                           => ("()", Color::Rgb(220, 210, 180)),
        "false_door" | "doorway"           => ("╔╗", Color::Rgb(180, 60, 60)),
        "tomb" | "tomb_inscription"        => ("▼▼", Color::Rgb(140, 100, 180)),
        "vessel" | "object"                => ("♦♦", Color::Rgb(190, 120, 70)),
        "ostrakon" | "ostracon"            => (",,", Color::Rgb(200, 170, 90)),
        "stele" | "rounded_stele" | "rectangular_stele"
                                           => ("▐▌", Color::Rgb(180, 180, 195)),
        "sculpture"                        => ("☥☥", Color::Rgb(100, 180, 170)),
        "coffin"                           => ("⊏⊐", Color::Rgb(140, 90, 160)),
        "rock" | "rock_cut"                => ("▓▓", Color::Rgb(160, 120, 80)),
        _                                  => ("≡≡", Color::Rgb(160, 155, 140)),
    }
}

/// Render stacking gather toast notifications at top-left.
pub fn render_gather_toasts(
    toasts: &[(String, &str, Color, std::time::Instant)],
    area: Rect,
    buf: &mut Buffer,
) {
    let warm_white = Color::Rgb(240, 235, 220);
    let max_show = 5.min(toasts.len());
    for (i, (text, glyph, color, _)) in toasts.iter().rev().take(max_show).enumerate() {
        let y = area.y + 1 + i as u16;
        if y >= area.bottom() { break; }
        let x = area.x + 1;
        let w = area.width.saturating_sub(2);

        // Glyph
        buf.set_string(x, y, glyph, Style::default().fg(*color));
        // Text
        let msg = format!(" {}", text);
        let max_len = (w as usize).saturating_sub(3);
        let truncated = if msg.len() > max_len { &msg[..max_len] } else { &msg };
        buf.set_string(x + 2, y, truncated, Style::default().fg(warm_white));
    }
}

/// Hex distance between two coordinates.
fn hex_distance(a: hex2d_stacks::Coordinate<i32>, b: hex2d_stacks::Coordinate<i32>) -> i32 {
    let dx = (a.x - b.x).abs();
    let dy = (a.y - b.y).abs();
    let dz = ((-a.x - a.y) - (-b.x - b.y)).abs();
    (dx + dy + dz) / 2
}

/// Derive a visible border color from wall_color.
fn hud_border_color(wall: Color) -> Color {
    match wall {
        Color::Rgb(r, g, b) => Color::Rgb(
            ((r as u16 * 7 / 10) as u8).max(45),
            ((g as u16 * 7 / 10) as u8).max(40),
            ((b as u16 * 7 / 10) as u8).max(35),
        ),
        c => c,
    }
}

/// Dim all cells in a rect to create a semi-transparent overlay background.
fn dim_rect(buf: &mut Buffer, area: Rect) {
    for y in area.y..area.y.saturating_add(area.height) {
        for x in area.x..area.x.saturating_add(area.width) {
            let pos = ratatui::layout::Position { x, y };
            if let Some(cell) = buf.cell_mut(pos) {
                cell.set_fg(Color::Rgb(30, 25, 20));
                cell.set_bg(Color::Rgb(15, 12, 8));
            }
        }
    }
}
