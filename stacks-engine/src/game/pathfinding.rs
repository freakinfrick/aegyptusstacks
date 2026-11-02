use std::collections::{BinaryHeap, HashMap};
use std::cmp::Reverse;
use hex2d_stacks::{Coordinate, Direction};
use super::world::WorldMap;

/// A* pathfinding on the hex grid.
/// Returns the first step from `start` toward `goal`, or None if unreachable
/// within `max_cost` steps.
pub fn astar_next_step(
    start: Coordinate<i32>,
    goal: Coordinate<i32>,
    world: &WorldMap,
    max_cost: u32,
) -> Option<Coordinate<i32>> {
    if start == goal {
        return None;
    }

    // f_score = g + h, stored as Reverse for min-heap
    let mut open: BinaryHeap<Reverse<(u32, Coordinate<i32>)>> = BinaryHeap::new();
    let mut g_score: HashMap<Coordinate<i32>, u32> = HashMap::new();
    let mut came_from: HashMap<Coordinate<i32>, Coordinate<i32>> = HashMap::new();

    let h = |c: Coordinate<i32>| c.distance(goal) as u32;

    g_score.insert(start, 0);
    open.push(Reverse((h(start), start)));

    while let Some(Reverse((_f, current))) = open.pop() {
        if current == goal {
            // Reconstruct: walk came_from back to find the first step from start
            let mut step = current;
            loop {
                match came_from.get(&step) {
                    Some(&prev) if prev == start => return Some(step),
                    Some(&prev) => step = prev,
                    None => return None, // shouldn't happen
                }
            }
        }

        let current_g = match g_score.get(&current) {
            Some(&g) => g,
            None => continue,
        };

        // Budget exceeded
        if current_g >= max_cost {
            continue;
        }

        for &neighbor in current.neighbors().iter() {
            // Passability: only tiles present in WorldMap and marked passable
            let passable = world.tiles.get(&neighbor)
                .map(|wt| wt.tile.is_passable())
                .unwrap_or(false);
            if !passable && neighbor != goal {
                // Allow goal itself even if "impassable" (player might be standing
                // on a shaft/lectern which is passable, but we don't want to
                // accidentally exclude the goal)
                continue;
            }

            let tentative_g = current_g + 1;
            if tentative_g < *g_score.get(&neighbor).unwrap_or(&u32::MAX) {
                came_from.insert(neighbor, current);
                g_score.insert(neighbor, tentative_g);
                open.push(Reverse((tentative_g + h(neighbor), neighbor)));
            }
        }
    }

    None // no path found within budget
}

/// Compute the full A* path from `start` to `goal`, returning a sequence of
/// Directions (one per hex step). Returns None if unreachable within `max_cost`.
pub fn astar_full_path(
    start: Coordinate<i32>,
    goal: Coordinate<i32>,
    world: &WorldMap,
    max_cost: u32,
) -> Option<Vec<Direction>> {
    if start == goal {
        return Some(Vec::new());
    }

    let mut open: BinaryHeap<Reverse<(u32, Coordinate<i32>)>> = BinaryHeap::new();
    let mut g_score: HashMap<Coordinate<i32>, u32> = HashMap::new();
    let mut came_from: HashMap<Coordinate<i32>, Coordinate<i32>> = HashMap::new();

    let h = |c: Coordinate<i32>| c.distance(goal) as u32;

    g_score.insert(start, 0);
    open.push(Reverse((h(start), start)));

    while let Some(Reverse((_f, current))) = open.pop() {
        if current == goal {
            // Reconstruct full path as coordinate sequence
            let mut path_coords = vec![current];
            let mut step = current;
            while let Some(&prev) = came_from.get(&step) {
                path_coords.push(prev);
                step = prev;
                if prev == start { break; }
            }
            path_coords.reverse();

            // Convert coordinate pairs to directions
            let mut directions = Vec::with_capacity(path_coords.len() - 1);
            for pair in path_coords.windows(2) {
                if let Some(dir) = coord_to_direction(pair[0], pair[1]) {
                    directions.push(dir);
                }
            }
            return Some(directions);
        }

        let current_g = match g_score.get(&current) {
            Some(&g) => g,
            None => continue,
        };

        if current_g >= max_cost {
            continue;
        }

        for &neighbor in current.neighbors().iter() {
            let passable = world.tiles.get(&neighbor)
                .map(|wt| wt.tile.is_passable())
                .unwrap_or(false);
            if !passable && neighbor != goal {
                continue;
            }

            let tentative_g = current_g + 1;
            if tentative_g < *g_score.get(&neighbor).unwrap_or(&u32::MAX) {
                came_from.insert(neighbor, current);
                g_score.insert(neighbor, tentative_g);
                open.push(Reverse((tentative_g + h(neighbor), neighbor)));
            }
        }
    }

    None
}

/// Determine which hex Direction leads from `from` to `to` (must be adjacent).
fn coord_to_direction(from: Coordinate<i32>, to: Coordinate<i32>) -> Option<Direction> {
    for &dir in Direction::all() {
        if from + dir == to {
            return Some(dir);
        }
    }
    None
}

/// Map a Direction to its keyboard key name (for agent consumption).
pub fn direction_to_key(dir: Direction) -> &'static str {
    match dir {
        Direction::YZ => "q",  // NW
        Direction::XZ => "e",  // NE
        Direction::XY => "d",  // E
        Direction::ZY => "c",  // SE
        Direction::ZX => "z",  // SW
        Direction::YX => "a",  // W
    }
}

/// Map a Direction to its compass name.
pub fn direction_to_compass(dir: Direction) -> &'static str {
    match dir {
        Direction::YZ => "NW",
        Direction::XZ => "NE",
        Direction::XY => "E",
        Direction::ZY => "SE",
        Direction::ZX => "SW",
        Direction::YX => "W",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex2d_stacks::{Coordinate, Direction};
    use crate::game::tile::Tile;
    use crate::game::world::WorldTile;

    fn floor_tile(room: [i32; 3]) -> WorldTile {
        WorldTile { tile: Tile::Floor, room_coord: room, face: None, fixture_id: None }
    }

    fn wall_tile(room: [i32; 3]) -> WorldTile {
        WorldTile { tile: Tile::Wall, room_coord: room, face: None, fixture_id: None }
    }

    #[test]
    fn test_straight_line_path() {
        let mut world = WorldMap::new();
        let room = [0, 0, 0];
        // Line of floor tiles from (0,0) to (5,0) in the XY direction
        for i in 0..=5 {
            let coord = Coordinate::new(0, 0) + Coordinate::<i32>::from(Direction::XY).scale(i);
            world.tiles.insert(coord, floor_tile(room));
        }

        let start = Coordinate::new(0, 0);
        let goal = Coordinate::new(0, 0) + Coordinate::<i32>::from(Direction::XY).scale(5);
        let step = astar_next_step(start, goal, &world, 30);
        assert!(step.is_some());
        // First step should be one hex in the XY direction
        let expected = start + Direction::XY;
        assert_eq!(step.unwrap(), expected);
    }

    #[test]
    fn test_no_path_returns_none() {
        let mut world = WorldMap::new();
        let room = [0, 0, 0];
        let start = Coordinate::new(0, 0);
        let goal = Coordinate::new(3, 0);
        // Only start tile exists, goal is in void
        world.tiles.insert(start, floor_tile(room));
        assert_eq!(astar_next_step(start, goal, &world, 30), None);
    }

    #[test]
    fn test_same_position_returns_none() {
        let world = WorldMap::new();
        let pos = Coordinate::new(0, 0);
        assert_eq!(astar_next_step(pos, pos, &world, 30), None);
    }

    #[test]
    fn test_budget_exceeded_returns_none() {
        let mut world = WorldMap::new();
        let room = [0, 0, 0];
        // Long line of floor tiles
        for i in 0..=10 {
            let coord = Coordinate::new(0, 0) + Coordinate::<i32>::from(Direction::XY).scale(i);
            world.tiles.insert(coord, floor_tile(room));
        }

        let start = Coordinate::new(0, 0);
        let goal = Coordinate::new(0, 0) + Coordinate::<i32>::from(Direction::XY).scale(10);
        // Budget of 5 can't reach distance 10
        assert_eq!(astar_next_step(start, goal, &world, 5), None);
    }

    #[test]
    fn test_routes_around_wall() {
        let mut world = WorldMap::new();
        let room = [0, 0, 0];
        // Create a small area with a wall blocking the direct path
        // Floor at (0,0), wall at XY, floor at XY+XY
        let start = Coordinate::new(0, 0);
        world.tiles.insert(start, floor_tile(room));

        let blocked = start + Direction::XY;
        world.tiles.insert(blocked, wall_tile(room));

        let goal = blocked + Direction::XY;
        world.tiles.insert(goal, floor_tile(room));

        // Add detour tiles
        let detour1 = start + Direction::XZ;
        let detour2 = detour1 + Direction::XY;
        world.tiles.insert(detour1, floor_tile(room));
        world.tiles.insert(detour2, floor_tile(room));

        let step = astar_next_step(start, goal, &world, 30);
        assert!(step.is_some());
        // Should NOT step into the wall
        assert_ne!(step.unwrap(), blocked);
    }
}
