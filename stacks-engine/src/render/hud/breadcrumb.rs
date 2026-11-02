use std::collections::VecDeque;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use crate::render::palette::Palette;
use crate::render::text_util::{display_width, truncate_to_cols};

use super::dim_rect;
use super::HudData;

/// Render the room info panel. Caller provides the exact rect.
pub fn render_breadcrumb(
    trail: &VecDeque<[i32; 3]>,
    current_room: [i32; 3],
    hud_data: &HudData,
    accent: Color,
    border_color: Color,
    _palette: Palette,
    rect: Rect,
    buf: &mut Buffer,
) {
    let w = rect.width;
    let h = rect.height;
    if w < 10 || h < 5 {
        return;
    }
    let x0 = rect.x;
    let y0 = rect.y;

    dim_rect(buf, rect);

    // Border
    let border_style = Style::default().fg(border_color);

    // Top border with title
    buf.set_string(x0, y0, "┌ Room ", border_style);
    for i in 7..w.saturating_sub(1) {
        buf.set_string(x0 + i, y0, "─", border_style);
    }
    buf.set_string(x0 + w - 1, y0, "┐", border_style);

    // Side borders + clear interior
    let bg = Style::default().fg(Color::Rgb(15, 12, 8)).bg(Color::Rgb(15, 12, 8));
    for i in 1..h - 1 {
        buf.set_string(x0, y0 + i, "│", border_style);
        for ix in (x0 + 1)..(x0 + w - 1) {
            buf.set_string(ix, y0 + i, " ", bg);
        }
        buf.set_string(x0 + w - 1, y0 + i, "│", border_style);
    }

    // Bottom border — shared with panels below, use T-junctions
    buf.set_string(x0, y0 + h - 1, "├", border_style);
    for i in 1..w.saturating_sub(1) {
        buf.set_string(x0 + i, y0 + h - 1, "─", border_style);
    }
    buf.set_string(x0 + w - 1, y0 + h - 1, "┤", border_style);

    let text_x = x0 + 2;
    let max_text_w = (w.saturating_sub(4)) as usize;

    // Line 1: Room name (may wrap to line 2 if long)
    let name_style = Style::default().fg(accent).add_modifier(Modifier::BOLD);
    let name_w = display_width(&hud_data.room_name);

    // Layout: name (1-2 lines) + district + era + coords
    // render_location writes 2 rows (district, era), so trail goes 2 below it
    if name_w <= max_text_w {
        buf.set_string(text_x, y0 + 1, &hud_data.room_name, name_style);
        render_location(buf, text_x, y0 + 2, max_text_w, hud_data);
        render_trail(buf, text_x, y0 + 4, max_text_w, trail, current_room, accent);
    } else {
        // Split at max_text_w display columns
        let mut col = 0;
        let split_byte = hud_data.room_name.char_indices()
            .find(|(_, c)| {
                col += unicode_width::UnicodeWidthChar::width(*c).unwrap_or(0);
                col > max_text_w
            })
            .map(|(i, _)| i)
            .unwrap_or(hud_data.room_name.len());
        let line1 = &hud_data.room_name[..split_byte];
        let line2 = truncate_to_cols(&hud_data.room_name[split_byte..], max_text_w);
        buf.set_string(text_x, y0 + 1, line1, name_style);
        buf.set_string(text_x, y0 + 2, &line2, name_style);
        render_location(buf, text_x, y0 + 3, max_text_w, hud_data);
        render_trail(buf, text_x, y0 + 5, max_text_w, trail, current_room, accent);
    }
}

fn render_location(buf: &mut Buffer, x: u16, y: u16, max_w: usize, hud_data: &HudData) {
    let dim = Style::default().fg(Color::Rgb(120, 115, 100));
    let district = truncate_to_cols(&hud_data.district, max_w);
    buf.set_string(x, y, &district, dim);
    let era = truncate_to_cols(&hud_data.era, max_w);
    buf.set_string(x, y + 1, &era, Style::default().fg(Color::Rgb(90, 85, 75)));
}

fn render_trail(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    max_w: usize,
    _trail: &VecDeque<[i32; 3]>,
    current_room: [i32; 3],
    accent: Color,
) {
    let coord_str = format!("[{},{},{}]", current_room[0], current_room[1], current_room[2]);
    let coord_str = truncate_to_cols(&coord_str, max_w);
    buf.set_string(x, y, &coord_str, Style::default().fg(accent));
}
