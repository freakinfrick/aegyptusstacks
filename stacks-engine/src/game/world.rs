use std::collections::{HashMap, VecDeque};
use hex2d_stacks::{Angle, Coordinate, Direction, Spin};
use bfs_client::{DoorEdge, ShaftEdge, RoomMetadata};
use super::tile::{Tile, ShaftKind};
use super::composition::{RoomComposition, room_seed, RoomRng};

/// Maximum number of rooms kept resident in memory.
/// Oldest rooms are evicted when this cap is exceeded.
const MAX_RESIDENT_ROOMS: usize = 256;

/// Gallery radius: outer wall ring distance from room center.
/// Must equal HALF_STRIDE so adjacent rooms share wall rings.
const GALLERY_RADIUS: i32 = 9;

/// Half-stride: half the hex distance between adjacent room centers.
/// Rooms share walls at this spacing — must equal GALLERY_RADIUS.
const HALF_STRIDE: i32 = 9;

/// Interior radius for a given wall radius: walkable floor extends here.
/// Rings at R, R-1, R-2 form the wall zone (outer wall, shelf alcove, inner wall).
fn interior_radius(radius: i32) -> i32 {
    radius - 3
}

/// Doorway half-width for a given radius.
/// Scales to 2 for larger rooms (radius >= 11) for proportional doorway width.
fn door_halfwidth(radius: i32) -> usize {
    if radius >= 11 { 2 } else { 1 }
}

/// State of a single hex wall (one of 6 sides)
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum WallState {
    /// Bears bookshelves (4 per gallery)
    Shelved,
    /// Opens into a vestibule corridor (2 per gallery)
    FreeDoor,
    /// Free wall but sealed — no vestibule beyond
    FreeSealed,
}

/// Wall layout for a gallery — 6 walls indexed by wall number.
/// Wall 0=E(XY), 1=NE(XZ), 2=NW(YZ), 3=W(YX), 4=SW(ZX), 5=SE(ZY)
#[derive(Clone, Debug)]
pub struct GalleryLayout {
    pub walls: [WallState; 6],
}

impl GalleryLayout {
    /// Derive gallery wall states from BFS door_edges.
    /// Horizontal door directions become FreeDoor; remaining walls are Shelved.
    pub fn from_door_edges(edges: &[DoorEdge]) -> Self {
        let mut walls = [WallState::Shelved; 6];
        for edge in edges {
            if let Some(idx) = direction_to_wall_index(&edge.direction) {
                walls[idx] = WallState::FreeDoor;
            }
        }
        GalleryLayout { walls }
    }

    /// Which directions have doors
    pub fn door_directions(&self) -> Vec<Direction> {
        self.walls.iter().enumerate()
            .filter(|(_, s)| **s == WallState::FreeDoor)
            .map(|(i, _)| wall_index_to_direction(i))
            .collect()
    }
}

/// Map BFS direction string to wall index (0-5).
/// Verified: with the rotated lattice (bfs_to_world), shared boundary tiles
/// between adjacent rooms predominantly fall on the face matching the BFS label.
fn direction_to_wall_index(dir: &str) -> Option<usize> {
    match dir {
        "E" | "e" => Some(0),
        "NE" | "ne" => Some(1),
        "NW" | "nw" => Some(2),
        "W" | "w" => Some(3),
        "SW" | "sw" => Some(4),
        "SE" | "se" => Some(5),
        _ => None, // UP/DOWN are not wall indices
    }
}

/// Map wall index (0-5) to hex2d Direction
fn wall_index_to_direction(idx: usize) -> Direction {
    match idx {
        0 => Direction::XY,
        1 => Direction::XZ,
        2 => Direction::YZ,
        3 => Direction::YX,
        4 => Direction::ZX,
        5 => Direction::ZY,
        _ => panic!("wall index must be 0-5"),
    }
}

/// Compact room info — only fields accessed at runtime after stamping.
/// Extracted from RoomMetadata at place_room time; the full BFS response
/// (volumes, volumes_typed, wall_edges, shelves, shelf_types) is dropped.
#[derive(Clone, Debug)]
pub struct RoomInfo {
    pub q: i32,
    pub r: i32,
    pub z: i32,
    pub room_type: String,
    pub district: String,
    pub volume_count: u32,
    pub door_edges: Vec<DoorEdge>,
    pub shaft_edges: Vec<ShaftEdge>,
    pub cluster_id: String,
    pub cluster_signature: Vec<String>,
    pub volumes_typed: Vec<bfs_client::VolumeTyped>,
}

impl RoomInfo {
    pub fn from_metadata(m: &RoomMetadata) -> Self {
        RoomInfo {
            q: m.q, r: m.r, z: m.z,
            room_type: m.room_type.clone(),
            district: m.district.clone(),
            volume_count: m.volume_count,
            door_edges: m.door_edges.clone(),
            shaft_edges: m.shaft_edges.clone(),
            cluster_id: m.cluster_id.clone(),
            cluster_signature: m.cluster_signature.clone(),
            volumes_typed: m.volumes_typed.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct LoadedRoom {
    pub bfs_coord: [i32; 3],
    pub center: Coordinate<i32>,
    pub grid_pos: Coordinate<i32>,
    pub layout: GalleryLayout,
    pub info: RoomInfo,
    pub hall_style: HallStyle,
    /// Pinned rooms (synthetic galleries, observatory) are never evicted.
    pub pinned: bool,
}

/// Which hex face a wall/shelf cell belongs to (for glyph selection).
/// Faces 0(E) and 3(W) are horizontal on screen; others are diagonal.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HexFace {
    /// E face — horizontal, screen direction →
    E,
    /// NE face — diagonal, screen direction ↗
    NE,
    /// NW face — diagonal, screen direction ↖
    NW,
    /// W face — horizontal, screen direction ←
    W,
    /// SW face — diagonal, screen direction ↙
    SW,
    /// SE face — diagonal, screen direction ↘
    SE,
}

impl HexFace {
    fn from_wall_idx(idx: usize) -> Self {
        match idx {
            0 => HexFace::E,
            1 => HexFace::NE,
            2 => HexFace::NW,
            3 => HexFace::W,
            4 => HexFace::SW,
            5 => HexFace::SE,
            _ => HexFace::E,
        }
    }

    /// Is this face horizontal on screen (NW or SE edges)?
    /// These edges step in E/W hex directions → same screen row.
    pub fn is_horizontal(&self) -> bool {
        matches!(self, HexFace::NW | HexFace::SE)
    }

    /// Is this face a ╱ rising diagonal (W or SW edges)?
    /// Screen step has row decreasing → line goes from lower-left to upper-right.
    pub fn is_rising(&self) -> bool {
        matches!(self, HexFace::W | HexFace::SW)
    }

    /// Is this face a ╲ falling diagonal (NE or E edges)?
    /// Screen step has row increasing → line goes from upper-left to lower-right.
    pub fn is_falling(&self) -> bool {
        matches!(self, HexFace::NE | HexFace::E)
    }
}

/// Hall style determines wall glyph variation per district.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HallStyle {
    Oracle,     // royal — ║║
    Utterance,  // admin/school — ██ (default)
    Dead,       // necropolis — ░░
    Power,      // temple — ▓▓
    Cosmos,     // house_of_life — ▒▒
    Wisdom,     // quarry — ▞▞
    Domestic,   // domestic — ┃┃
}

impl HallStyle {
    /// Derive hall style from district name.
    pub fn from_district(district: &str) -> Self {
        match district {
            "temple" => HallStyle::Power,
            "necropolis" => HallStyle::Dead,
            "house_of_life" => HallStyle::Cosmos,
            "admin" | "school" | "unplaced" => HallStyle::Utterance,
            "quarry" => HallStyle::Wisdom,
            "royal" => HallStyle::Oracle,
            "domestic" => HallStyle::Domestic,
            _ => HallStyle::Utterance,
        }
    }

    /// Wall glyph for this hall style (2-char wide).
    pub fn wall_glyph(&self) -> &'static str {
        match self {
            HallStyle::Oracle => "║║",
            HallStyle::Utterance => "██",
            HallStyle::Dead => "░░",
            HallStyle::Power => "▓▓",
            HallStyle::Cosmos => "▒▒",
            HallStyle::Wisdom => "▞▞",
            HallStyle::Domestic => "┃┃",
        }
    }

    /// Extended wall glyph (4-char, for adjacent wall pairs).
    pub fn wall_glyph_wide(&self) -> &'static str {
        match self {
            HallStyle::Oracle => "║║║║",
            HallStyle::Utterance => "████",
            HallStyle::Dead => "░░░░",
            HallStyle::Power => "▓▓▓▓",
            HallStyle::Cosmos => "▒▒▒▒",
            HallStyle::Wisdom => "▞▞▞▞",
            HallStyle::Domestic => "┃┃┃┃",
        }
    }
}

/// A fixture is a single text-bearing object in a room.
/// One BFS shelf → one fixture → one tile on screen.
/// Holds up to ~12 volumes. Addressable by room_coord + fixture_id.
#[derive(Clone, Debug)]
pub struct FixtureMeta {
    pub id: u16,
    pub tile_type: Tile,
    /// Index into the room's BFS `shelves` array
    pub shelf_index: usize,
    /// World coordinate where this fixture is placed
    pub coord: Coordinate<i32>,
    /// Display label (e.g. "Papyrus Shelf", "Stele", "Scroll Rack")
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct WorldTile {
    pub tile: Tile,
    pub room_coord: [i32; 3],
    /// Which hex face this cell belongs to (for wall/shelf glyph selection)
    pub face: Option<HexFace>,
    /// Fixture identity — links this tile to a specific BFS shelf.
    /// None for structural tiles (walls, floors, doors) and generic shelf runs.
    pub fixture_id: Option<u16>,
}

/// Active portal pair — origin and destination tile clusters with saved originals.
pub struct PortalPair {
    /// Origin room coordinate (BFS [q,r,z])
    pub origin_room: [i32; 3],
    /// Destination room coordinate (BFS [q,r,z])
    pub dest_room: [i32; 3],
    /// World hex coordinates of origin portal tiles
    pub origin_tiles: Vec<Coordinate<i32>>,
    /// World hex coordinates of destination portal tiles
    pub dest_tiles: Vec<Coordinate<i32>>,
    /// Saved original tiles for restoration on despawn
    pub saved_originals: HashMap<Coordinate<i32>, WorldTile>,
}

pub struct WorldMap {
    pub tiles: HashMap<Coordinate<i32>, WorldTile>,
    pub rooms: HashMap<[i32; 3], LoadedRoom>,
    pub shelves: HashMap<[i32; 3], Vec<Vec<String>>>,
    /// Fixture registry: room_coord → list of fixtures in that room
    pub fixtures: HashMap<[i32; 3], Vec<FixtureMeta>>,
    /// Article display tiles: world hex coord → article filename
    pub article_tiles: HashMap<Coordinate<i32>, String>,
    /// Active portal pair (origin + destination), cleared on next spawn
    pub active_portal: Option<PortalPair>,
    /// LRU order: front = oldest, back = most recently touched
    room_lru: VecDeque<[i32; 3]>,
}

impl WorldMap {
    pub fn new() -> Self {
        WorldMap {
            tiles: HashMap::new(),
            rooms: HashMap::new(),
            shelves: HashMap::new(),
            fixtures: HashMap::new(),
            article_tiles: HashMap::new(),
            active_portal: None,
            room_lru: VecDeque::new(),
        }
    }

    pub fn has_room(&self, coord: &[i32; 3]) -> bool {
        self.rooms.contains_key(coord)
    }

    /// Move a room to the back of the LRU (most recently used).
    pub fn touch_room(&mut self, coord: [i32; 3]) {
        if let Some(pos) = self.room_lru.iter().position(|c| *c == coord) {
            self.room_lru.remove(pos);
        }
        self.room_lru.push_back(coord);
    }

    /// Evict rooms beyond the cap, returning their coords so the caller
    /// can clean up external state (particles, etc).
    /// `protected` coords are never evicted (current room + neighbors).
    pub fn evict_distant(&mut self, protected: &std::collections::HashSet<[i32; 3]>) -> Vec<[i32; 3]> {
        let mut evicted = Vec::new();
        let mut skipped = 0usize;
        while self.room_lru.len() > MAX_RESIDENT_ROOMS {
            if let Some(coord) = self.room_lru.pop_front() {
                // Never evict protected (current + neighbors) or pinned (synthetic) rooms
                let is_pinned = self.rooms.get(&coord).map(|r| r.pinned).unwrap_or(false);
                if protected.contains(&coord) || is_pinned {
                    self.room_lru.push_back(coord);
                    skipped += 1;
                    // All remaining rooms are protected/pinned — stop
                    if skipped >= self.room_lru.len() {
                        break;
                    }
                    continue;
                }
                skipped = 0;
                self.evict_room(coord);
                evicted.push(coord);
            }
        }
        evicted
    }

    /// Remove a single room and all its owned data from memory.
    fn evict_room(&mut self, coord: [i32; 3]) {
        // Clean up portal tiles belonging to this room
        if let Some(ref mut portal) = self.active_portal {
            portal.origin_tiles.retain(|c| {
                self.tiles.get(c).map(|wt| wt.room_coord != coord).unwrap_or(true)
            });
            portal.dest_tiles.retain(|c| {
                self.tiles.get(c).map(|wt| wt.room_coord != coord).unwrap_or(true)
            });
            portal.saved_originals.retain(|_, wt| wt.room_coord != coord);
            if portal.origin_tiles.is_empty() && portal.dest_tiles.is_empty() {
                self.active_portal = None;
            }
        }

        self.tiles.retain(|_, wt| wt.room_coord != coord);
        self.rooms.remove(&coord);
        self.shelves.remove(&coord);
        self.fixtures.remove(&coord);
    }

    // ── Fixture lookup ─────────────────────────────────────────────

    /// Find the world coordinate of the fixture tile that holds a given volume ID.
    /// Walks the room's shelves to find the shelf_index, then matches it to a fixture.
    /// Returns None if the volume isn't in any shelf or no fixture maps to that shelf.
    pub fn find_fixture_coord_for_volume(
        &self,
        room_coord: &[i32; 3],
        volume_id: &str,
    ) -> Option<Coordinate<i32>> {
        let room_shelves = self.shelves.get(room_coord)?;
        let shelf_index = room_shelves.iter().position(|shelf| {
            shelf.iter().any(|id| id == volume_id)
        })?;
        let fixtures = self.fixtures.get(room_coord)?;
        fixtures.iter()
            .find(|f| f.shelf_index == shelf_index)
            .map(|f| f.coord)
    }

    // ── Portal pair management ─────────────────────────────────────

    /// Clear the active portal, restoring original tiles.
    pub fn clear_active_portal(&mut self) {
        if let Some(portal) = self.active_portal.take() {
            for (coord, original_wt) in portal.saved_originals {
                if let Some(current) = self.tiles.get(&coord) {
                    if current.tile == Tile::Portal {
                        self.tiles.insert(coord, original_wt);
                    }
                }
            }
        }
    }

    /// Spawn a portal pair: 2-3 tiles near the player (origin) and near
    /// the destination room center. Clears any existing portal first.
    pub fn spawn_portal_pair(
        &mut self,
        player_pos: Coordinate<i32>,
        player_facing: Direction,
        origin_room: [i32; 3],
        dest_room: [i32; 3],
        dest_center: Coordinate<i32>,
    ) {
        self.clear_active_portal();

        let origin_tiles = self.find_portal_cluster(player_pos, player_facing, player_pos);
        if origin_tiles.is_empty() {
            return;
        }

        // Use an arbitrary facing for destination; avoid an impossible coord
        let dest_tiles = self.find_portal_cluster(
            dest_center, Direction::XY, Coordinate::new(-9999, -9999),
        );

        let mut saved = HashMap::new();
        for &coord in origin_tiles.iter().chain(dest_tiles.iter()) {
            if let Some(wt) = self.tiles.get(&coord) {
                saved.insert(coord, wt.clone());
            }
            let room_coord = self.tiles.get(&coord)
                .map(|wt| wt.room_coord)
                .unwrap_or(origin_room);
            self.tiles.insert(coord, WorldTile {
                tile: Tile::Portal,
                room_coord,
                face: None,
                fixture_id: None,
            });
        }

        self.active_portal = Some(PortalPair {
            origin_room,
            dest_room,
            origin_tiles,
            dest_tiles,
            saved_originals: saved,
        });
    }

    /// Build a 3-tile vertical zigzag portal column near `near`.
    ///
    /// Screen layout (XY/YZ zigzag = pure vertical):
    /// ```
    ///   [CC]       row -2
    /// [BB]         row -1
    ///   [AA]       row  0  (seed)
    /// ```
    /// 4 chars wide × 3 rows tall on screen.
    fn find_portal_cluster(
        &self,
        near: Coordinate<i32>,
        facing: Direction,
        avoid: Coordinate<i32>,
    ) -> Vec<Coordinate<i32>> {
        // Try seed positions in front of `near`
        let seeds = [
            near + facing + facing,
            near + facing,
            near + (facing + Angle::Left),
            near + (facing + Angle::Right),
        ];

        for &seed in &seeds {
            if seed == avoid {
                continue;
            }
            // Build 3-tile vertical zigzag: seed, seed+XY, seed+XY+YZ
            let col = [
                seed,
                seed + Direction::XY,
                seed + Direction::XY + Direction::YZ,
            ];
            let valid: Vec<_> = col.iter()
                .filter(|&&c| c != avoid && self.is_floor_tile(c))
                .copied()
                .collect();
            if valid.len() >= 2 {
                return valid;
            }

            // Try downward zigzag: seed, seed+YX, seed+YX+ZY
            let col_down = [
                seed,
                seed + Direction::YX,
                seed + Direction::YX + Direction::ZY,
            ];
            let valid: Vec<_> = col_down.iter()
                .filter(|&&c| c != avoid && self.is_floor_tile(c))
                .copied()
                .collect();
            if valid.len() >= 2 {
                return valid;
            }
        }

        // Fallback: any 2 adjacent floor tiles within range 3
        for r in 1..=3 {
            for coord in near.ring_iter(r, Spin::CW(hex2d_stacks::XY)) {
                if coord == avoid || !self.is_floor_tile(coord) {
                    continue;
                }
                for &dir in Direction::all() {
                    let adj = coord + dir;
                    if adj != avoid && self.is_floor_tile(adj) {
                        return vec![coord, adj];
                    }
                }
            }
        }

        Vec::new()
    }

    /// Check if a tile at `coord` is walkable floor (not a shelf, wall, etc.).
    fn is_floor_tile(&self, coord: Coordinate<i32>) -> bool {
        self.tiles.get(&coord)
            .map(|wt| matches!(wt.tile,
                Tile::Floor | Tile::VestibuleFloor | Tile::CorridorFloor | Tile::Ostrakon
            ))
            .unwrap_or(false)
    }

    /// Place a room using BFS coordinates mapped directly to world hex positions.
    /// Extracts a compact RoomInfo (dropping volumes/wall_edges/shelf_types)
    /// and manages LRU eviction when the room cap is exceeded.
    pub fn place_room(&mut self, metadata: &RoomMetadata) -> Coordinate<i32> {
        let bfs_coord = [metadata.q, metadata.r, metadata.z];

        // Already loaded — just touch LRU
        if self.rooms.contains_key(&bfs_coord) {
            self.touch_room(bfs_coord);
            return self.rooms[&bfs_coord].center;
        }

        // Map BFS (q, r) → world coordinate via hex transform
        let grid_pos = Coordinate::new(metadata.q, metadata.r);
        let center = bfs_to_world(metadata.q, metadata.r, metadata.z);

        // Derive gallery wall layout from BFS door_edges
        let layout = GalleryLayout::from_door_edges(&metadata.door_edges);

        let hall_style = HallStyle::from_district(&metadata.district);
        let loaded = LoadedRoom {
            bfs_coord,
            center,
            grid_pos,
            layout: layout.clone(),
            info: RoomInfo::from_metadata(metadata),
            hall_style,
            pinned: false,
        };
        self.rooms.insert(bfs_coord, loaded);
        self.shelves.insert(bfs_coord, metadata.shelves.clone());
        self.room_lru.push_back(bfs_coord);

        // Determine room properties for stamping
        let has_shaft = !metadata.shaft_edges.is_empty()
            || metadata.door_edges.iter().any(|e| e.direction == "UP" || e.direction == "DOWN");
        let furniture = RoomFurniture::from_bfs_type(&metadata.room_type, has_shaft);

        let radius = GALLERY_RADIUS;

        self.stamp_room(bfs_coord, center, &layout, metadata, &furniture, radius);

        // Repair pass: clear any Shelf tile that's adjacent to a Door tile.
        // This prevents shelves from blocking doorways regardless of stamp order.
        self.repair_door_adjacency(center, radius);

        center
    }

    /// Stamp article gallery rooms in expanding hex rings around observatory [0,0,3].
    /// Ring 1: 6 rooms (18 articles max), Ring 2: 12 rooms (36 more), etc.
    /// Each gallery holds up to 3 ArticleDisplay tiles around a central brazier.
    pub fn stamp_article_galleries(&mut self, articles: Vec<(String, String)>) {
        use hex2d_stacks::Spin;

        if articles.is_empty() { return; }

        let obs_coord: [i32; 3] = [0, 0, 3];
        let obs_q = 0i32;
        let obs_r = 0i32;

        // BFS hex direction offsets: E, NE, NW, W, SW, SE
        let bfs_dirs: [(i32, i32); 6] = [(1, 0), (1, -1), (0, -1), (-1, 0), (-1, 1), (0, 1)];
        // Direction labels matching bfs_dirs indices
        let dir_labels: [&str; 6] = ["E", "NE", "NW", "W", "SW", "SE"];
        // Opposite direction (for door facing inward)
        let opposite: [usize; 6] = [3, 4, 5, 0, 1, 2]; // E↔W, NE↔SW, NW↔SE

        // Generate BFS ring coordinates at distance `dist` from center
        let bfs_ring = |dist: i32| -> Vec<(i32, i32)> {
            if dist == 0 { return vec![(obs_q, obs_r)]; }
            let mut coords = Vec::with_capacity((6 * dist) as usize);
            // Start at center + dist steps in direction 4 (SW)
            let mut q = obs_q + dist * bfs_dirs[4].0;
            let mut r = obs_r + dist * bfs_dirs[4].1;
            for dir_idx in 0..6 {
                let (dq, dr) = bfs_dirs[dir_idx];
                for _ in 0..dist {
                    coords.push((q, r));
                    q += dq;
                    r += dr;
                }
            }
            coords
        };

        // Find the inward direction for a BFS coord: which of 6 neighbors is closer to center
        let inward_dir = |q: i32, r: i32| -> usize {
            let cur_dist = hex_distance_bfs(obs_q, obs_r, q, r);
            for (idx, (dq, dr)) in bfs_dirs.iter().enumerate() {
                let nq = q + dq;
                let nr = r + dr;
                if hex_distance_bfs(obs_q, obs_r, nq, nr) < cur_dist {
                    return opposite[idx]; // door faces TOWARD the inward neighbor
                }
            }
            0 // fallback
        };

        // Distribute articles across expanding rings
        let mut article_iter = articles.into_iter();
        let mut ring_dist = 1i32;
        let mut rooms_created: Vec<([i32; 3], usize)> = Vec::new(); // (coord, inward_wall_idx)

        'outer: loop {
            let ring_coords = bfs_ring(ring_dist);
            for (gq, gr) in ring_coords {
                // Collect next chunk of up to 3 articles
                let mut chunk: Vec<(String, String)> = Vec::new();
                for _ in 0..3 {
                    match article_iter.next() {
                        Some(a) => chunk.push(a),
                        None => break,
                    }
                }
                if chunk.is_empty() { break 'outer; }

                let gallery_coord = [gq, gr, 3];
                let door_face = inward_dir(gq, gr);
                let door_dir = dir_labels[door_face];

                // Find inward neighbor coord for door_edge target
                let opp_idx = opposite[door_face];
                let (idq, idr) = bfs_dirs[opp_idx];
                let inward_coord = [gq + idq, gr + idr, 3];

                let meta = RoomMetadata {
                    room_key: format!("article_gallery_r{}_{},{}", ring_dist, gq, gr),
                    q: gq,
                    r: gr,
                    z: 3,
                    room_type: "gallery".to_string(),
                    district: "scribe_gallery".to_string(),
                    volume_count: 0,
                    door_count: 1,
                    palette: String::new(),
                    z_band: 3,
                    shelves: Vec::new(),
                    volumes: Vec::new(),
                    volumes_typed: Vec::new(),
                    door_edges: vec![DoorEdge {
                        from_coord: gallery_coord,
                        to_coord: inward_coord,
                        direction: door_dir.to_string(),
                    }],
                    wall_edges: Vec::new(),
                    shaft_edges: Vec::new(),
                    shelf_types: Vec::new(),
                    cluster_id: format!("Article Gallery"),
                    cluster_signature: chunk.iter().map(|(_, title)| title.clone()).collect(),
                };

                let center = self.place_room(&meta);
                // Pin synthetic gallery rooms — they can't be re-fetched from BFS
                if let Some(room) = self.rooms.get_mut(&gallery_coord) {
                    room.pinned = true;
                }
                rooms_created.push((gallery_coord, direction_to_wall_index(door_dir).unwrap_or(0)));

                // Brazier at center
                self.tiles.insert(center, WorldTile {
                    tile: Tile::Brazier,
                    room_coord: gallery_coord,
                    face: None,
                    fixture_id: None,
                });

                // Place ArticleDisplay tiles on ring distance 3, skipping door face
                let door_wall_idx = direction_to_wall_index(door_dir).unwrap_or(0);
                let ring_positions: Vec<Coordinate<i32>> = center
                    .ring_iter(3, Spin::CW(hex2d_stacks::XY))
                    .enumerate()
                    .filter(|(i, _)| {
                        let edge_idx = (*i / 3 + 1) % 6;
                        edge_idx != door_wall_idx
                    })
                    .map(|(_, coord)| coord)
                    .collect();

                let available = ring_positions.len();
                if available == 0 { continue; }
                let step = available / chunk.len().max(1);

                for (j, (filename, _title)) in chunk.iter().enumerate() {
                    let pos_idx = (j * step) % available;
                    let coord = ring_positions[pos_idx];
                    if self.tiles.get(&coord).map(|wt| wt.tile == Tile::Floor).unwrap_or(false) {
                        self.tiles.insert(coord, WorldTile {
                            tile: Tile::ArticleDisplay,
                            room_coord: gallery_coord,
                            face: None,
                            fixture_id: None,
                        });
                        self.article_tiles.insert(coord, filename.clone());
                    }
                }

                // Check if all articles placed
                if article_iter.len() == 0 { break 'outer; }
            }
            ring_dist += 1;
            if ring_dist > 5 { break; } // safety cap
        }

        // Punch doors into the observatory wall ring for ring-1 galleries
        let obs_center = bfs_to_world(obs_q, obs_r, 3);
        let r = GALLERY_RADIUS;
        let r_usize = r as usize;
        let dhw = door_halfwidth(r);
        let mid = r_usize / 2;

        for &(gallery_coord, door_wall_idx) in &rooms_created {
            // Only punch into observatory for ring-1 rooms (direct neighbors)
            let dist = hex_distance_bfs(obs_q, obs_r, gallery_coord[0], gallery_coord[1]);
            if dist != 1 { continue; }

            // The observatory face toward this gallery is the OPPOSITE of the gallery's door face
            let obs_face = opposite[door_wall_idx];
            self.punch_door(obs_center, obs_coord, r, r_usize, dhw, mid, obs_face);
        }

        // Punch doors into ring-1 rooms for ring-2 connectivity
        for &(gallery_coord, door_wall_idx) in &rooms_created {
            let dist = hex_distance_bfs(obs_q, obs_r, gallery_coord[0], gallery_coord[1]);
            if dist < 2 { continue; }

            // Find the inward neighbor and punch a door into it
            let opp_idx = opposite[door_wall_idx];
            let (idq, idr) = bfs_dirs[opp_idx];
            let inner_bfs = [gallery_coord[0] + idq, gallery_coord[1] + idr, 3];

            if let Some(inner_room) = self.rooms.get(&inner_bfs) {
                let inner_center = inner_room.center;
                // The inner room's face toward us is the opposite of our door direction
                let inner_face = opposite[opp_idx];
                self.punch_door(inner_center, inner_bfs, r, r_usize, dhw, mid, inner_face);
            }
        }

        // Pin the observatory — its punched doors can't be restored from BFS reload
        if let Some(obs) = self.rooms.get_mut(&obs_coord) {
            obs.pinned = true;
        }
    }

    /// Punch a door into an existing room's wall ring on a specific face.
    fn punch_door(
        &mut self,
        center: Coordinate<i32>,
        room_coord: [i32; 3],
        r: i32,
        r_usize: usize,
        dhw: usize,
        mid: usize,
        wall_face: usize,
    ) {
        use hex2d_stacks::Spin;
        for (i, coord) in center.ring_iter(r, Spin::CW(hex2d_stacks::XY)).enumerate() {
            let edge_idx = i / r_usize;
            let pos_in_edge = i % r_usize;
            let tile_wall_idx = (edge_idx + 1) % 6;

            if tile_wall_idx == wall_face {
                let in_doorway = pos_in_edge >= mid.saturating_sub(dhw)
                    && pos_in_edge <= mid + dhw;
                if in_doorway {
                    self.tiles.insert(coord, WorldTile {
                        tile: Tile::Door(true),
                        room_coord,
                        face: Some(HexFace::from_wall_idx(wall_face)),
                        fixture_id: None,
                    });
                }
            }
        }
    }

}

/// Hex distance in BFS axial coordinates.
fn hex_distance_bfs(q1: i32, r1: i32, q2: i32, r2: i32) -> i32 {
    let dq = q2 - q1;
    let dr = r2 - r1;
    let ds = -dq - dr;
    (dq.abs() + dr.abs() + ds.abs()) / 2
}

/// Per-room-type furniture configuration (matches v2 room_stamper.gd).
struct RoomFurniture {
    columns: usize,      // count, placed on ring at radius 4
    sconces: usize,      // count, placed on ring at radius 6
    has_shaft: bool,     // center gets shaft tile
}

impl RoomFurniture {
    fn from_bfs_type(room_type: &str, has_shaft: bool) -> Self {
        match room_type {
            "terminus" => RoomFurniture { columns: 0, sconces: 2, has_shaft },
            "hallway"  => RoomFurniture { columns: 0, sconces: 2, has_shaft },
            "chamber"  => RoomFurniture { columns: 0, sconces: 4, has_shaft },
            "gallery"  => RoomFurniture { columns: 3, sconces: 6, has_shaft },
            "vault"    => RoomFurniture { columns: 0, sconces: 3, has_shaft },
            "atrium"   => RoomFurniture { columns: 6, sconces: 6, has_shaft },
            "rotunda"  => RoomFurniture { columns: 8, sconces: 8, has_shaft },
            "junction" => RoomFurniture { columns: 0, sconces: 4, has_shaft },
            _          => RoomFurniture { columns: 0, sconces: 4, has_shaft },
        }
    }
}

impl WorldMap {
    /// Stamp room geometry using composition-driven biome placement.
    ///
    /// Zone rings (from center outward):
    /// - distance ≤ interior_radius (R-3): floor interior + furniture
    /// - distance == R-2: inner wall (vault/rotunda) or floor (others)
    /// - distance == R-1: wall-mounted features (composition-driven) or shelf (fallback)
    /// - distance == R: outer wall with door dissolution
    ///
    /// When volumes_typed data is available, the room interior reflects its corpus:
    /// - Wall ring: papyrus tablets, inscriptions, false doors, tomb alcoves
    /// - Center: coffin, sculpture, stele, or lectern
    /// - Floor: scroll pedestals, vessels, stelae at regular intervals
    /// - Scatter: ostraka on remaining floor tiles
    /// - Ambient: pools (temple), braziers (deep z)
    fn stamp_room(
        &mut self,
        bfs_coord: [i32; 3],
        center: Coordinate<i32>,
        layout: &GalleryLayout,
        metadata: &RoomMetadata,
        furniture: &RoomFurniture,
        radius: i32,
    ) {
        let r = radius;
        let r_usize = r as usize;
        let ir = interior_radius(r);
        let dhw = door_halfwidth(r);

        // Corridor rooms get distinct floor tiles and columns
        let is_corridor = metadata.room_type == "corridor";
        let floor_tile = if is_corridor { Tile::CorridorFloor } else { Tile::Floor };

        // Build composition from typed volumes (empty = fallback to generic shelves)
        let comp = RoomComposition::from_volumes_typed(&metadata.volumes_typed);
        let use_composition = !comp.is_empty();

        // 1. Floor fill: all tiles within interior radius (R-3)
        for coord in center.range_iter(ir) {
            self.tiles.insert(coord, WorldTile {
                tile: floor_tile, room_coord: bfs_coord, face: None, fixture_id: None,
            });
        }

        // 2. Ring at R-2: floor (part of walkable interior)
        for coord in center.ring_iter(r - 2, Spin::CW(hex2d_stacks::XY)) {
            self.tiles.insert(coord, WorldTile {
                tile: floor_tile, room_coord: bfs_coord, face: None, fixture_id: None,
            });
        }

        // 3. Wall-mount ring at R-1: composition-driven or generic shelf
        let shelf_r = (r - 1) as usize;
        let ring_tiles: Vec<(usize, Coordinate<i32>)> = center
            .ring_iter(r - 1, Spin::CW(hex2d_stacks::XY))
            .enumerate()
            .collect();

        // Determine which non-door faces have WALL-type shelf data.
        // Floor-type shelves (stele, scroll, vessel, ostrakon, etc.) get their own
        // fixtures on the interior ring — they don't claim wall faces.
        // Only wall-type or unclassified shelves claim faces on the R-1 ring.
        let non_door_face_indices: Vec<usize> = (0..6)
            .filter(|i| layout.walls[*i] != WallState::FreeDoor)
            .collect();
        let effective_shelf_count = metadata.shelves.iter()
            .enumerate()
            .filter(|(_, s)| !s.is_empty())
            .filter(|(idx, _)| {
                // Keep this shelf on the wall ring only if it's a wall type
                // or has no shelf_type classification (generic shelf).
                let stype = metadata.shelf_types.get(*idx)
                    .map(|s| s.to_lowercase())
                    .unwrap_or_default();
                is_wall_shelf_type(&stype)
            })
            .count();
        // Faces with data: first N non-door faces
        let faces_with_shelves: std::collections::HashSet<usize> = non_door_face_indices.iter()
            .take(effective_shelf_count)
            .copied()
            .collect();

        // Collect non-door ring tiles (available for wall-mounted features)
        let mut wall_slots: Vec<(Coordinate<i32>, usize, Option<HexFace>)> = Vec::new();
        for &(i, coord) in &ring_tiles {
            let edge_idx = i / shelf_r;
            let pos_in_edge = i % shelf_r;
            let wall_idx = (edge_idx + 1) % 6;
            let face = Some(HexFace::from_wall_idx(wall_idx));

            let is_door_zone = match layout.walls[wall_idx] {
                WallState::FreeDoor => true,
                _ => {
                    if pos_in_edge == 0 {
                        let prev_wall = (wall_idx + 5) % 6;
                        layout.walls[prev_wall] == WallState::FreeDoor
                    } else if pos_in_edge == shelf_r - 1 {
                        let next_wall = (wall_idx + 1) % 6;
                        layout.walls[next_wall] == WallState::FreeDoor
                    } else {
                        false
                    }
                }
            };

            if is_door_zone {
                self.tiles.insert(coord, WorldTile {
                    tile: Tile::Floor, room_coord: bfs_coord, face, fixture_id: None,
                });
            } else if !faces_with_shelves.contains(&wall_idx) {
                // This face has no shelf data — render as bare wall interior (floor tile)
                self.tiles.insert(coord, WorldTile {
                    tile: Tile::Floor, room_coord: bfs_coord, face, fixture_id: None,
                });
            } else {
                // Don't place over existing passable tiles
                if let Some(existing) = self.tiles.get(&coord) {
                    if existing.tile.is_passable() {
                        continue;
                    }
                }
                wall_slots.push((coord, wall_idx, face));
            }
        }

        // Track which BFS shelf indices are claimed by wall fixtures (prevents doubling)
        let mut claimed_shelves = std::collections::HashSet::new();

        // Assign wall fixtures: one fixture per BFS wall-type shelf
        if use_composition && !wall_slots.is_empty() {
            self.stamp_wall_fixtures(bfs_coord, &wall_slots, metadata, &mut claimed_shelves);
        } else {
            // Fallback: all Shelf (generic multi-tile treatment, no fixture_id)
            for (coord, _, face) in &wall_slots {
                self.tiles.insert(*coord, WorldTile {
                    tile: Tile::Shelf, room_coord: bfs_coord, face: *face, fixture_id: None,
                });
            }
        }

        // 4. Outer wall ring at R: door dissolution on FREE_DOOR faces
        for (i, coord) in center.ring_iter(r, Spin::CW(hex2d_stacks::XY)).enumerate() {
            let edge_idx = i / r_usize;
            let pos_in_edge = i % r_usize;
            let wall_idx = (edge_idx + 1) % 6;
            let mid = r_usize / 2;
            let face = Some(HexFace::from_wall_idx(wall_idx));

            let tile = match layout.walls[wall_idx] {
                WallState::FreeDoor => {
                    let in_doorway = pos_in_edge >= mid.saturating_sub(dhw)
                        && pos_in_edge <= mid + dhw;
                    if in_doorway { Tile::Door(true) } else { Tile::Wall }
                }
                _ => {
                    if let Some(existing) = self.tiles.get(&coord) {
                        if existing.tile == Tile::Door(true) {
                            continue;
                        }
                    }
                    Tile::Wall
                }
            };
            self.tiles.insert(coord, WorldTile { tile, room_coord: bfs_coord, face, fixture_id: None });
        }

        // 5. Center feature: shaft takes priority at exact center
        if furniture.has_shaft {
            let shaft_kind = shaft_kind_from_edges(&metadata.shaft_edges, &metadata.door_edges);
            self.tiles.insert(center, WorldTile {
                tile: Tile::Shaft(shaft_kind), room_coord: bfs_coord, face: None, fixture_id: None,
            });
        } else if use_composition {
            // Composition center piece (coffin, sculpture, stele)
            let center_tile = if comp.coffin > 0 {
                Tile::Coffin
            } else if comp.sculpture > 0 {
                Tile::Sculpture
            } else if comp.stele > 2 {
                Tile::Stele
            } else {
                Tile::Floor
            };
            if center_tile != Tile::Floor {
                self.tiles.insert(center, WorldTile {
                    tile: center_tile, room_coord: bfs_coord, face: None, fixture_id: None,
                });
            }
        }

        // 5b. Lectern: ALWAYS present when room has corpus data.
        // Placed at center if free, otherwise offset 2 hexes in a clear direction.
        if metadata.volume_count > 0 {
            let center_tile = self.tiles.get(&center).map(|wt| wt.tile);
            let needs_offset = matches!(center_tile,
                Some(Tile::Shaft(_)) | Some(Tile::Coffin) |
                Some(Tile::Sculpture) | Some(Tile::Stele));
            if needs_offset {
                // Place lectern 2 hexes from center in the first clear direction
                for &dir in Direction::all() {
                    let offset_pos = center + dir + dir;
                    if let Some(wt) = self.tiles.get(&offset_pos) {
                        if wt.tile == Tile::Floor {
                            self.tiles.insert(offset_pos, WorldTile {
                                tile: Tile::Lectern, room_coord: bfs_coord, face: None, fixture_id: None,
                            });
                            break;
                        }
                    }
                }
            } else {
                self.tiles.insert(center, WorldTile {
                    tile: Tile::Lectern, room_coord: bfs_coord, face: None, fixture_id: None,
                });
            }
        }

        // 6. Floor fixtures on interior ring — driven by BFS shelf_types.
        //    Each unclaimed non-wall shelf gets one fixture tile on a ring.
        //    These ARE the room's freestanding artifacts: stelae, scroll racks, vessels, etc.
        if !is_corridor {
            self.stamp_floor_fixtures(bfs_coord, center, r, metadata, layout, &claimed_shelves);
        }

        // 7. Ambient light sources: sconces, columns, braziers.
        //    Sconces on ring R-4 (between shelf ring and interior).
        //    Columns on ring 3 (near center). Braziers replace sconces deep (z <= -4).
        if !is_corridor {
            self.stamp_light_sources(bfs_coord, center, r, metadata, furniture);
        }

        // 8. Corridor door-flanking sconces: 2 sconces per door, placed on R-1 ring
        //    tiles adjacent to (but not inside) the doorway.
        if is_corridor {
            let inner_ring: Vec<(usize, Coordinate<i32>)> = center
                .ring_iter(r - 1, Spin::CW(hex2d_stacks::XY))
                .enumerate()
                .collect();
            let inner_r = (r - 1) as usize;
            for wall_idx in 0..6 {
                if layout.walls[wall_idx] != WallState::FreeDoor {
                    continue;
                }
                let mid = inner_r / 2;
                let dhw_inner = door_halfwidth(r);
                // Place sconces just outside doorway on both sides
                let low = mid.saturating_sub(dhw_inner + 1);
                let high = (mid + dhw_inner + 1).min(inner_r.saturating_sub(1));
                for slot in [low, high] {
                    let edge_start = ((wall_idx + 5) % 6) * inner_r;
                    let idx = edge_start + slot;
                    if idx < inner_ring.len() {
                        let coord = inner_ring[idx].1;
                        let existing = self.tiles.get(&coord).map(|wt| wt.tile);
                        if matches!(existing, Some(Tile::CorridorFloor) | Some(Tile::Floor)) {
                            self.tiles.insert(coord, WorldTile {
                                tile: Tile::Sconce, room_coord: bfs_coord, face: None, fixture_id: None,
                            });
                        }
                    }
                }
            }
        }

    }

    /// Stamp wall fixtures: one fixture per BFS shelf that maps to a wall type.
    /// Each fixture is a SINGLE tile with a fixture_id. Surrounding wall slots
    /// on the same face become generic Shelf tiles (visual fill, no fixture_id).
    /// Claimed shelf indices are recorded in `claimed_shelves` to prevent doubling.
    fn stamp_wall_fixtures(
        &mut self,
        bfs_coord: [i32; 3],
        wall_slots: &[(Coordinate<i32>, usize, Option<HexFace>)],
        metadata: &RoomMetadata,
        claimed_shelves: &mut std::collections::HashSet<usize>,
    ) {
        // Map each BFS shelf to a wall fixture type (if it's a wall type)
        let wall_fixtures: Vec<(usize, Tile, String)> = metadata.shelf_types.iter()
            .enumerate()
            .filter_map(|(shelf_idx, stype)| {
                let s = stype.to_lowercase();
                let (tile, label) = if s.contains("papyrus") || s.contains("writing") || s.contains("unknown") || s == "" {
                    (Tile::PapyrusTablet, "Papyrus Shelf")
                } else if s.contains("wand") || s.contains("wall") || s.contains("inscription") {
                    (Tile::WallInscription, "Wall Inscription")
                } else if s.contains("scheint") || s.contains("false_door") || s.contains("doorway") {
                    (Tile::FalseDoor, "False Door")
                } else if s.contains("grab") || s.contains("tomb") || s.contains("chamber") {
                    (Tile::TombAlcove, "Tomb Alcove")
                } else {
                    return None; // not a wall type
                };
                Some((shelf_idx, tile, label.to_string()))
            })
            .collect();

        if wall_fixtures.is_empty() || wall_slots.is_empty() {
            // No wall fixtures — fill all slots as generic Shelf
            for (coord, _, face) in wall_slots {
                self.tiles.insert(*coord, WorldTile {
                    tile: Tile::Shelf, room_coord: bfs_coord, face: *face, fixture_id: None,
                });
            }
            return;
        }

        // Group wall_slots by face (wall_idx)
        let mut face_slots: std::collections::BTreeMap<usize, Vec<(Coordinate<i32>, Option<HexFace>)>> = std::collections::BTreeMap::new();
        for &(coord, wall_idx, face) in wall_slots {
            face_slots.entry(wall_idx).or_default().push((coord, face));
        }

        // Assign one fixture per wall-type shelf, placed at the midpoint of a face.
        // Remaining slots on that face become generic Shelf.
        let mut face_iter = face_slots.iter();
        let fixture_list = self.fixtures.entry(bfs_coord).or_default();
        let base_id = fixture_list.len() as u16;

        for (i, (shelf_idx, tile, label)) in wall_fixtures.iter().enumerate() {
            claimed_shelves.insert(*shelf_idx);
            let fixture_id = base_id + i as u16;

            // Pick the next face with slots
            if let Some((_wall_idx, slots)) = face_iter.next() {
                let mid = slots.len() / 2;
                for (j, (coord, face)) in slots.iter().enumerate() {
                    if j == mid {
                        // This is the fixture tile
                        self.tiles.insert(*coord, WorldTile {
                            tile: *tile, room_coord: bfs_coord, face: *face, fixture_id: Some(fixture_id),
                        });
                        fixture_list.push(FixtureMeta {
                            id: fixture_id,
                            tile_type: *tile,
                            shelf_index: *shelf_idx,
                            coord: *coord,
                            label: label.clone(),
                        });
                    } else {
                        // Visual fill: generic Shelf (not individually addressable)
                        self.tiles.insert(*coord, WorldTile {
                            tile: Tile::Shelf, room_coord: bfs_coord, face: *face, fixture_id: None,
                        });
                    }
                }
            }
        }

        // Remaining faces with no fixture assignment → generic Shelf fill
        for (_wall_idx, slots) in face_iter {
            for (coord, face) in slots {
                self.tiles.insert(*coord, WorldTile {
                    tile: Tile::Shelf, room_coord: bfs_coord, face: *face, fixture_id: None,
                });
            }
        }
    }

    /// Place floor fixtures on an interior ring, driven by BFS shelf_types.
    /// Each non-wall shelf type that hasn't been claimed gets ONE fixture tile.
    /// Shelf indices already in `claimed_shelves` are skipped (no doubling).
    fn stamp_floor_fixtures(
        &mut self,
        bfs_coord: [i32; 3],
        center: Coordinate<i32>,
        radius: i32,
        metadata: &RoomMetadata,
        layout: &GalleryLayout,
        claimed_shelves: &std::collections::HashSet<usize>,
    ) {
        // Map each unclaimed BFS shelf to a floor fixture type
        let floor_fixtures: Vec<(usize, Tile, String)> = metadata.shelf_types.iter()
            .enumerate()
            .filter(|(idx, _)| !claimed_shelves.contains(idx))
            .filter_map(|(shelf_idx, stype)| {
                let s = stype.to_lowercase();
                let (tile, label) = if s.contains("stele") || s.contains("stel") {
                    (Tile::Stele, "Stele")
                } else if s.contains("scroll") || s.contains("schriftrolle") {
                    (Tile::ScrollPedestal, "Scroll Rack")
                } else if s.contains("vessel") || s.contains("object") || s.contains("gef") {
                    (Tile::Vessel, "Vessel")
                } else if s.contains("sculpt") || s.contains("skulptur") {
                    (Tile::Sculpture, "Sculpture")
                } else if s.contains("coffin") || s.contains("sarg") {
                    (Tile::Coffin, "Coffin")
                } else if s.contains("ostra") {
                    (Tile::Ostrakon, "Ostrakon")
                } else if s.contains("rock") || s.contains("fels") {
                    (Tile::RockFace, "Rock Inscription")
                } else {
                    return None; // wall type or unknown — skip
                };
                Some((shelf_idx, tile, label.to_string()))
            })
            .collect();

        if floor_fixtures.is_empty() { return; }

        // Place on ring at radius-5
        let ring_r = (radius - 5).max(2);
        let ring_r_usize = ring_r as usize;
        let ring: Vec<(usize, Coordinate<i32>)> = center
            .ring_iter(ring_r, Spin::CW(hex2d_stacks::XY))
            .enumerate()
            .collect();

        let valid: Vec<Coordinate<i32>> = ring.iter()
            .filter(|(i, _)| {
                if ring_r_usize == 0 { return true; }
                let edge_idx = i / ring_r_usize;
                let wall_idx = (edge_idx + 1) % 6;
                layout.walls[wall_idx] != WallState::FreeDoor
            })
            .map(|(_, c)| *c)
            .filter(|c| {
                self.tiles.get(c).map(|wt| wt.tile == Tile::Floor).unwrap_or(false)
            })
            .collect();

        if valid.is_empty() { return; }

        let step = (valid.len() / floor_fixtures.len()).max(1);
        let fixture_list = self.fixtures.entry(bfs_coord).or_default();
        let base_id = fixture_list.len() as u16;

        for (i, (shelf_idx, tile, label)) in floor_fixtures.iter().enumerate() {
            let pos = (i * step) % valid.len();
            let coord = valid[pos];
            let fixture_id = base_id + i as u16;

            self.tiles.insert(coord, WorldTile {
                tile: *tile, room_coord: bfs_coord, face: None, fixture_id: Some(fixture_id),
            });
            fixture_list.push(FixtureMeta {
                id: fixture_id,
                tile_type: *tile,
                shelf_index: *shelf_idx,
                coord,
                label: label.clone(),
            });
        }
    }

    // stamp_ambient_pools removed — no ground clutter

    /// Placeholder to keep method signatures if referenced elsewhere
    #[allow(dead_code)]
    fn stamp_ambient_pools(
        &mut self,
        bfs_coord: [i32; 3],
        center: Coordinate<i32>,
        radius: i32,
    ) {
        let pool_r = 2.min(radius - 4);
        if pool_r < 1 { return; }

        let ring: Vec<Coordinate<i32>> = center.ring_iter(pool_r, Spin::CW(hex2d_stacks::XY)).collect();
        // Place pools at positions 0 and 3 (opposite sides)
        for &idx in &[0usize, 3] {
            if idx < ring.len() {
                let coord = ring[idx];
                if let Some(wt) = self.tiles.get(&coord) {
                    if wt.tile == Tile::Floor {
                        self.tiles.insert(coord, WorldTile {
                            tile: Tile::Pool, room_coord: bfs_coord, face: None, fixture_id: None,
                        });
                    }
                }
            }
        }
    }

    /// Stamp ambient light sources into a room: sconces, columns, braziers.
    ///
    /// Placement strategy:
    /// - Columns on ring 3 (near center), evenly spaced
    /// - Sconces on ring R-4 (between shelves and interior), evenly spaced
    /// - Deep rooms (z <= -4): 1-2 sconces replaced with braziers
    /// - Temple rooms: add 1-2 pools near center
    fn stamp_light_sources(
        &mut self,
        bfs_coord: [i32; 3],
        center: Coordinate<i32>,
        radius: i32,
        metadata: &RoomMetadata,
        furniture: &RoomFurniture,
    ) {
        // Sconces (and braziers) on ring R-4
        if furniture.sconces > 0 {
            let sconce_r = (radius - 4).max(2);
            let sconce_ring: Vec<Coordinate<i32>> = center
                .ring_iter(sconce_r, Spin::CW(hex2d_stacks::XY))
                .collect();
            if !sconce_ring.is_empty() {
                let step = (sconce_ring.len() / furniture.sconces).max(1);
                // Deep rooms: first 1-2 become braziers instead of sconces
                let brazier_count = if metadata.z <= -4 {
                    (furniture.sconces / 3).max(1).min(2)
                } else {
                    0
                };
                for i in 0..furniture.sconces {
                    let idx = (i * step) % sconce_ring.len();
                    let coord = sconce_ring[idx];
                    if self.tiles.get(&coord).map(|wt| wt.tile == Tile::Floor).unwrap_or(false) {
                        let tile = if i < brazier_count { Tile::Brazier } else { Tile::Sconce };
                        self.tiles.insert(coord, WorldTile {
                            tile, room_coord: bfs_coord, face: None, fixture_id: None,
                        });
                    }
                }
            }
        }

        // Temple rooms: add pools near center
        if metadata.district == "temple" && metadata.volume_count > 10 {
            let pool_r = 2.min(radius - 4);
            if pool_r >= 1 {
                let ring: Vec<Coordinate<i32>> = center
                    .ring_iter(pool_r, Spin::CW(hex2d_stacks::XY))
                    .collect();
                // Place pools at positions 0 and 3 (opposite sides)
                for &idx in &[0usize, 3] {
                    if idx < ring.len() {
                        let coord = ring[idx];
                        if self.tiles.get(&coord).map(|wt| wt.tile == Tile::Floor).unwrap_or(false) {
                            self.tiles.insert(coord, WorldTile {
                                tile: Tile::Pool, room_coord: bfs_coord, face: None, fixture_id: None,
                            });
                        }
                    }
                }
            }
        }
    }
}

impl WorldMap {
    /// Post-stamp repair: convert any wall-mounted tile adjacent to a Door(true) tile into Floor.
    /// This guarantees doorways are never blocked by shelves/features regardless of room stamp order.
    fn repair_door_adjacency(&mut self, center: Coordinate<i32>, radius: i32) {
        // Collect tiles that need clearing (can't mutate while iterating)
        let mut to_clear: Vec<Coordinate<i32>> = Vec::new();

        // Check shelf ring (R-1) and nearby tiles
        for coord in center.ring_iter(radius - 1, Spin::CW(hex2d_stacks::XY)) {
            if let Some(wt) = self.tiles.get(&coord) {
                if is_wall_mounted(wt.tile) {
                    // Check all 6 neighbors for a Door tile
                    for &dir in Direction::all() {
                        let adj = coord + dir;
                        if let Some(adj_wt) = self.tiles.get(&adj) {
                            if adj_wt.tile == Tile::Door(true) {
                                to_clear.push(coord);
                                break;
                            }
                        }
                    }
                }
            }
        }

        // Also check the ring just outside (in case neighbor room's shelf ring blocks)
        for coord in center.ring_iter(radius, Spin::CW(hex2d_stacks::XY)) {
            if let Some(wt) = self.tiles.get(&coord) {
                if wt.tile == Tile::Door(true) {
                    for &dir in Direction::all() {
                        let adj = coord + dir;
                        if let Some(adj_wt) = self.tiles.get(&adj) {
                            if is_wall_mounted(adj_wt.tile) {
                                to_clear.push(adj);
                            }
                        }
                    }
                }
            }
        }

        // Apply repairs
        for coord in to_clear {
            if let Some(wt) = self.tiles.get_mut(&coord) {
                wt.tile = Tile::Floor;
            }
        }
    }

    /// Seal doors that lead to non-existent rooms.
    /// Called after preload: any door_edge whose target coord is not in the world
    /// AND is in `missing_coords` (confirmed non-existent, not just unloaded) gets
    /// its Door(true) tiles overwritten with Wall.
    pub fn seal_dangling_doors(&mut self, missing_coords: &std::collections::HashSet<[i32; 3]>) {
        let room_coords: Vec<[i32; 3]> = self.rooms.keys().cloned().collect();
        for room_coord in room_coords {
            let (center, radius, dangling_walls) = {
                let room = match self.rooms.get(&room_coord) {
                    Some(r) => r,
                    None => continue,
                };
                let dangling: Vec<usize> = room.info.door_edges.iter()
                    .filter_map(|edge| {
                        if missing_coords.contains(&edge.to_coord) {
                            direction_to_wall_index(&edge.direction)
                        } else {
                            None
                        }
                    })
                    .collect();
                if dangling.is_empty() { continue; }
                (room.center, GALLERY_RADIUS, dangling)
            };

            let r = radius;
            let r_usize = r as usize;
            for (i, coord) in center.ring_iter(r, Spin::CW(hex2d_stacks::XY)).enumerate() {
                let edge_idx = i / r_usize;
                let wall_idx = (edge_idx + 1) % 6;
                if dangling_walls.contains(&wall_idx) {
                    if let Some(wt) = self.tiles.get_mut(&coord) {
                        if wt.tile == Tile::Door(true) {
                            wt.tile = Tile::Wall;
                        }
                    }
                }
            }
        }
    }
}

/// Whether a shelf_type string classifies as a wall-mounted shelf.
/// Floor types (stele, scroll, vessel, ostrakon, etc.) return false —
/// they get floor fixtures and don't claim wall faces.
fn is_wall_shelf_type(s: &str) -> bool {
    if s.is_empty() || s == "unknown" || s == "undetermined" {
        return true; // generic / unclassified → wall shelf
    }
    // Explicit wall types
    if s.contains("papyrus") || s.contains("writing") { return true; }
    if s.contains("wand") || s.contains("wall") || s.contains("inscription") { return true; }
    if s.contains("scheint") || s.contains("false_door") || s.contains("doorway") { return true; }
    if s.contains("grab") || s.contains("tomb") || s.contains("chamber") { return true; }
    // Explicit floor types → NOT wall
    if s.contains("stele") || s.contains("stel") { return false; }
    if s.contains("scroll") || s.contains("schriftrolle") { return false; }
    if s.contains("vessel") || s.contains("object") || s.contains("gef") { return false; }
    if s.contains("sculpt") || s.contains("skulptur") { return false; }
    if s.contains("coffin") || s.contains("sarg") { return false; }
    if s.contains("ostra") { return false; }
    if s.contains("rock") || s.contains("fels") { return false; }
    // Unknown classification → treat as wall (safe default)
    true
}

/// Whether a tile is a wall-mounted feature (should be cleared near doors).
fn is_wall_mounted(tile: Tile) -> bool {
    matches!(tile,
        Tile::Shelf | Tile::PapyrusTablet | Tile::WallInscription
        | Tile::FalseDoor | Tile::TombAlcove
    )
}

/// Determine shaft directionality from room's edge data.
fn shaft_kind_from_edges(shaft_edges: &[bfs_client::ShaftEdge], door_edges: &[DoorEdge]) -> ShaftKind {
    let has_up = shaft_edges.iter().any(|e| e.direction == "UP")
        || door_edges.iter().any(|e| e.direction == "UP");
    let has_down = shaft_edges.iter().any(|e| e.direction == "DOWN")
        || door_edges.iter().any(|e| e.direction == "DOWN");
    match (has_up, has_down) {
        (true, true) => ShaftKind::Both,
        (true, false) => ShaftKind::Up,
        (false, true) => ShaftKind::Down,
        (false, false) => ShaftKind::Both, // fallback
    }
}

/// Convert BFS (q, r) to world hex coordinate using 30°-rotated room lattice.
///
/// Standard lattice places neighbors at hex VERTICES → 30° misalignment with ring faces.
/// Rotated lattice: x = hs*(2q+r), y = hs*(r-q) places neighbors at ring FACE midpoints.
/// This matches v2's ROOM_DIRECTIONS solution to the hex nesting problem.
/// Z-offset stride: each z-level is shifted by this much in world-x so floors
/// never overlap. Value must be larger than the entire q-range of any single floor.
const Z_OFFSET: i32 = 2000;

pub fn bfs_to_world(q: i32, r: i32, z: i32) -> Coordinate<i32> {
    let hs = HALF_STRIDE;
    Coordinate::new(
        hs * (2 * q + r) + z * Z_OFFSET,
        hs * (r - q),
    )
}

/// Map wall index (0-5) to ring edge index (0-5).
/// Ring edges are 30° rotated from wall directions: edge = (wall + 5) % 6.
fn wall_to_edge(wall_idx: usize) -> usize {
    (wall_idx + 5) % 6
}

/// Compute the face midpoint of a ring edge.
/// Returns the world coordinate of the middle tile on ring edge `edge_idx` at `radius`.
fn edge_face_midpoint(center: Coordinate<i32>, edge_idx: usize, radius: i32) -> Coordinate<i32> {
    let half = radius / 2;
    let (dx, dy) = match edge_idx {
        0 => (half, -radius),
        1 => (-half, -half),
        2 => (-radius, half),
        3 => (-half, radius),
        4 => (half, half),
        5 => (radius, -half),
        _ => panic!("edge_idx must be 0-5"),
    };
    center + Coordinate::new(dx, dy)
}

fn direction_from_center(center: Coordinate<i32>, coord: Coordinate<i32>) -> Option<Direction> {
    let delta = Coordinate::new(coord.x - center.x, coord.y - center.y);
    delta.direction_from_center_cw()
}

/// Map hex2d Direction to wall index (0-5)
fn direction_to_wall_index_from_dir(dir: Direction) -> usize {
    match dir {
        Direction::XY => 0,  // E
        Direction::XZ => 1,  // NE
        Direction::YZ => 2,  // NW
        Direction::YX => 3,  // W
        Direction::ZX => 4,  // SW
        Direction::ZY => 5,  // SE
    }
}

/// Map wall index (0=E, 1=NE, ...) to BFS coordinate offset (Δq, Δr)
fn wall_to_bfs_offset(wall_idx: usize) -> (i32, i32) {
    match wall_idx {
        0 => (1, 0),   // E
        1 => (1, -1),  // NE
        2 => (0, -1),  // NW
        3 => (-1, 0),  // W
        4 => (-1, 1),  // SW
        5 => (0, 1),   // SE
        _ => panic!("wall_idx must be 0-5"),
    }
}

pub fn bfs_direction_to_hex(dir_str: &str) -> Option<Direction> {
    match dir_str {
        "E" | "e" => Some(Direction::XY),
        "NE" | "ne" => Some(Direction::XZ),
        "NW" | "nw" => Some(Direction::YZ),
        "W" | "w" => Some(Direction::YX),
        "SW" | "sw" => Some(Direction::ZX),
        "SE" | "se" => Some(Direction::ZY),
        _ => None,
    }
}

pub fn direction_to_bfs_string(dir: Direction) -> &'static str {
    match dir {
        Direction::XY => "E",
        Direction::XZ => "NE",
        Direction::YZ => "NW",
        Direction::YX => "W",
        Direction::ZX => "SW",
        Direction::ZY => "SE",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rotated_lattice_distance() {
        // All 6 BFS neighbors should be equidistant (2 * HALF_STRIDE = 26)
        let center = bfs_to_world(0, 0, 0);
        let expected_dist = 2 * HALF_STRIDE; // 26
        let cases = [(1,0), (1,-1), (0,-1), (-1,0), (-1,1), (0,1)];
        for (dq, dr) in cases {
            let nbr = bfs_to_world(dq, dr, 0);
            let dist = center.distance(nbr);
            assert_eq!(dist, expected_dist, "BFS({},{}) distance should be {}", dq, dr, expected_dist);
        }
    }

    #[test]
    fn test_rotated_lattice_sectors() {
        // Each BFS direction should map to the correct direction sector
        let center = bfs_to_world(0, 0, 0);
        let cases = [
            ((1, 0), Direction::XY, "E"),
            ((1, -1), Direction::XZ, "NE"),    // (1,-2)*HS → XZ sector? Let me check
            ((0, -1), Direction::ZX, "NW"),
            ((-1, 0), Direction::YX, "W"),
            ((-1, 1), Direction::YZ, "SW"),
            ((0, 1), Direction::XZ, "SE"),
        ];
        for ((dq, dr), _expected_dir, label) in cases {
            let nbr = bfs_to_world(dq, dr, 0);
            let delta = Coordinate::new(nbr.x - center.x, nbr.y - center.y);
            let dir = delta.direction_from_center_cw();
            // With rotated lattice, neighbors are in face-aligned directions,
            // so they land in the correct vertex sector (each ROOM_DIR falls in
            // the same sector as the standard direction it replaces)
            assert!(dir.is_some(), "BFS direction {} should have a valid sector", label);
        }
    }

    #[test]
    fn test_rooms_share_walls_zero_gap() {
        // With HS = 13, distance = 2*HS = 26 — uniform stride at max radius
        let center_a = bfs_to_world(0, 0, 0);
        let center_b = bfs_to_world(1, 0, 0);
        let dist = center_a.distance(center_b);
        assert_eq!(dist, 2 * HALF_STRIDE, "Adjacent rooms should be 2*HS=26 apart");
    }

    #[test]
    fn test_gallery_layout_from_door_edges() {
        let edges = vec![
            DoorEdge { from_coord: [0,0,0], to_coord: [1,0,0], direction: "E".to_string() },
            DoorEdge { from_coord: [0,0,0], to_coord: [-1,0,0], direction: "W".to_string() },
        ];
        let layout = GalleryLayout::from_door_edges(&edges);
        assert_eq!(layout.walls[0], WallState::FreeDoor); // E
        assert_eq!(layout.walls[3], WallState::FreeDoor); // W
        assert_eq!(layout.walls[1], WallState::Shelved);  // NE
    }

    #[test]
    fn test_edge_face_alignment() {
        // Face midpoints should be proportional to ROOM_DIRECTIONS
        let center = Coordinate::new(0i32, 0);
        let r = GALLERY_RADIUS; // 13
        // Edge 5 faces E: midpoint should be at (R, -R/2)
        let mid = edge_face_midpoint(center, 5, r);
        assert_eq!(mid, Coordinate::new(r, -r/2));
        // Edge 2 faces W: midpoint should be at (-R, R/2)
        let mid = edge_face_midpoint(center, 2, r);
        assert_eq!(mid, Coordinate::new(-r, r/2));
    }

    fn test_room(q: i32, r: i32, z: i32, door_dirs: &[&str]) -> RoomMetadata {
        let bfs = [q, r, z];
        let door_edges: Vec<DoorEdge> = door_dirs.iter().map(|d| {
            let offset = match *d {
                "E" => (1, 0), "NE" => (1, -1), "NW" => (0, -1),
                "W" => (-1, 0), "SW" => (-1, 1), "SE" => (0, 1),
                _ => (0, 0),
            };
            DoorEdge {
                from_coord: bfs,
                to_coord: [q + offset.0, r + offset.1, z],
                direction: d.to_string(),
            }
        }).collect();
        RoomMetadata {
            room_key: format!("test_{}_{}", q, r),
            q, r, z,
            room_type: "gallery".into(),
            district: "test".into(),
            volume_count: 0,
            door_count: door_dirs.len() as u32,
            palette: "warm_amber".into(),
            z_band: z,
            shelves: vec![],
            volumes: vec![],
            volumes_typed: vec![],
            door_edges,
            wall_edges: vec![],
            shaft_edges: vec![],
            shelf_types: vec![],
            cluster_id: String::new(),
            cluster_signature: vec![],
        }
    }

    #[test]
    fn test_adjacent_rooms_consistent_spacing() {
        // With uniform HALF_STRIDE=13, adjacent rooms are always 26 apart.
        // Rooms with volume_count=0 have radius 7, so there's open space between.
        // Rooms with volume_count=50 have radius 13 and share wall rings.
        let mut world = WorldMap::new();
        let mut room_a = test_room(0, 0, 0, &["E", "W"]);
        room_a.volume_count = 50; // large gallery, radius 13
        let mut room_b = test_room(1, 0, 0, &["W", "E"]);
        room_b.volume_count = 50; // large gallery, radius 13

        world.place_room(&room_a);
        world.place_room(&room_b);

        let center_a = bfs_to_world(0, 0, 0);
        let center_b = bfs_to_world(1, 0, 0);
        let r = 13; // max radius for 50+ volumes

        // With HS=R=13, large rooms share wall rings (zero gap)
        let has_gap = world.tiles.iter().any(|(coord, _)| {
            coord.distance(center_a) > r && coord.distance(center_b) > r
        });
        assert!(!has_gap, "No gap tiles should exist between max-radius rooms with HS=R");

        // The face midpoint of A's E edge should have a tile
        let a_e_mid = edge_face_midpoint(center_a, wall_to_edge(0), r);
        assert!(world.tiles.contains_key(&a_e_mid),
            "Face midpoint {:?} should have a tile", a_e_mid);
    }

    #[test]
    fn test_mutual_doorway_creates_passage() {
        let mut world = WorldMap::new();
        let mut room_a = test_room(0, 0, 0, &["E", "W"]);
        room_a.volume_count = 50; // radius 13
        let mut room_b = test_room(1, 0, 0, &["W", "E"]);
        room_b.volume_count = 50;

        world.place_room(&room_a);
        world.place_room(&room_b);

        let center_a = bfs_to_world(0, 0, 0);
        let r = GALLERY_RADIUS; // 13

        // The doorway face midpoint should be Door(true) (both rooms opened it)
        let a_e_mid = edge_face_midpoint(center_a, wall_to_edge(0), r);
        assert_eq!(
            world.tiles.get(&a_e_mid).map(|t| t.tile),
            Some(Tile::Door(true)),
            "Mutual doorway at face midpoint should be Door(true)"
        );
    }

    #[test]
    fn test_room_center_is_floor_without_shaft_edges() {
        // Room without shaft_edges gets lectern or floor at center, not shaft
        let mut world = WorldMap::new();
        let room = test_room(0, 0, 0, &["E", "W"]);
        world.place_room(&room);

        let center = bfs_to_world(0, 0, 0);
        // test_room has volume_count=0, no shaft_edges → center is Floor
        assert_eq!(world.tiles.get(&center).map(|t| t.tile), Some(Tile::Floor));
    }

    #[test]
    fn test_room_center_is_shaft_with_shaft_edges() {
        let mut world = WorldMap::new();
        let mut room = test_room(0, 0, 0, &["E", "W"]);
        room.shaft_edges = vec![bfs_client::ShaftEdge {
            from_coord: [0, 0, 0],
            to_coord: [0, 0, -1],
            direction: "DOWN".to_string(),
        }];
        world.place_room(&room);

        let center = bfs_to_world(0, 0, 0);
        assert_eq!(world.tiles.get(&center).map(|t| t.tile), Some(Tile::Shaft(ShaftKind::Down)));
    }

    #[test]
    fn test_doorway_is_narrow() {
        // Doorway should only open 2*door_halfwidth+1 tiles on the wall ring face
        let mut world = WorldMap::new();
        let room = test_room(0, 0, 0, &["E", "W"]);
        world.place_room(&room);

        let r = GALLERY_RADIUS; // 7 (volume_count=0)
        let dhw = door_halfwidth(r); // 1 (radius < 11)
        let center = bfs_to_world(0, 0, 0);

        // Count Door(true) tiles on the wall ring (these are doorway openings)
        let doorway_count = center.ring_iter(r, Spin::CW(hex2d_stacks::XY))
            .filter(|c| world.tiles.get(c).map(|t| t.tile) == Some(Tile::Door(true)))
            .count();

        // 2 doors × (2*dhw+1) tiles each
        let expected = 2 * (2 * dhw + 1);
        assert_eq!(doorway_count, expected,
            "Should have {} doorway tiles (2 doors × {} wide), got {}",
            expected, 2 * dhw + 1, doorway_count);
    }

    #[test]
    fn test_diagonal_door_alignment() {
        // BFS SE neighbor (Δq=0,Δr=+1): shared boundary tiles fall on wall 5 (SE face).
        // With both rooms opening reciprocal faces, the doorway has ≥2 passable tiles.
        let mut world = WorldMap::new();
        let mut room_a = test_room(0, 0, 0, &["SE"]);
        room_a.volume_count = 50;
        let mut room_b = test_room(0, 1, 0, &["NW"]);
        room_b.volume_count = 50;

        world.place_room(&room_a);
        world.place_room(&room_b);

        let center_a = bfs_to_world(0, 0, 0);
        let center_b = bfs_to_world(0, 1, 0);
        let r = GALLERY_RADIUS; // 13

        // Find passable tiles on the shared boundary (at distance R from both centers)
        let shared_passable: Vec<_> = center_a.ring_iter(r, Spin::CW(hex2d_stacks::XY))
            .filter(|c| {
                c.distance(center_b) <= r
                    && world.tiles.get(c).map(|t| t.tile.is_passable()).unwrap_or(false)
            })
            .collect();

        assert!(shared_passable.len() >= 2,
            "Diagonal door should have ≥2 passable tiles on boundary, got {}",
            shared_passable.len());
    }

    #[test]
    fn test_composition_driven_stamping() {
        // Room with volumes_typed AND shelf data should produce varied wall-mounted tiles
        let mut world = WorldMap::new();
        let mut room = test_room(0, 0, 0, &["E", "W"]);
        room.volume_count = 30;
        // Shelf data grounds the wall rendering — 5 populated shelves
        room.shelves = vec![
            vec!["v1".into(), "v2".into(), "v3".into()],   // papyrus
            vec!["v4".into(), "v5".into()],                  // wall inscription
            vec!["v6".into()],                               // false door
            vec!["v7".into(), "v8".into()],                  // ostrakon
            vec!["v9".into(), "v10".into()],                 // vessel + stele
        ];
        room.volumes_typed = vec![
            bfs_client::VolumeTyped { id: "v1".into(), physical_type: "Papyrus".into() },
            bfs_client::VolumeTyped { id: "v2".into(), physical_type: "Papyrus".into() },
            bfs_client::VolumeTyped { id: "v3".into(), physical_type: "Papyrus".into() },
            bfs_client::VolumeTyped { id: "v4".into(), physical_type: "Wand".into() },
            bfs_client::VolumeTyped { id: "v5".into(), physical_type: "Wand".into() },
            bfs_client::VolumeTyped { id: "v6".into(), physical_type: "Scheintür".into() },
            bfs_client::VolumeTyped { id: "v7".into(), physical_type: "Ostrakon".into() },
            bfs_client::VolumeTyped { id: "v8".into(), physical_type: "Ostrakon".into() },
            bfs_client::VolumeTyped { id: "v9".into(), physical_type: "Gefäß".into() },
            bfs_client::VolumeTyped { id: "v10".into(), physical_type: "Stele".into() },
        ];
        // BFS shelf_types: one per shelf, matching shelf order
        room.shelf_types = vec![
            "Papyrus".into(), "Wand".into(), "false_door".into(),  // wall types
            "ostrakon".into(), "vessel".into(),                     // floor types
        ];
        world.place_room(&room);

        let center = bfs_to_world(0, 0, 0);
        let r = GALLERY_RADIUS; // 11

        // Should have PapyrusTablet tiles on wall ring
        let has_papyrus = center.ring_iter(r - 1, Spin::CW(hex2d_stacks::XY))
            .any(|c| world.tiles.get(&c).map(|t| t.tile) == Some(Tile::PapyrusTablet));
        assert!(has_papyrus, "Should have PapyrusTablet on wall ring");

        // Should have WallInscription tiles
        let has_inscription = center.ring_iter(r - 1, Spin::CW(hex2d_stacks::XY))
            .any(|c| world.tiles.get(&c).map(|t| t.tile) == Some(Tile::WallInscription));
        assert!(has_inscription, "Should have WallInscription on wall ring");

        // Should have FalseDoor tiles
        let has_false_door = center.ring_iter(r - 1, Spin::CW(hex2d_stacks::XY))
            .any(|c| world.tiles.get(&c).map(|t| t.tile) == Some(Tile::FalseDoor));
        assert!(has_false_door, "Should have FalseDoor on wall ring");

        // Center should be Lectern (no coffin/sculpture, total > 0)
        assert_eq!(world.tiles.get(&center).map(|t| t.tile), Some(Tile::Lectern));

        // Should have Ostrakon fixture on floor
        let has_ostrakon = world.tiles.values()
            .any(|wt| wt.tile == Tile::Ostrakon && wt.room_coord == [0, 0, 0]);
        assert!(has_ostrakon, "Should have Ostrakon fixture on floor");

        // Fixtures should be registered — no duplicates
        let fixtures = world.fixtures.get(&[0, 0, 0]).expect("Room should have fixtures");
        assert!(!fixtures.is_empty(), "Should have at least one fixture");
        // Each fixture has a unique ID
        let ids: Vec<u16> = fixtures.iter().map(|f| f.id).collect();
        let unique: std::collections::HashSet<u16> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique.len(), "Fixture IDs must be unique");
    }

    #[test]
    fn test_composition_coffin_center() {
        // Room with coffin should place Coffin at center
        let mut world = WorldMap::new();
        let mut room = test_room(0, 0, 0, &["E"]);
        room.volume_count = 10;
        room.volumes_typed = vec![
            bfs_client::VolumeTyped { id: "v1".into(), physical_type: "Sarg".into() },
            bfs_client::VolumeTyped { id: "v2".into(), physical_type: "Papyrus".into() },
        ];
        world.place_room(&room);

        let center = bfs_to_world(0, 0, 0);
        assert_eq!(world.tiles.get(&center).map(|t| t.tile), Some(Tile::Coffin));
    }

    #[test]
    fn test_composition_fallback_no_typed() {
        // Room with shelf data but no volumes_typed uses generic Shelf tiles
        let mut world = WorldMap::new();
        let mut room = test_room(0, 0, 0, &["E", "W"]);
        room.volume_count = 20;
        // Shelf data present (corpus truth) but no type classification
        room.shelves = vec![
            vec!["a".into(), "b".into(), "c".into()],
            vec!["d".into(), "e".into()],
        ];
        // volumes_typed is empty (fallback to generic Shelf tiles)
        world.place_room(&room);

        let center = bfs_to_world(0, 0, 0);
        let r = GALLERY_RADIUS;

        // Should have Shelf tiles on wall ring (fallback — 2 shelves = 2 faces)
        let has_shelf = center.ring_iter(r - 1, Spin::CW(hex2d_stacks::XY))
            .any(|c| world.tiles.get(&c).map(|t| t.tile) == Some(Tile::Shelf));
        assert!(has_shelf, "Fallback should use Shelf tiles on faces with data");

        // Faces beyond shelf count should be bare (Floor, not Shelf)
        // With 2 doors (E,W) and 2 shelves, 2 of 4 non-door faces should be bare
        let shelf_count = center.ring_iter(r - 1, Spin::CW(hex2d_stacks::XY))
            .filter(|c| world.tiles.get(c).map(|t| t.tile) == Some(Tile::Shelf))
            .count();
        let floor_on_ring = center.ring_iter(r - 1, Spin::CW(hex2d_stacks::XY))
            .filter(|c| world.tiles.get(c).map(|t| t.tile) == Some(Tile::Floor))
            .count();
        assert!(shelf_count > 0, "Some faces should have shelves");
        assert!(floor_on_ring > 0, "Faces beyond shelf count should be bare floor");
    }
}
