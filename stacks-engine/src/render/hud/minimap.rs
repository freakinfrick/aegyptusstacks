use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use crate::game::{ActorKind, world::{LoadedRoom, WorldMap}};
use super::dim_rect;
use super::Palette;

const HEX_BG: Color = Color::Rgb(15, 12, 8);

/// Render a fullscreen map with a custom camera center.
/// No border drawing — caller provides the clear rect.
pub fn render_fullscreen_map(
    world: &WorldMap,
    camera_center: [i32; 3],
    current_room: [i32; 3],
    rect: Rect,
    buf: &mut Buffer,
) {
    let w = rect.width;
    let h = rect.height;
    if w < 12 || h < 8 {
        return;
    }

    let current_z = camera_center[2];
    let cq = camera_center[0];
    let cr = camera_center[1];

    let inner_x = rect.x;
    let inner_y = rect.y;
    let inner_w = w as i32;
    let inner_h = h as i32;

    let cx = inner_w / 2 - 3;
    let cy = inner_h / 2 - 2;

    let bfs_to_hex = |dq: i32, dr: i32| -> (i32, i32) {
        (5 * dq + cx, -(2 * dq + 4 * dr) + cy)
    };

    let mut rooms_on_z: Vec<_> = world.rooms.values()
        .filter(|r| r.bfs_coord[2] == current_z)
        .collect();
    rooms_on_z.sort_by_key(|r| if r.bfs_coord == current_room { 1i32 } else { 0 });

    for room in &rooms_on_z {
        let dq = room.bfs_coord[0] - cq;
        let dr = room.bfs_coord[1] - cr;
        let (hx, hy) = bfs_to_hex(dq, dr);

        if hx + 6 < 0 || hx >= inner_w || hy + 4 < 0 || hy >= inner_h {
            continue;
        }

        let is_current = room.bfs_coord == current_room;
        let is_corridor = room.info.room_type == "corridor";

        let room_palette = Palette::from_room(
            room.bfs_coord[2],
            &room.info.district,
        );

        let (line1, line2, line3) = hex_content(room, is_current, is_corridor);
        let has_shaft = !room.info.shaft_edges.is_empty();

        let (hex_border_fg, hex_content_fg, bold) = if is_current {
            let player_color = ActorKind::Player.color();
            (player_color, player_color, true)
        } else if is_corridor {
            let c = visible_floor(dim_color(room_palette.wall_color(), 40));
            (c, c, false)
        } else {
            let wall = visible_floor(room_palette.wall_color());
            (wall, wall, false)
        };

        let line1_fg = if is_current {
            hex_content_fg
        } else if has_shaft {
            room_palette.shaft_color()
        } else {
            hex_content_fg
        };

        draw_hex(
            buf, hx, hy, inner_x, inner_y, inner_w, inner_h,
            &line1, &line2, &line3,
            hex_border_fg, line1_fg, hex_content_fg, bold,
        );
    }
}

/// Render the minimap as interlocking ASCII hexagons.
///
/// Each hex cell shows BFS room data in 2 metadata rows (5 chars each).
/// Corridors render as blank hex outlines.
///
/// Hex geometry (7 wide × 5 tall):
/// ```text
///   ___
///  /   \       row 1: player marker or blank
/// / Q,R \      row 2: coordinates
/// \info /      row 3: volume count / shaft
///  \___/
/// ```
///
/// Tiling stride: 5 cols horizontal, 4 rows vertical.
/// Adjacent hexes interlock — shared border characters at seams.
pub fn render_minimap(
    world: &WorldMap,
    current_room: [i32; 3],
    palette: Palette,
    rect: Rect,
    buf: &mut Buffer,
) {
    let w = rect.width;
    let h = rect.height;
    if w < 12 || h < 8 {
        return;
    }
    let x0 = rect.x;
    let y0 = rect.y;

    dim_rect(buf, rect);

    let border_style = Style::default().fg(dim_color(palette.wall_color(), 60));

    // ── Panel border ──────────────────────────────────────────

    // Top border (T-junction on left shared with z-column)
    buf.set_string(x0, y0, "┬", border_style);
    buf.set_string(x0 + 1, y0, " Map ", border_style);
    for i in 6..w.saturating_sub(1) {
        buf.set_string(x0 + i, y0, "─", border_style);
    }
    buf.set_string(x0 + w - 1, y0, "┤", border_style);

    // Side borders + dark interior fill
    let fill = Style::default().fg(HEX_BG).bg(HEX_BG);
    for i in 1..h.saturating_sub(1) {
        buf.set_string(x0, y0 + i, "│", border_style);
        for ix in (x0 + 1)..(x0 + w - 1) {
            buf.set_string(ix, y0 + i, " ", fill);
        }
        buf.set_string(x0 + w - 1, y0 + i, "│", border_style);
    }

    // Bottom border
    buf.set_string(x0, y0 + h - 1, "┴", border_style);
    for i in 1..w.saturating_sub(1) {
        buf.set_string(x0 + i, y0 + h - 1, "─", border_style);
    }
    buf.set_string(x0 + w - 1, y0 + h - 1, "┘", border_style);

    // ── Hex grid ──────────────────────────────────────────────

    let current_z = current_room[2];
    let cq = current_room[0];
    let cr = current_room[1];

    let inner_x = x0 + 1;
    let inner_y = y0 + 1;
    let inner_w = (w.saturating_sub(2)) as i32;
    let inner_h = (h.saturating_sub(2)) as i32;

    // Center hex bounding-box top-left in inner coords
    let cx = inner_w / 2 - 3;
    let cy = inner_h / 2 - 2;

    // BFS (dq, dr) → hex top-left (col, row) in inner coords.
    // Stride: 5 cols per BFS q-step, 4 rows per BFS r-step.
    let bfs_to_hex = |dq: i32, dr: i32| -> (i32, i32) {
        (5 * dq + cx, -(2 * dq + 4 * dr) + cy)
    };

    // Collect rooms on current z-level, draw current room last (on top)
    let mut rooms_on_z: Vec<_> = world.rooms.values()
        .filter(|r| r.bfs_coord[2] == current_z)
        .collect();
    rooms_on_z.sort_by_key(|r| if r.bfs_coord == current_room { 1i32 } else { 0 });

    for room in &rooms_on_z {
        let dq = room.bfs_coord[0] - cq;
        let dr = room.bfs_coord[1] - cr;
        let (hx, hy) = bfs_to_hex(dq, dr);

        // Cull hexes entirely outside inner area
        if hx + 6 < 0 || hx >= inner_w || hy + 4 < 0 || hy >= inner_h {
            continue;
        }

        let is_current = room.bfs_coord == current_room;
        let is_corridor = room.info.room_type == "corridor";

        // Per-room palette: z-level + district → wall colors match the main viewport
        let room_palette = Palette::from_room(
            room.bfs_coord[2],
            &room.info.district,
        );

        let (line1, line2, line3) = hex_content(room, is_current, is_corridor);

        let has_shaft = !room.info.shaft_edges.is_empty();

        let (hex_border_fg, hex_content_fg, bold) = if is_current {
            let player_color = ActorKind::Player.color();
            (player_color, player_color, true)
        } else if is_corridor {
            let c = visible_floor(dim_color(room_palette.wall_color(), 40));
            (c, c, false)
        } else {
            let wall = visible_floor(room_palette.wall_color());
            (wall, wall, false)
        };

        // Shaft arrows get shaft_color; player marker keeps its color
        let line1_fg = if is_current {
            hex_content_fg
        } else if has_shaft {
            room_palette.shaft_color()
        } else {
            hex_content_fg
        };

        draw_hex(
            buf, hx, hy, inner_x, inner_y, inner_w, inner_h,
            &line1, &line2, &line3,
            hex_border_fg, line1_fg, hex_content_fg, bold,
        );
    }
}

// ── Hex content ──────────────────────────────────────────────

/// Compute the three content lines for a hex cell.
/// Corridors are blank outlines. Rooms show coords + info.
fn hex_content(room: &LoadedRoom, is_current: bool, is_corridor: bool) -> (String, String, String) {
    // Line 1 (3 chars): player marker, shaft arrows, or blank
    let line1 = if is_current {
        let g = ActorKind::Player.minimap_glyph();
        format!(" {} ", g)
    } else if !room.info.shaft_edges.is_empty() {
        let has_up = room.info.shaft_edges.iter().any(|e| e.direction == "UP");
        let has_down = room.info.shaft_edges.iter().any(|e| e.direction == "DOWN");
        match (has_up, has_down) {
            (true, true)   => " ↕ ".to_string(),
            (true, false)  => " ↑ ".to_string(),
            (false, true)  => " ↓ ".to_string(),
            (false, false) => "   ".to_string(),
        }
    } else {
        "   ".to_string()
    };

    // Corridors: blank interior
    if is_corridor {
        return (line1, "     ".to_string(), "     ".to_string());
    }

    // Line 2 (5 chars): Q,R coordinates
    let qr = format!("{},{}", room.bfs_coord[0], room.bfs_coord[1]);
    let line2 = fit(5, &qr);

    // Line 3 (5 chars): volume count
    let line3 = if room.info.volume_count > 0 {
        fit(5, &format!("v:{}", room.info.volume_count.min(999)))
    } else {
        "  ·  ".to_string()
    };

    (line1, line2, line3)
}

/// Center string in `width` display columns; truncate if wider.
fn fit(width: usize, s: &str) -> String {
    crate::render::text_util::pad_center(s, width)
}

// ── Hex drawing ──────────────────────────────────────────────

/// Draw a single ASCII hex at inner position (hx, hy).
///
/// ```text
///   ___          row 0: underscores at cols 2-4
///  /aaa\         row 1: 3 content chars at cols 2-4
/// /bbbbb\        row 2: 5 content chars at cols 1-5
/// \ccccc/        row 3: 5 content chars at cols 1-5
///  \___/         row 4: underscores at cols 2-4
/// ```
fn draw_hex(
    buf: &mut Buffer,
    hx: i32, hy: i32,
    ix: u16, iy: u16, iw: i32, ih: i32,
    line1: &str, line2: &str, line3: &str,
    border_fg: Color, line1_fg: Color, content_fg: Color, bold: bool,
) {
    let bdr = Style::default().fg(border_fg).bg(HEX_BG);
    let l1 = if bold {
        Style::default().fg(line1_fg).bg(HEX_BG).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(line1_fg).bg(HEX_BG)
    };
    let cnt = if bold {
        Style::default().fg(content_fg).bg(HEX_BG).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(content_fg).bg(HEX_BG)
    };

    // Row 0:  ___
    put(buf, hx + 2, hy,     ix, iy, iw, ih, '_', bdr);
    put(buf, hx + 3, hy,     ix, iy, iw, ih, '_', bdr);
    put(buf, hx + 4, hy,     ix, iy, iw, ih, '_', bdr);

    // Row 1: /aaa\
    put(buf, hx + 1, hy + 1, ix, iy, iw, ih, '/', bdr);
    for (i, ch) in line1.chars().enumerate().take(3) {
        put(buf, hx + 2 + i as i32, hy + 1, ix, iy, iw, ih, ch, l1);
    }
    put(buf, hx + 5, hy + 1, ix, iy, iw, ih, '\\', bdr);

    // Row 2: /bbbbb\
    put(buf, hx,     hy + 2, ix, iy, iw, ih, '/', bdr);
    for (i, ch) in line2.chars().enumerate().take(5) {
        put(buf, hx + 1 + i as i32, hy + 2, ix, iy, iw, ih, ch, cnt);
    }
    put(buf, hx + 6, hy + 2, ix, iy, iw, ih, '\\', bdr);

    // Row 3: \ccccc/
    put(buf, hx,     hy + 3, ix, iy, iw, ih, '\\', bdr);
    for (i, ch) in line3.chars().enumerate().take(5) {
        put(buf, hx + 1 + i as i32, hy + 3, ix, iy, iw, ih, ch, cnt);
    }
    put(buf, hx + 6, hy + 3, ix, iy, iw, ih, '/', bdr);

    // Row 4: \___/
    put(buf, hx + 1, hy + 4, ix, iy, iw, ih, '\\', bdr);
    put(buf, hx + 2, hy + 4, ix, iy, iw, ih, '_', bdr);
    put(buf, hx + 3, hy + 4, ix, iy, iw, ih, '_', bdr);
    put(buf, hx + 4, hy + 4, ix, iy, iw, ih, '_', bdr);
    put(buf, hx + 5, hy + 4, ix, iy, iw, ih, '/', bdr);
}

/// Place a character at inner coords (col, row) if within bounds.
#[inline]
fn put(
    buf: &mut Buffer,
    col: i32, row: i32,
    ix: u16, iy: u16, iw: i32, ih: i32,
    ch: char, style: Style,
) {
    if col >= 0 && col < iw && row >= 0 && row < ih {
        let pos = ratatui::layout::Position {
            x: ix + col as u16,
            y: iy + row as u16,
        };
        if let Some(cell) = buf.cell_mut(pos) {
            cell.set_char(ch);
            cell.set_style(style);
        }
    }
}

/// Darken a color by subtracting `amount` from each channel.
fn dim_color(color: Color, amount: u8) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(
            r.saturating_sub(amount),
            g.saturating_sub(amount),
            b.saturating_sub(amount),
        ),
        c => c,
    }
}

/// Ensure a color stays visible against the dark HEX_BG.
/// Lifts each channel to a warm-tinted minimum floor.
fn visible_floor(color: Color) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(r.max(40), g.max(35), b.max(30)),
        c => c,
    }
}
