use super::actor::{Actor, ActorKind};
use super::librarian::{self, LibrarianState, ScribeState};
use super::tile::Tile;
use super::world::WorldMap;
use hex2d_stacks::{Coordinate, Direction};

/// Action a player or AI can take
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Action {
    Move(Direction),
    /// Two consecutive hex moves — used for cardinal up/down on screen
    DoubleMove(Direction, Direction),
    Ascend,
    Descend,
    Interact,
    Wait,
    Quit,
}

/// Result of processing an action
#[derive(Clone, Debug, PartialEq)]
pub enum ActionResult {
    Ok,
    Quit,
    Blocked(&'static str),
    /// Player entered a new room — caller should load its neighbors
    EnteredRoom([i32; 3]),
    /// Inspect a text-bearing tile at this world coordinate
    InspectShelf(hex2d_stacks::Coordinate<i32>),
    ReadLectern,
    /// Player interacted with an article display tile
    ReadArticle,
    /// Player interacted with the librarian
    LibrarianInteract,
    /// Player interacted with the scribe
    ScribeInteract,
    /// Player stepped onto an active portal tile
    PortalEnter,
}

/// The game engine — manages player and delegates tile queries to WorldMap.
pub struct Engine {
    pub turn: u64,
    pub player: Actor,
    pub librarians: Vec<Actor>,
    pub librarian_state: LibrarianState,
    pub scribes: Vec<Actor>,
    pub scribe_state: ScribeState,
    /// Which BFS room the player is currently in
    pub current_room: [i32; 3],
}

impl Engine {
    pub fn new() -> Self {
        let player = Actor::new(
            ActorKind::Player,
            Coordinate::new(0, 0),
            Direction::YZ,
        );

        Engine {
            turn: 0,
            player,
            librarians: Vec::new(),
            librarian_state: LibrarianState::new(),
            scribes: Vec::new(),
            scribe_state: ScribeState::new(),
            current_room: [0, 0, 0],
        }
    }

    /// Process a player action against the world map
    pub fn process_action(&mut self, action: Action, world: &WorldMap) -> ActionResult {
        let result = match action {
            Action::Move(dir) => self.try_move(dir, world),
            Action::DoubleMove(dir1, dir2) => {
                let r1 = self.try_move(dir1, world);
                match r1 {
                    ActionResult::Ok => self.try_move(dir2, world),
                    other => other, // blocked or entered room on first step
                }
            }
            Action::Ascend => self.try_shaft(true, world),
            Action::Descend => self.try_shaft(false, world),
            Action::Interact => self.try_interact(world),
            Action::Wait => { self.turn += 1; ActionResult::Ok }
            Action::Quit => ActionResult::Quit,
        };

        // Tick the librarian companion after each player action (except Quit)
        if !matches!(action, Action::Quit) {
            if let Some(lib) = self.librarians.first_mut() {
                librarian::tick_librarian(
                    lib,
                    &mut self.librarian_state,
                    self.player.coord(),
                    self.player.facing(),
                    world,
                );
            }

            // Record librarian footstep for scribe trail, then tick scribe
            if let Some(lib) = self.librarians.first() {
                self.scribe_state.record_step(lib.coord());
            }
            if let Some(scribe) = self.scribes.first_mut() {
                let leader_coord = self.librarians.first()
                    .map(|l| l.coord()).unwrap_or(self.player.coord());
                let leader_facing = self.librarians.first()
                    .map(|l| l.facing()).unwrap_or(self.player.facing());
                librarian::tick_follower(
                    scribe,
                    &mut self.scribe_state,
                    leader_coord,
                    leader_facing,
                    world,
                );
            }
        }

        result
    }

    /// Teleport the librarian to behind the player (used on z-level transitions).
    pub fn teleport_librarian_to_player(&mut self, world: &WorldMap) {
        if let Some(lib) = self.librarians.first_mut() {
            librarian::teleport_behind_player(
                lib,
                self.player.coord(),
                self.player.facing(),
                world,
            );
        }
        self.librarian_state.clear_trail();
    }

    /// Teleport the scribe to behind the librarian (used on z-level transitions).
    pub fn teleport_scribe_to_librarian(&mut self, world: &WorldMap) {
        if let (Some(scribe), Some(lib)) = (self.scribes.first_mut(), self.librarians.first()) {
            librarian::teleport_behind_player(
                scribe,
                lib.coord(),
                lib.facing(),
                world,
            );
        }
        self.scribe_state.clear_trail();
    }

    fn try_move(&mut self, dir: Direction, world: &WorldMap) -> ActionResult {
        let target = self.player.coord() + dir;

        match world.tiles.get(&target) {
            Some(wt) if wt.tile.is_passable() => {
                // Record footstep for librarian trail BEFORE moving
                self.librarian_state.record_step(self.player.coord());
                self.player.pos.coord = target;
                self.player.pos.dir = dir;
                self.turn += 1;

                // Always update current_room to the tile we stepped on
                let old_room = self.current_room;
                self.current_room = wt.room_coord;

                // Portal step — teleport handled by the caller
                if wt.tile == Tile::Portal {
                    return ActionResult::PortalEnter;
                }

                if self.current_room != old_room {
                    ActionResult::EnteredRoom(self.current_room)
                } else {
                    ActionResult::Ok
                }
            }
            Some(wt) => {
                match wt.tile {
                    Tile::Wall | Tile::Shelf | Tile::ClosetPanel | Tile::Mirror | Tile::Column => {
                        ActionResult::Blocked("can't walk there")
                    }
                    Tile::Railing => ActionResult::Blocked("low railing blocks the way"),
                    Tile::Door(false) => ActionResult::Blocked("door is closed"),
                    _ => ActionResult::Blocked("blocked"),
                }
            }
            None => ActionResult::Blocked("void"),
        }
    }

    fn try_shaft(&mut self, up: bool, world: &WorldMap) -> ActionResult {
        let coord = self.player.coord();
        let dir_str = if up { "UP" } else { "DOWN" };

        // Must be standing on a shaft tile
        let wt = match world.tiles.get(&coord) {
            Some(wt) if wt.tile.is_shaft() || wt.tile == Tile::Staircase => wt,
            _ => return ActionResult::Blocked("not on a shaft"),
        };

        if let Some(room) = world.rooms.get(&wt.room_coord) {
            for edge in &room.info.shaft_edges {
                if edge.direction == dir_str {
                    self.turn += 1;
                    return ActionResult::EnteredRoom(edge.to_coord);
                }
            }
            for edge in &room.info.door_edges {
                if edge.direction == dir_str {
                    self.turn += 1;
                    return ActionResult::EnteredRoom(edge.to_coord);
                }
            }
        }

        ActionResult::Blocked(if up { "shaft doesn't go up" } else { "shaft doesn't go down" })
    }

    /// Find ALL shaft/staircase rooms within range 2 of player.
    fn find_all_shaft_rooms(&self, world: &WorldMap) -> Vec<[i32; 3]> {
        let coord = self.player.coord();
        let mut rooms = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for &nearby in &self.tiles_in_range(coord, 2) {
            if let Some(wt) = world.tiles.get(&nearby) {
                if wt.tile.is_shaft() || wt.tile == Tile::Staircase {
                    if seen.insert(wt.room_coord) {
                        rooms.push(wt.room_coord);
                    }
                }
            }
        }
        rooms
    }

    /// Get all hex coordinates within `range` steps of `center` (including center).
    fn tiles_in_range(&self, center: Coordinate<i32>, range: i32) -> Vec<Coordinate<i32>> {
        let mut result = vec![center];
        for r in 1..=range {
            for coord in center.ring_iter(r, hex2d_stacks::Spin::CW(hex2d_stacks::XY)) {
                result.push(coord);
            }
        }
        result
    }

    fn try_interact(&mut self, world: &WorldMap) -> ActionResult {
        let coord = self.player.coord();

        // Shaft + Lectern: must be standing ON the tile
        if let Some(wt) = world.tiles.get(&coord) {
            if wt.tile.is_shaft() || wt.tile == Tile::Staircase {
                // Try shaft traversal (down first, then up)
                if let Some(room) = world.rooms.get(&wt.room_coord) {
                    for dir_str in &["DOWN", "UP"] {
                        for edge in &room.info.shaft_edges {
                            if edge.direction == *dir_str {
                                return ActionResult::EnteredRoom(edge.to_coord);
                            }
                        }
                        for edge in &room.info.door_edges {
                            if edge.direction == *dir_str {
                                return ActionResult::EnteredRoom(edge.to_coord);
                            }
                        }
                    }
                }
                return ActionResult::Blocked("shaft has no connection");
            }
            if wt.tile == Tile::ArticleDisplay {
                return ActionResult::ReadArticle;
            }
            if wt.tile == Tile::Lectern {
                return ActionResult::ReadLectern;
            }
        }
        // Check for NPCs: standing on, facing distance 1-2, then any adjacent
        let facing = self.player.facing();

        // Standing ON an NPC (distance 0)
        if self.librarians.iter().any(|l| l.coord() == coord) {
            return ActionResult::LibrarianInteract;
        }
        if self.scribes.iter().any(|s| s.coord() == coord) {
            return ActionResult::ScribeInteract;
        }

        // Librarian: facing distance 1-2, then any adjacent
        for dist in 1..=2 {
            let mut check = coord;
            for _ in 0..dist { check = check + facing; }
            if self.librarians.iter().any(|l| l.coord() == check) {
                return ActionResult::LibrarianInteract;
            }
        }
        for &dir in Direction::all() {
            if dir == facing { continue; }
            let check = coord + dir;
            if self.librarians.iter().any(|l| l.coord() == check) {
                return ActionResult::LibrarianInteract;
            }
        }
        // Scribe: facing distance 1-2, then any adjacent
        for dist in 1..=2 {
            let mut check = coord;
            for _ in 0..dist { check = check + facing; }
            if self.scribes.iter().any(|s| s.coord() == check) {
                return ActionResult::ScribeInteract;
            }
        }
        for &dir in Direction::all() {
            if dir == facing { continue; }
            let check = coord + dir;
            if self.scribes.iter().any(|s| s.coord() == check) {
                return ActionResult::ScribeInteract;
            }
        }

        // Check for text-bearing features within range 2.
        // Prioritize facing direction, then sweep all directions.
        let facing = self.player.facing();
        // Facing: check distance 1 and 2
        for dist in 1..=2 {
            let mut target = coord;
            for _ in 0..dist { target = target + facing; }
            if let Some(wt) = world.tiles.get(&target) {
                if wt.tile.is_text_bearing() {
                    return ActionResult::InspectShelf(target);
                }
            }
        }
        // All other directions: distance 1 then 2
        for dist in 1..=2i32 {
            for &dir in Direction::all() {
                if dir == facing { continue; }
                let mut target = coord;
                for _ in 0..dist { target = target + dir; }
                if let Some(wt) = world.tiles.get(&target) {
                    if wt.tile.is_text_bearing() {
                        return ActionResult::InspectShelf(target);
                    }
                }
            }
        }
        ActionResult::Blocked("nothing to interact with")
    }
}
