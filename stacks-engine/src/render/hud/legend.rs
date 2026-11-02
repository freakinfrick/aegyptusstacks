use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use super::Palette;
use super::dim_rect;
use crate::render::text_util::display_width;

/// Fixture legend entries: (glyph, label, color_fn).
/// Order matches visual prominence — structural first, then fixtures.
const LEGEND_ENTRIES: &[(&str, &str, LegendColor)] = &[
    // Structural / architectural
    ("██", "Wall",       LegendColor::Wall),
    ("::", "Floor",      LegendColor::Floor),
    ("╡╞", "Door",       LegendColor::Door),
    ("↕↕", "Shaft",      LegendColor::Shaft),
    ("△△", "Lectern",    LegendColor::Lectern),
    ("≋≋", "Article",    LegendColor::Article),
    ("{}", "Staircase",  LegendColor::Staircase),
    ("··", "Corridor",   LegendColor::CorridorFloor),
    ("≡≡", "Shelf",      LegendColor::Shelf),
    // Corpus fixtures — wall-mounted
    ("││", "Papyrus",    LegendColor::Papyrus),
    ("══", "Inscription", LegendColor::Inscription),
    ("╔╗", "False Door", LegendColor::FalseDoor),
    ("▼▼", "Tomb Alcove", LegendColor::TombAlcove),
    // Corpus fixtures — freestanding
    ("▐▌", "Stele",      LegendColor::Stele),
    ("()", "Scroll Rack", LegendColor::Scroll),
    ("♦♦", "Vessel",     LegendColor::Vessel),
    ("☥☥", "Sculpture",  LegendColor::Sculpture),
    ("⊏⊐", "Coffin",     LegendColor::Coffin),
    (",,", "Ostrakon",   LegendColor::Ostrakon),
    ("▓▓", "Rock Face",  LegendColor::RockFace),
];

enum LegendColor {
    // Architectural
    Wall,
    Floor,
    Door,
    Shaft,
    Lectern,
    Article,
    Staircase,
    CorridorFloor,
    Shelf,
    // Corpus fixtures (fixed colors)
    Papyrus,
    Inscription,
    FalseDoor,
    TombAlcove,
    Stele,
    Scroll,
    Vessel,
    Sculpture,
    Coffin,
    Ostrakon,
    RockFace,
}

impl LegendColor {
    fn resolve(&self, palette: &Palette) -> Color {
        match self {
            // Architectural — palette-dependent
            LegendColor::Wall => palette.wall_color(),
            LegendColor::Floor => palette.floor_color(),
            LegendColor::Door => palette.door_color(),
            LegendColor::Shaft => palette.shaft_color(),
            LegendColor::Lectern => palette.lectern_color(),
            LegendColor::Article => palette.article_display_color(),
            LegendColor::Staircase => palette.staircase_color(),
            LegendColor::CorridorFloor => palette.corridor_floor_color(),
            LegendColor::Shelf => palette.shelf_color(),
            // Corpus fixtures — fixed, unique per type
            LegendColor::Papyrus => palette.papyrus_color(),
            LegendColor::Inscription => palette.inscription_color(),
            LegendColor::FalseDoor => palette.false_door_color(),
            LegendColor::TombAlcove => palette.tomb_alcove_color(),
            LegendColor::Stele => palette.stele_color(),
            LegendColor::Scroll => palette.scroll_color(),
            LegendColor::Vessel => palette.vessel_color(),
            LegendColor::Sculpture => palette.sculpture_color(),
            LegendColor::Coffin => palette.coffin_color(),
            LegendColor::Ostrakon => palette.ostrakon_color(),
            LegendColor::RockFace => palette.rock_face_color(),
        }
    }
}

/// Render the glyph legend panel, anchored to bottom-left of the viewport.
pub fn render_legend(
    palette: Palette,
    border_color: Color,
    area: Rect,
    buf: &mut Buffer,
) {
    let entry_count = LEGEND_ENTRIES.len() as u16;
    let panel_h = entry_count + 3; // top border + title + entries + bottom border
    let panel_w: u16 = 20; // "  ██ Wall Inscription" fits in 20

    if area.width < panel_w + 2 || area.height < panel_h + 1 {
        return;
    }

    // Anchor to top-right
    let x0 = area.right().saturating_sub(panel_w + 1);
    let y0 = area.y + 1;
    let panel = Rect::new(x0, y0, panel_w, panel_h);

    dim_rect(buf, panel);

    let border_style = Style::default().fg(border_color);
    let title_style = Style::default().fg(Color::Rgb(196, 146, 42)); // gold
    let label_style = Style::default().fg(Color::Rgb(180, 170, 155));

    // Top border
    let top = format!("┌{}┐", "─".repeat((panel_w - 2) as usize));
    buf.set_string(x0, y0, &top, border_style);

    // Title row
    let title = " Legend ";
    buf.set_string(x0, y0 + 1, "│", border_style);
    buf.set_string(x0 + 1, y0 + 1, title, title_style);
    // pad rest
    let title_w = display_width(title);
    let pad = (panel_w - 2) as usize - title_w;
    buf.set_string(x0 + 1 + title_w as u16, y0 + 1, &" ".repeat(pad), border_style);
    buf.set_string(x0 + panel_w - 1, y0 + 1, "│", border_style);

    // Entry rows
    for (i, (glyph, label, color_enum)) in LEGEND_ENTRIES.iter().enumerate() {
        let y = y0 + 2 + i as u16;
        let fg = color_enum.resolve(&palette);

        buf.set_string(x0, y, "│", border_style);
        buf.set_string(x0 + 1, y, " ", label_style);
        buf.set_string(x0 + 2, y, glyph, Style::default().fg(fg));

        // Label after glyph (glyph is 2 chars wide)
        let label_x = x0 + 5;
        let max_label = (panel_w - 6) as usize;
        let truncated: String = label.chars().take(max_label).collect();
        buf.set_string(label_x, y, &truncated, label_style);

        // Clear remaining space + right border
        let used = 5 + display_width(&truncated) as u16;
        if used < panel_w - 1 {
            let spaces = (panel_w - 1 - used) as usize;
            buf.set_string(x0 + used, y, &" ".repeat(spaces), label_style);
        }
        buf.set_string(x0 + panel_w - 1, y, "│", border_style);
    }

    // Bottom border
    let bot_y = y0 + 2 + entry_count;
    let bot = format!("└{}┘", "─".repeat((panel_w - 2) as usize));
    buf.set_string(x0, bot_y, &bot, border_style);
}
