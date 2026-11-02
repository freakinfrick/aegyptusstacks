use std::fmt;

/// Shaft directionality
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum ShaftKind {
    Up,
    Down,
    Both,
}

/// Tile types in The Stacks
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum Tile {
    Wall,
    Floor,
    Door(bool), // true = open
    Shelf,
    Shaft(ShaftKind),
    Lectern,
    /// Article display — holds a scribe article, passable, warm light
    ArticleDisplay,
    /// Decorative column (opaque, impassable)
    Column,
    /// Wall sconce (visual marker, transparent)
    Sconce,
    /// Low railing around the central ventilation shaft
    Railing,
    /// Vestibule corridor floor between galleries
    VestibuleFloor,
    /// Spiral staircase in a vestibule (z±1 traversal)
    Staircase,
    /// Closet alcove panel (metadata or summary)
    ClosetPanel,
    /// Mirror in vestibule
    Mirror,
    // ── Egyptian Kit-of-Parts ───────────────────────────────────
    /// Papyrus tablet display (impassable, semi-opaque)
    PapyrusTablet,
    /// Hieroglyphic wall inscription (impassable, semi-opaque)
    WallInscription,
    /// Scroll display pedestal (impassable, transparent)
    ScrollPedestal,
    /// False door — ritual boundary, fully opaque
    FalseDoor,
    /// Tomb alcove niche (impassable, semi-opaque)
    TombAlcove,
    /// Canopic or offering vessel (impassable, transparent)
    Vessel,
    /// Ostrakon fragment on the floor (passable)
    Ostrakon,
    /// Upright stele (impassable, semi-opaque)
    Stele,
    /// Sculpture / ankh statue (impassable, fully opaque)
    Sculpture,
    /// Sarcophagus / coffin (impassable, semi-opaque)
    Coffin,
    /// Raw rock face (impassable, fully opaque)
    RockFace,
    /// Shallow sacred pool (passable, transparent)
    Pool,
    /// Fire brazier (passable, transparent)
    Brazier,
    /// Magical portal (passable, transparent)
    Portal,
    /// Corridor floor — distinct pattern for passageways between rooms
    CorridorFloor,
}

impl Tile {
    /// Human-readable description
    pub fn description(&self) -> &str {
        match self {
            Tile::Wall => "wall",
            Tile::Floor => "floor",
            Tile::Door(true) => "open door",
            Tile::Door(false) => "closed door",
            Tile::Shelf => "shelf",
            Tile::Shaft(ShaftKind::Up) => "shaft (up)",
            Tile::Shaft(ShaftKind::Down) => "shaft (down)",
            Tile::Shaft(ShaftKind::Both) => "shaft (up/down)",
            Tile::Lectern => "lectern",
            Tile::ArticleDisplay => "article display",
            Tile::Column => "column",
            Tile::Sconce => "sconce",
            Tile::Railing => "railing",
            Tile::VestibuleFloor => "vestibule",
            Tile::Staircase => "staircase",
            Tile::ClosetPanel => "closet",
            Tile::Mirror => "mirror",
            Tile::PapyrusTablet => "papyrus tablet",
            Tile::WallInscription => "wall inscription",
            Tile::ScrollPedestal => "scroll pedestal",
            Tile::FalseDoor => "false door",
            Tile::TombAlcove => "tomb alcove",
            Tile::Vessel => "vessel",
            Tile::Ostrakon => "ostrakon",
            Tile::Stele => "stele",
            Tile::Sculpture => "sculpture",
            Tile::Coffin => "coffin",
            Tile::RockFace => "rock face",
            Tile::Pool => "pool",
            Tile::Brazier => "brazier",
            Tile::Portal => "portal",
            Tile::CorridorFloor => "corridor",
        }
    }

    /// Whether an actor can walk through this tile
    pub fn is_passable(&self) -> bool {
        match self {
            Tile::Wall => false,
            Tile::Floor => true,
            Tile::Door(open) => *open,
            Tile::Shelf => false,
            Tile::Shaft(_) => true,
            Tile::Lectern => true,
            Tile::ArticleDisplay => true,
            Tile::Column => false,
            Tile::Sconce => true,
            Tile::Railing => false,
            Tile::VestibuleFloor => true,
            Tile::Staircase => true,
            Tile::ClosetPanel => false,
            Tile::Mirror => false,
            Tile::PapyrusTablet => false,
            Tile::WallInscription => false,
            Tile::ScrollPedestal => false,
            Tile::FalseDoor => false,
            Tile::TombAlcove => false,
            Tile::Vessel => false,
            Tile::Ostrakon => true,
            Tile::Stele => false,
            Tile::Sculpture => false,
            Tile::Coffin => false,
            Tile::RockFace => false,
            Tile::Pool => true,
            Tile::Brazier => true,
            Tile::Portal => true,
            Tile::CorridorFloor => true,
        }
    }

    /// Whether this tile is a shaft (any direction)
    pub fn is_shaft(&self) -> bool {
        matches!(self, Tile::Shaft(_))
    }

    /// Compact numeric type id for agent tile dumps.
    pub fn type_id(&self) -> i32 {
        match self {
            Tile::Wall => 0,
            Tile::Floor => 1,
            Tile::Door(false) => 2,
            Tile::Door(true) => 3,
            Tile::Shelf => 4,
            Tile::Shaft(ShaftKind::Up) => 5,
            Tile::Shaft(ShaftKind::Down) => 6,
            Tile::Shaft(ShaftKind::Both) => 7,
            Tile::Lectern => 8,
            Tile::ArticleDisplay => 9,
            Tile::Column => 10,
            Tile::Sconce => 11,
            Tile::Railing => 12,
            Tile::VestibuleFloor => 13,
            Tile::Staircase => 14,
            Tile::ClosetPanel => 15,
            Tile::Mirror => 16,
            Tile::PapyrusTablet => 17,
            Tile::WallInscription => 18,
            Tile::ScrollPedestal => 19,
            Tile::FalseDoor => 20,
            Tile::TombAlcove => 21,
            Tile::Vessel => 22,
            Tile::Ostrakon => 23,
            Tile::Stele => 24,
            Tile::Sculpture => 25,
            Tile::Coffin => 26,
            Tile::RockFace => 27,
            Tile::Pool => 28,
            Tile::Brazier => 29,
            Tile::Portal => 30,
            Tile::CorridorFloor => 31,
        }
    }

    /// Whether this tile emits light
    pub fn is_light_source(&self) -> bool {
        matches!(self, Tile::Brazier | Tile::Sconce | Tile::Pool | Tile::Portal | Tile::Shaft(_) | Tile::ArticleDisplay)
    }

    /// Light radius in hex distance (0 = not a light source)
    pub fn light_radius(&self) -> u8 {
        match self {
            Tile::Brazier => 8,
            Tile::Sconce => 5,
            Tile::Lectern => 2,
            Tile::ArticleDisplay => 3,
            Tile::Pool => 2,
            Tile::Portal => 5,
            Tile::Shaft(_) => 3,
            _ => 0,
        }
    }

    /// RGB color temperature offset for this light source
    pub fn light_temperature(&self) -> (i8, i8, i8) {
        match self {
            Tile::Brazier => (25, 14, -8),
            Tile::Sconce => (18, 10, -5),
            Tile::Lectern => (5, 5, 8),
            Tile::ArticleDisplay => (15, 10, -3),
            Tile::Pool => (-5, -3, 12),
            Tile::Portal => (15, -5, 20),
            Tile::Shaft(_) => (-5, 10, 10),
            _ => (0, 0, 0),
        }
    }

    /// Whether this tile carries corpus text (can be inspected by the player).
    pub fn is_text_bearing(&self) -> bool {
        matches!(self,
            Tile::Shelf | Tile::PapyrusTablet | Tile::WallInscription |
            Tile::ScrollPedestal | Tile::FalseDoor | Tile::TombAlcove |
            Tile::Stele | Tile::Sculpture | Tile::Coffin | Tile::Vessel |
            Tile::Ostrakon | Tile::RockFace
        )
    }
}

impl fmt::Display for Tile {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

impl Default for Tile {
    fn default() -> Tile {
        Tile::Wall
    }
}
