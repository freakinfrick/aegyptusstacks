use std::collections::BTreeSet;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use crate::game::world::WorldMap;
use crate::render::palette::Palette;

use super::dim_rect;

/// Render the z-level elevator column. Caller provides the exact rect.
pub fn render_z_column(
    world: &WorldMap,
    current_room: [i32; 3],
    accent: Color,
    border_color: Color,
    palette: Palette,
    rect: Rect,
    buf: &mut Buffer,
) {
    let z_levels: BTreeSet<i32> = world.rooms.values()
        .map(|r| r.bfs_coord[2])
        .collect();

    if z_levels.is_empty() {
        return;
    }

    let mut sorted: Vec<i32> = z_levels.into_iter().collect();
    sorted.sort_by(|a, b| b.cmp(a));

    let current_z = current_room[2];
    let w = rect.width;
    let h = rect.height;
    let x0 = rect.x;
    let y0 = rect.y;

    if w < 5 || h < 3 {
        return;
    }

    dim_rect(buf, rect);

    let border_style = Style::default().fg(border_color);

    // Top border — shared with room panel above, use T-junction on right
    buf.set_string(x0, y0, "├", border_style);
    buf.set_string(x0 + 1, y0, "─Z─", border_style);
    if w >= 6 {
        buf.set_string(x0 + 4, y0, "─", border_style);
    }
    // Right side uses T-junction connecting to minimap
    buf.set_string(x0 + w - 1, y0, "┬", border_style);

    // Side borders + clear interior
    let num_rows = h.saturating_sub(2);
    let bg = Style::default().fg(Color::Rgb(15, 12, 8)).bg(Color::Rgb(15, 12, 8));
    for i in 1..=num_rows {
        let row = y0 + i;
        buf.set_string(x0, row, "│", border_style);
        for ix in (x0 + 1)..(x0 + w - 1) {
            buf.set_string(ix, row, " ", bg);
        }
        buf.set_string(x0 + w - 1, row, "│", border_style);
    }

    // Bottom border
    buf.set_string(x0, y0 + h - 1, "└", border_style);
    for i in 1..w.saturating_sub(1) {
        buf.set_string(x0 + i, y0 + h - 1, "─", border_style);
    }
    buf.set_string(x0 + w - 1, y0 + h - 1, "┴", border_style);

    let dim = Style::default().fg(Color::Rgb(80, 75, 65));
    let lit = Style::default().fg(accent).add_modifier(Modifier::BOLD);
    let shaft_style = Style::default().fg(palette.shaft_color());

    // Render z-levels (clamp to available rows)
    let max_entries = num_rows as usize;
    for (i, &z) in sorted.iter().take(max_entries).enumerate() {
        let has_up = world.rooms.values().any(|r| {
            r.bfs_coord[2] == z && (
                r.info.shaft_edges.iter().any(|e| e.direction == "UP")
                || r.info.door_edges.iter().any(|e| e.direction == "UP")
            )
        });
        let has_down = world.rooms.values().any(|r| {
            r.bfs_coord[2] == z && (
                r.info.shaft_edges.iter().any(|e| e.direction == "DOWN")
                || r.info.door_edges.iter().any(|e| e.direction == "DOWN")
            )
        });

        let is_current = z == current_z;
        let style = if is_current { lit } else { dim };

        let z_str = format!("{:>2}", z);
        let indicator = if is_current {
            "●"
        } else {
            match (has_up, has_down) {
                (true, true) => "↕",
                (true, false) => "▲",
                (false, true) => "▼",
                (false, false) => " ",
            }
        };

        let row = y0 + 1 + i as u16;
        if row < y0 + h - 1 {
            buf.set_string(x0 + 1, row, &z_str, style);
            let ind_style = if is_current { lit } else { shaft_style };
            buf.set_string(x0 + 3, row, indicator, ind_style);
        }
    }
}
