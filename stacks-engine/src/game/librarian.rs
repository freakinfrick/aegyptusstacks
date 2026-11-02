use std::collections::VecDeque;
use hex2d_stacks::{Coordinate, Direction, Angle};
use super::actor::Actor;
use super::world::WorldMap;
use super::pathfinding::astar_next_step;

/// How many trail crumbs behind the player the librarian stays.
const TRAIL_LAG: usize = 1;

/// Maximum trail length before old crumbs are dropped.
const MAX_TRAIL: usize = 128;

/// Teleport if the librarian is this far from the player.
const TELEPORT_DISTANCE: i32 = 30;

/// Companion state — tracks the player's footstep trail.
pub struct LibrarianState {
    /// Player's movement trail: each entry is a position the player
    /// occupied, in order. The librarian follows this exact path.
    pub trail: VecDeque<Coordinate<i32>>,
}

impl LibrarianState {
    pub fn new() -> Self {
        LibrarianState {
            trail: VecDeque::with_capacity(MAX_TRAIL),
        }
    }

    /// Record a player position onto the trail (called on each player step).
    pub fn record_step(&mut self, pos: Coordinate<i32>) {
        self.trail.push_back(pos);
        if self.trail.len() > MAX_TRAIL {
            self.trail.pop_front();
        }
    }

    /// Clear the trail (called on z-level teleport).
    pub fn clear_trail(&mut self) {
        self.trail.clear();
    }
}

/// Scribe companion state — tracks the librarian's footstep trail.
pub struct ScribeState {
    pub trail: VecDeque<Coordinate<i32>>,
}

impl ScribeState {
    pub fn new() -> Self {
        ScribeState {
            trail: VecDeque::with_capacity(MAX_TRAIL),
        }
    }

    pub fn record_step(&mut self, pos: Coordinate<i32>) {
        self.trail.push_back(pos);
        if self.trail.len() > MAX_TRAIL {
            self.trail.pop_front();
        }
    }

    pub fn clear_trail(&mut self) {
        self.trail.clear();
    }
}

/// Decide and execute one librarian tick. Called once per player keypress.
///
/// The librarian follows the player's exact footstep trail, consuming
/// breadcrumbs to stay TRAIL_LAG steps behind. This guarantees the path
/// is walkable (the player walked it) and works across room boundaries.
pub fn tick_librarian(
    librarian: &mut Actor,
    state: &mut LibrarianState,
    player_pos: Coordinate<i32>,
    player_facing: Direction,
    world: &WorldMap,
) {
    // Always face the player
    librarian.face_toward(player_pos);

    // If player walked onto the librarian, sidestep to a free adjacent hex
    if librarian.coord() == player_pos {
        sidestep(librarian, player_pos, player_facing, world);
        return;
    }

    // Safety: if way too far, teleport
    let dist = librarian.coord().distance(player_pos);
    if dist > TELEPORT_DISTANCE {
        teleport_behind_player(librarian, player_pos, player_facing, world);
        state.clear_trail();
        return;
    }

    // Follow the trail: only move if trail is longer than the lag buffer.
    if state.trail.len() <= TRAIL_LAG {
        return;
    }

    // Consume up to 2 crumbs per tick so the librarian keeps pace
    // even with DoubleMove (W/X keys produce 2 trail entries per keypress).
    for _ in 0..2 {
        if state.trail.len() <= TRAIL_LAG {
            break;
        }

        if let Some(crumb) = state.trail.pop_front() {
            // Never step onto the player's current tile
            if crumb == player_pos {
                continue;
            }

            let crumb_dist = librarian.coord().distance(crumb);

            if crumb_dist <= 1 {
                librarian.pos.coord = crumb;
            } else if crumb_dist <= 6 {
                match astar_next_step(librarian.coord(), crumb, world, 10) {
                    Some(step) if step != player_pos => librarian.pos.coord = step,
                    Some(_) => {
                        // A* wants to step on the player — skip, put crumb back
                        state.trail.push_front(crumb);
                        break;
                    }
                    None => librarian.pos.coord = crumb,
                }
                if librarian.coord() != crumb {
                    state.trail.push_front(crumb);
                    break;
                }
            } else {
                teleport_behind_player(librarian, player_pos, player_facing, world);
                state.clear_trail();
                break;
            }
        }
    }
}

/// Tick the scribe follower — follows the librarian's trail.
/// Same logic as tick_librarian but with ScribeState and leader = librarian.
pub fn tick_follower(
    follower: &mut Actor,
    state: &mut ScribeState,
    leader_pos: Coordinate<i32>,
    leader_facing: Direction,
    world: &WorldMap,
) {
    follower.face_toward(leader_pos);

    if follower.coord() == leader_pos {
        sidestep(follower, leader_pos, leader_facing, world);
        return;
    }

    let dist = follower.coord().distance(leader_pos);
    if dist > TELEPORT_DISTANCE {
        teleport_behind_player(follower, leader_pos, leader_facing, world);
        state.clear_trail();
        return;
    }

    if state.trail.len() <= TRAIL_LAG {
        return;
    }

    for _ in 0..2 {
        if state.trail.len() <= TRAIL_LAG {
            break;
        }

        if let Some(crumb) = state.trail.pop_front() {
            if crumb == leader_pos {
                continue;
            }

            let crumb_dist = follower.coord().distance(crumb);

            if crumb_dist <= 1 {
                follower.pos.coord = crumb;
            } else if crumb_dist <= 6 {
                match astar_next_step(follower.coord(), crumb, world, 10) {
                    Some(step) if step != leader_pos => follower.pos.coord = step,
                    Some(_) => {
                        state.trail.push_front(crumb);
                        break;
                    }
                    None => follower.pos.coord = crumb,
                }
                if follower.coord() != crumb {
                    state.trail.push_front(crumb);
                    break;
                }
            } else {
                teleport_behind_player(follower, leader_pos, leader_facing, world);
                state.clear_trail();
                break;
            }
        }
    }
}

/// Sidestep: librarian moves to the nearest passable adjacent hex
/// that isn't the player's tile. Prefers behind the player.
fn sidestep(
    librarian: &mut Actor,
    player_pos: Coordinate<i32>,
    player_facing: Direction,
    world: &WorldMap,
) {
    // Prefer behind, then sides, then in front
    let back = player_facing + Angle::Back;
    let order = [
        back,
        back + Angle::Left,
        back + Angle::Right,
        player_facing + Angle::Left,
        player_facing + Angle::Right,
        player_facing,
    ];
    for dir in &order {
        let candidate = player_pos + *dir;
        if candidate != player_pos && is_passable_at(candidate, world) {
            librarian.pos.coord = candidate;
            return;
        }
    }
}

/// Place the librarian 2 hexes behind the player. Falls back through
/// alternative positions if the primary target is impassable.
pub fn teleport_behind_player(
    librarian: &mut Actor,
    player_pos: Coordinate<i32>,
    player_facing: Direction,
    world: &WorldMap,
) {
    let behind = player_facing + Angle::Back;

    // Primary: 2 hexes behind player
    let target = player_pos + behind + behind;
    if target != player_pos && is_passable_at(target, world) {
        librarian.pos.coord = target;
        return;
    }

    // Try all 6 directions at distance 2
    for &dir in Direction::all() {
        let candidate = player_pos + dir + dir;
        if candidate != player_pos && is_passable_at(candidate, world) {
            librarian.pos.coord = candidate;
            return;
        }
    }

    // Try all 6 directions at distance 1
    for &dir in Direction::all() {
        let candidate = player_pos + dir;
        if candidate != player_pos && is_passable_at(candidate, world) {
            librarian.pos.coord = candidate;
            return;
        }
    }
}

fn is_passable_at(coord: Coordinate<i32>, world: &WorldMap) -> bool {
    world.tiles.get(&coord)
        .map(|wt| wt.tile.is_passable())
        .unwrap_or(false)
}
