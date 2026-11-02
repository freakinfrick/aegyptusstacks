use hex2d_stacks::{Coordinate, Direction, Position};
use ratatui::style::Color;

/// The kind of actor
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ActorKind {
    Player,
    Librarian,
    Scribe,
}

impl ActorKind {
    /// Canonical 2-char glyph for the world view.
    pub const fn glyph(&self) -> &'static str {
        match self {
            ActorKind::Player => "@@",
            ActorKind::Librarian => "&&",
            ActorKind::Scribe => "%%",
        }
    }

    /// Canonical 1-char glyph for the minimap.
    pub const fn minimap_glyph(&self) -> char {
        match self {
            ActorKind::Player => '@',
            ActorKind::Librarian => '&',
            ActorKind::Scribe => '%',
        }
    }

    /// Canonical color for this actor kind.
    pub const fn color(&self) -> Color {
        match self {
            ActorKind::Player => Color::Green,
            ActorKind::Librarian => Color::Magenta,
            ActorKind::Scribe => Color::Rgb(0, 210, 210),
        }
    }
}

/// An actor in the world — stripped of combat, items, races
#[derive(Clone, Debug)]
pub struct Actor {
    /// What kind of actor
    pub kind: ActorKind,
    /// Position (coordinate + facing direction)
    pub pos: Position<i32>,
}

impl Actor {
    /// Create a new actor at the given position
    pub fn new(kind: ActorKind, coord: Coordinate<i32>, facing: Direction) -> Self {
        Actor {
            kind,
            pos: Position::new(coord, facing),
        }
    }

    /// The coordinate this actor occupies
    pub fn coord(&self) -> Coordinate<i32> {
        self.pos.coord
    }

    /// The direction this actor is facing
    pub fn facing(&self) -> Direction {
        self.pos.dir
    }

    /// The coordinate in front of this actor
    pub fn head(&self) -> Coordinate<i32> {
        self.pos.coord + self.pos.dir
    }

    /// Whether this is the player
    pub fn is_player(&self) -> bool {
        self.kind == ActorKind::Player
    }

    /// Update facing to look toward a target coordinate.
    /// No-op if target is the same tile (delta zero → no direction).
    pub fn face_toward(&mut self, target: Coordinate<i32>) {
        if let Some(dir) = self.pos.coord.direction_to_cw(target) {
            self.pos.dir = dir;
        }
    }
}
