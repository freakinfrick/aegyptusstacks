use hex2d_stacks::Direction;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use super::dim_rect;

/// A nearby point of interest to display beside the compass.
pub struct NearbyItem {
    pub glyph: &'static str,
    pub label: String,
    pub color: Color,
}

/// Render the compass rose + nearby context, anchored bottom-left.
///
/// ```text
/// ┌─────┐
/// │Q W E│ && Librarian: Hermes
/// │A @ D│ %% Scribe: Kimi
/// │Z X C│ ↕↕ Shaft: up/down
/// └─────┘
/// ```
pub fn render_compass(
    facing: Direction,
    accent: Color,
    border_color: Color,
    nearby: &[NearbyItem],
    area: Rect,
    buf: &mut Buffer,
) {
    let w: u16 = 7;
    let h: u16 = 5;
    if area.width < w + 2 || area.height < h + 1 {
        return;
    }
    let x0 = area.x + 1;
    let y0 = area.bottom().saturating_sub(h + 1);
    let rect = Rect::new(x0, y0, w, h);

    dim_rect(buf, rect);

    let border_style = Style::default().fg(border_color);
    buf.set_string(x0, y0, "┌─────┐", border_style);
    buf.set_string(x0, y0 + 1, "│     │", border_style);
    buf.set_string(x0, y0 + 2, "│     │", border_style);
    buf.set_string(x0, y0 + 3, "│     │", border_style);
    buf.set_string(x0, y0 + 4, "└─────┘", border_style);

    let dim = Style::default().fg(Color::Rgb(60, 55, 50));
    let lit = Style::default().fg(accent).add_modifier(Modifier::BOLD);
    let player_style = Style::default().fg(Color::Green).add_modifier(Modifier::BOLD);

    let singles: [(char, Direction, u16, u16); 6] = [
        ('Q', Direction::YZ, 0, 0),
        ('E', Direction::XZ, 4, 0),
        ('A', Direction::YX, 0, 1),
        ('D', Direction::XY, 4, 1),
        ('Z', Direction::ZX, 0, 2),
        ('C', Direction::ZY, 4, 2),
    ];

    for (ch, dir, dx, dy) in singles {
        let style = if dir == facing { lit } else { dim };
        buf.set_string(x0 + 1 + dx, y0 + 1 + dy, &ch.to_string(), style);
    }

    let w_lit = facing == Direction::XZ;
    let x_lit = facing == Direction::ZX;
    buf.set_string(x0 + 3, y0 + 1, "W", if w_lit { lit } else { dim });
    buf.set_string(x0 + 3, y0 + 3, "X", if x_lit { lit } else { dim });
    buf.set_string(x0 + 3, y0 + 2, "@", player_style);

    // ── Nearby context lines to the right of the compass ──
    let ctx_x = x0 + w + 1;
    let max_lines = 3; // rows 1..3 inside the compass box
    let max_label_w = area.right().saturating_sub(ctx_x + 1) as usize;
    if max_label_w < 8 { return; }

    // Dim the area behind context lines
    for i in 0..max_lines.min(nearby.len()) {
        let cy = y0 + 1 + i as u16;
        let ctx_rect = Rect::new(ctx_x, cy, max_label_w as u16, 1);
        dim_rect(buf, ctx_rect);
    }

    for (i, item) in nearby.iter().take(max_lines).enumerate() {
        let cy = y0 + 1 + i as u16;
        buf.set_string(ctx_x, cy, item.glyph, Style::default().fg(item.color));
        let label_x = ctx_x + 3; // glyph(2) + space(1)
        let label_w = max_label_w.saturating_sub(3);
        let truncated: String = item.label.chars().take(label_w).collect();
        let label_color = dim_label(item.color);
        buf.set_string(label_x, cy, &truncated, Style::default().fg(label_color));
    }
}

/// Slightly dim a color for the label text (keep glyph bright, label softer).
fn dim_label(c: Color) -> Color {
    match c {
        Color::Rgb(r, g, b) => Color::Rgb(
            (r as u16 * 70 / 100) as u8,
            (g as u16 * 70 / 100) as u8,
            (b as u16 * 70 / 100) as u8,
        ),
        other => other,
    }
}
