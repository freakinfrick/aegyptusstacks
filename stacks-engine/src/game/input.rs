use crossterm::event::KeyCode;
use hex2d_stacks::Direction;
use super::engine::Action;

/// Map key press to game action.
///
/// 8-directional movement on hex grid:
///
///     Q (NW)   W (up)   E (NE)
///          \     |     /
///     A (W) --- @ --- D (E)
///          /     |     \
///     Z (SW)   X (down)  C (SE)
///
/// W/X = cardinal up/down (2 hex steps: NW+NE / SE+SW)
/// A/D = cardinal left/right (single hex step: W / E)
/// Q/E/Z/C = diagonal (single hex step: NW / NE / SW / SE)
///
pub fn map_key_to_action(key: KeyCode) -> Option<Action> {
    match key {
        // Cardinal left/right (single hex step)
        KeyCode::Char('a') | KeyCode::Char('A') => Some(Action::Move(Direction::YX)),  // W (left)
        KeyCode::Char('d') | KeyCode::Char('D') => Some(Action::Move(Direction::XY)),  // E (right)
        // Cardinal up/down (double hex step — jumps 2 tiles for straight vertical)
        KeyCode::Char('w') | KeyCode::Char('W') => Some(Action::DoubleMove(Direction::YZ, Direction::XZ)),  // NW then NE = straight up
        KeyCode::Char('x') | KeyCode::Char('X') => Some(Action::DoubleMove(Direction::ZY, Direction::ZX)),  // SE then SW = straight down
        // Diagonals (single hex step)
        KeyCode::Char('q') | KeyCode::Char('Q') => Some(Action::Move(Direction::YZ)),  // NW
        KeyCode::Char('e') | KeyCode::Char('E') => Some(Action::Move(Direction::XZ)),  // NE
        KeyCode::Char('z') | KeyCode::Char('Z') => Some(Action::Move(Direction::ZX)),  // SW
        KeyCode::Char('c') | KeyCode::Char('C') => Some(Action::Move(Direction::ZY)),  // SE
        // Vertical movement (shafts/stairs between z-levels)
        KeyCode::Char('r') | KeyCode::Up => Some(Action::Ascend),
        KeyCode::Char('f') | KeyCode::Down => Some(Action::Descend),
        // Interaction
        KeyCode::Char(' ') => Some(Action::Interact),
        KeyCode::Char('.') => Some(Action::Wait),
        // Esc handled by main loop (opens pause menu)
        KeyCode::Esc => None,
        _ => None,
    }
}
