use std::collections::HashMap;

use hex2d_stacks::Coordinate;
use ratatui::style::Color;

use crate::game::WorldTile;

const AMBIENT_FLOOR: f32 = 0.30;

/// A light-emitting position in the world.
pub struct LightSource {
    pub pos: Coordinate<i32>,
    pub radius: u8,
    pub temp_r: i8,
    pub temp_g: i8,
    pub temp_b: i8,
}

/// Chebyshev distance on a hex grid (cube-coordinate max-abs).
pub fn hex_distance(a: Coordinate<i32>, b: Coordinate<i32>) -> i32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    let dz = -(dx + dy);
    dx.abs().max(dy.abs()).max(dz.abs())
}

/// Gather all light sources from the tile map, plus the player torch.
pub fn collect_light_sources(
    tiles: &HashMap<Coordinate<i32>, WorldTile>,
    player_pos: Coordinate<i32>,
) -> Vec<LightSource> {
    let mut sources = Vec::new();

    // Player torch — always present, brightest source
    sources.push(LightSource {
        pos: player_pos,
        radius: 18,
        temp_r: 35,
        temp_g: 20,
        temp_b: -8,
    });

    for (pos, wt) in tiles {
        if wt.tile.is_light_source() {
            let (tr, tg, tb) = wt.tile.light_temperature();
            sources.push(LightSource {
                pos: *pos,
                radius: wt.tile.light_radius(),
                temp_r: tr,
                temp_g: tg,
                temp_b: tb,
            });
        }
    }

    sources
}

/// Apply radial lighting to a base color at a given coordinate.
///
/// Takes the maximum brightness factor across all sources (not additive),
/// applies color temperature from the nearest source, and clamps output.
pub fn apply_lighting(
    base_color: Color,
    coord: Coordinate<i32>,
    sources: &[LightSource],
) -> Color {
    let (r, g, b) = match base_color {
        Color::Rgb(r, g, b) => (r, g, b),
        other => return other,
    };

    let mut max_factor: f32 = AMBIENT_FLOOR;
    let mut nearest_dist = i32::MAX;
    let mut nearest_temp: (i8, i8, i8) = (0, 0, 0);

    for src in sources {
        let dist = hex_distance(coord, src.pos);
        let radius = src.radius as i32;

        if dist <= radius {
            // Quadratic falloff: bright near source, gentle decay
            let t = dist as f32 / radius as f32;
            let factor = 1.0 - t * t * 0.45;
            if factor > max_factor {
                max_factor = factor;
            }
        }

        if dist < nearest_dist {
            nearest_dist = dist;
            nearest_temp = (src.temp_r, src.temp_g, src.temp_b);
        }
    }

    // Scale temperature by proximity (closer = stronger tint)
    let prox = if nearest_dist <= 0 {
        1.0f32
    } else {
        (1.0 / nearest_dist as f32).min(1.0)
    };

    let tr = nearest_temp.0 as f32 * prox;
    let tg = nearest_temp.1 as f32 * prox;
    let tb = nearest_temp.2 as f32 * prox;

    let out_r = (r as f32 * max_factor + tr).clamp(0.0, 255.0) as u8;
    let out_g = (g as f32 * max_factor + tg).clamp(0.0, 255.0) as u8;
    let out_b = (b as f32 * max_factor + tb).clamp(0.0, 255.0) as u8;

    Color::Rgb(out_r, out_g, out_b)
}
