//! Agent state dump — writes /tmp/stacks-state.json each frame.
//!
//! Provides structured game state for an external AI agent
//! that plays the game via tmux send-keys + capture-pane.
//! This is the "structured hint" sidecar used during calibration;
//! the goal is to eventually tune down to screen-only observation.

use std::io::Write;

use hex2d_stacks::{Coordinate, Direction};
use serde::Serialize;

use super::engine::Engine;
use super::tile::Tile;
use super::world::WorldMap;

const STATE_PATH: &str = "/tmp/stacks-state.json";

/// Track last written turn to skip redundant writes.
static LAST_WRITTEN_TURN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);

/// Top-level agent state snapshot.
#[derive(Serialize)]
pub struct AgentState {
    pub turn: u64,
    pub player: PlayerState,
    pub room: RoomState,
    pub nearby_tiles: Vec<NearbyTile>,
    pub nearby_actors: Vec<NearbyActor>,
    pub popup: Option<String>,
    pub hud_mode: String,
    pub available_actions: Vec<String>,
}

#[derive(Serialize)]
pub struct PlayerState {
    pub hex_x: i32,
    pub hex_y: i32,
    pub facing: String,
}

#[derive(Serialize)]
pub struct RoomState {
    pub name: String,
    pub district: String,
    pub room_type: String,
    pub codex: [i32; 3],
    pub volume_count: u32,
    pub fixture_count: usize,
}

#[derive(Serialize)]
pub struct NearbyTile {
    pub direction: String,
    pub distance: i32,
    pub tile: String,
    pub passable: bool,
    pub text_bearing: bool,
}

#[derive(Serialize)]
pub struct NearbyActor {
    pub kind: String,
    pub direction: String,
    pub distance: i32,
}

fn dir_name(d: Direction) -> &'static str {
    match d {
        Direction::YZ => "NW",
        Direction::XZ => "NE",
        Direction::XY => "E",
        Direction::ZY => "SE",
        Direction::ZX => "SW",
        Direction::YX => "W",
    }
}

fn hex_distance(a: Coordinate<i32>, b: Coordinate<i32>) -> i32 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let dz = -dx - dy;
    (dx.abs() + dy.abs() + dz.abs()) / 2
}

/// Approximate direction from `from` to `to` on the hex grid.
fn approx_direction(from: Coordinate<i32>, to: Coordinate<i32>) -> &'static str {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let dz = -dx - dy;

    // Find the axis with the largest absolute delta
    let ax = dx.abs();
    let ay = dy.abs();
    let az = dz.abs();

    if ax >= ay && ax >= az {
        if dx > 0 { "E" } else { "W" }
    } else if ay >= ax && ay >= az {
        if dy > 0 { "NW" } else { "SE" }
    } else {
        if dz > 0 { "SW" } else { "NE" }
    }
}

/// Build and write the agent state snapshot.
/// Skips write if turn hasn't changed (no-op most frames).
pub fn write_agent_state(
    engine: &Engine,
    world: &WorldMap,
    popup_name: Option<&str>,
    hud_mode: &str,
) {
    let current_turn = engine.turn;
    let prev = LAST_WRITTEN_TURN.load(std::sync::atomic::Ordering::Relaxed);
    if prev == current_turn {
        return;
    }
    LAST_WRITTEN_TURN.store(current_turn, std::sync::atomic::Ordering::Relaxed);

    let player_pos = engine.player.coord();
    let player_facing = engine.player.facing();
    let room_coord = engine.current_room;

    // Room info
    let (room_name, district, room_type, volume_count) = world.rooms.get(&room_coord)
        .map(|r| {
            let name = super::room_name_from_cluster(
                &r.info.district, &r.info.cluster_signature, &r.info.room_type,
            );
            (
                name,
                r.info.district.clone(),
                r.info.room_type.clone(),
                r.info.volume_count,
            )
        })
        .unwrap_or_else(|| ("Unknown".into(), "unknown".into(), "unknown".into(), 0));

    let fixture_count = world.fixtures.get(&room_coord)
        .map(|f| f.len())
        .unwrap_or(0);

    // Nearby tiles: scan range 2 around player
    let mut nearby_tiles = Vec::new();
    for &dir in Direction::all() {
        for dist in 1..=2i32 {
            let mut target = player_pos;
            for _ in 0..dist {
                target = target + dir;
            }
            if let Some(wt) = world.tiles.get(&target) {
                nearby_tiles.push(NearbyTile {
                    direction: dir_name(dir).to_string(),
                    distance: dist,
                    tile: wt.tile.description().to_string(),
                    passable: wt.tile.is_passable(),
                    text_bearing: wt.tile.is_text_bearing(),
                });
            }
        }
    }

    // Also report the tile the player is standing on
    if let Some(wt) = world.tiles.get(&player_pos) {
        nearby_tiles.push(NearbyTile {
            direction: "HERE".to_string(),
            distance: 0,
            tile: wt.tile.description().to_string(),
            passable: true,
            text_bearing: wt.tile.is_text_bearing(),
        });
    }

    // Nearby actors
    let mut nearby_actors = Vec::new();
    for lib in &engine.librarians {
        let dist = hex_distance(player_pos, lib.coord());
        if dist <= 4 {
            nearby_actors.push(NearbyActor {
                kind: "Librarian".to_string(),
                direction: approx_direction(player_pos, lib.coord()).to_string(),
                distance: dist,
            });
        }
    }
    for scribe in &engine.scribes {
        let dist = hex_distance(player_pos, scribe.coord());
        if dist <= 4 {
            nearby_actors.push(NearbyActor {
                kind: "Scribe".to_string(),
                direction: approx_direction(player_pos, scribe.coord()).to_string(),
                distance: dist,
            });
        }
    }

    // Available actions based on current state
    let mut available = vec!["move".to_string(), "wait".to_string()];

    // Check if standing on shaft/staircase
    if let Some(wt) = world.tiles.get(&player_pos) {
        if wt.tile.is_shaft() || wt.tile == Tile::Staircase {
            available.push("ascend".to_string());
            available.push("descend".to_string());
        }
        if wt.tile == Tile::Lectern {
            available.push("interact_lectern".to_string());
        }
        if wt.tile == Tile::ArticleDisplay {
            available.push("interact_article".to_string());
        }
    }

    // Check for nearby interactables
    let has_nearby_fixture = nearby_tiles.iter().any(|t| t.text_bearing && t.distance <= 2);
    if has_nearby_fixture {
        available.push("interact_fixture".to_string());
    }

    let has_nearby_npc = nearby_actors.iter().any(|a| a.distance <= 2);
    if has_nearby_npc {
        available.push("interact_npc".to_string());
    }

    let state = AgentState {
        turn: engine.turn,
        player: PlayerState {
            hex_x: player_pos.x,
            hex_y: player_pos.y,
            facing: dir_name(player_facing).to_string(),
        },
        room: RoomState {
            name: room_name,
            district,
            room_type,
            codex: room_coord,
            volume_count,
            fixture_count,
        },
        nearby_tiles,
        nearby_actors,
        popup: popup_name.map(|s| s.to_string()),
        hud_mode: hud_mode.to_string(),
        available_actions: available,
    };

    // Write atomically via temp file + rename
    let tmp = format!("{}.tmp", STATE_PATH);
    if let Ok(mut f) = std::fs::File::create(&tmp) {
        if serde_json::to_writer_pretty(&mut f, &state).is_ok() {
            let _ = f.flush();
            let _ = std::fs::rename(&tmp, STATE_PATH);
        }
    }
}

// ── Tile grid dump ──────────────────────────────────────────────

const TILES_PATH: &str = "/tmp/stacks-tiles.json";

#[derive(Serialize)]
struct TileGridDump {
    player: [i32; 2],
    room_codex: [i32; 3],
    tile_count: usize,
    /// Flat list: each entry is [x, y, passable(0/1), text_bearing(0/1), room_q, room_r, room_z, tile_type_id]
    /// Compact format — JSON object-per-tile is too large for thousands of tiles.
    tiles: Vec<[i32; 8]>,
    /// Doors between rooms: [[from_q,from_r,from_z, to_q,to_r,to_z, direction_str_idx], ...]
    doors: Vec<String>,
}

/// Dump the tile grid to /tmp/stacks-tiles.json.
/// Only writes tiles for the current room + immediate neighbors (not entire world).
/// Spawns a background thread to avoid blocking the game loop.
pub fn write_tile_grid(world: &WorldMap, player_pos: Coordinate<i32>, room_codex: [i32; 3]) {
    // Collect only tiles belonging to current room and its door/shaft neighbors
    let mut relevant_rooms: std::collections::HashSet<[i32; 3]> = std::collections::HashSet::new();
    relevant_rooms.insert(room_codex);
    if let Some(room) = world.rooms.get(&room_codex) {
        for edge in &room.info.door_edges {
            relevant_rooms.insert(edge.to_coord);
        }
        for edge in &room.info.shaft_edges {
            relevant_rooms.insert(edge.to_coord);
        }
    }

    let mut tiles: Vec<[i32; 8]> = Vec::new();
    for (coord, wt) in &world.tiles {
        if relevant_rooms.contains(&wt.room_coord) {
            tiles.push([
                coord.x,
                coord.y,
                wt.tile.is_passable() as i32,
                wt.tile.is_text_bearing() as i32,
                wt.room_coord[0],
                wt.room_coord[1],
                wt.room_coord[2],
                wt.tile.type_id(),
            ]);
        }
    }

    let mut doors = Vec::new();
    for &rc in &relevant_rooms {
        if let Some(room) = world.rooms.get(&rc) {
            for edge in &room.info.door_edges {
                doors.push(format!(
                    "{},{},{} -> {},{},{} {}",
                    rc[0], rc[1], rc[2],
                    edge.to_coord[0], edge.to_coord[1], edge.to_coord[2],
                    edge.direction,
                ));
            }
            for edge in &room.info.shaft_edges {
                doors.push(format!(
                    "{},{},{} -> {},{},{} {} shaft",
                    rc[0], rc[1], rc[2],
                    edge.to_coord[0], edge.to_coord[1], edge.to_coord[2],
                    edge.direction,
                ));
            }
        }
    }

    let dump = TileGridDump {
        player: [player_pos.x, player_pos.y],
        room_codex,
        tile_count: tiles.len(),
        tiles,
        doors,
    };

    // Write on a background thread to avoid blocking the game loop
    std::thread::spawn(move || {
        let tmp = format!("{}.tmp", TILES_PATH);
        if let Ok(mut f) = std::fs::File::create(&tmp) {
            if serde_json::to_writer(&mut f, &dump).is_ok() {
                let _ = f.flush();
                let _ = std::fs::rename(&tmp, TILES_PATH);
            }
        }
    });
}

// ── Path request / response ─────────────────────────────────────

const PATH_REQ: &str = "/tmp/stacks-pathreq.json";
const PATH_RES: &str = "/tmp/stacks-path.json";

#[derive(serde::Deserialize)]
struct PathRequest {
    to_x: i32,
    to_y: i32,
}

#[derive(Serialize)]
struct PathResponse {
    found: bool,
    distance: usize,
    /// Keystroke sequence: ["d", "d", "e", "c", ...]
    keys: Vec<String>,
    /// Direction names: ["E", "E", "NE", "SE", ...]
    directions: Vec<String>,
    /// Coordinate sequence: [[x0,y0], [x1,y1], ...]
    waypoints: Vec<[i32; 2]>,
}

/// Check for a path request file, compute the path, write the response.
/// Called each frame from the main loop.
pub fn check_path_request(engine: &Engine, world: &WorldMap) {
    // Quick existence check (no allocation on miss)
    if std::fs::metadata(PATH_REQ).is_err() {
        return;
    }

    let req: PathRequest = match std::fs::read_to_string(PATH_REQ)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
    {
        Some(r) => r,
        None => {
            let _ = std::fs::remove_file(PATH_REQ);
            return;
        }
    };

    // Delete request immediately to avoid re-processing
    let _ = std::fs::remove_file(PATH_REQ);

    let from = engine.player.coord();
    let to = Coordinate::new(req.to_x, req.to_y);

    let response = match super::pathfinding::astar_full_path(from, to, world, 200) {
        Some(dirs) => {
            let mut waypoints = vec![[from.x, from.y]];
            let mut pos = from;
            for &d in &dirs {
                pos = pos + d;
                waypoints.push([pos.x, pos.y]);
            }
            PathResponse {
                found: true,
                distance: dirs.len(),
                keys: dirs.iter().map(|d| super::pathfinding::direction_to_key(*d).to_string()).collect(),
                directions: dirs.iter().map(|d| super::pathfinding::direction_to_compass(*d).to_string()).collect(),
                waypoints,
            }
        }
        None => PathResponse {
            found: false,
            distance: 0,
            keys: Vec::new(),
            directions: Vec::new(),
            waypoints: Vec::new(),
        },
    };

    let tmp = format!("{}.tmp", PATH_RES);
    if let Ok(mut f) = std::fs::File::create(&tmp) {
        if serde_json::to_writer_pretty(&mut f, &response).is_ok() {
            let _ = f.flush();
            let _ = std::fs::rename(&tmp, PATH_RES);
        }
    }
}

/// Clean up all agent state files on shutdown.
pub fn remove_agent_state() {
    for path in &[STATE_PATH, TILES_PATH, PATH_REQ, PATH_RES] {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(format!("{}.tmp", path));
    }
}
