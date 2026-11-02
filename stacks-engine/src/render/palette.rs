use ratatui::style::Color;

/// Chronological era palette (replaces the 5-tier z-band system).
/// Each era maps to a distinct z-level per DESIGN.md §2.1.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Tier {
    /// z >= 0: warm amber — undated / index / observatory
    Surface,
    /// z = -1: sandstone gold — Pyramid Texts, dawn of writing
    OldKingdom,
    /// z = -2: faded ochre — fragmentation, Coffin Texts emerge
    FirstIntermediate,
    /// z = -3: honey papyrus — classical language, Sinuhe
    MiddleKingdom,
    /// z = -4: dust corridor — Hyksos, transitional
    SecondIntermediate,
    /// z = -5: lapis empire — Book of the Dead, Ramesside
    NewKingdom,
    /// z = -6: sage archaism — Demotic rises, Saite renaissance
    LatePeriod,
    /// z <= -7: phosphor syncretic — Ptolemies, Rome, Coptic
    GrecoRoman,
    /// Observatory hub — pale luminous lavender/blue whites
    Ethereal,
    /// Scribe article galleries — deep cyan/teal
    Scribe,
}

impl Tier {
    pub fn from_z(z: i32) -> Self {
        match z {
            z if z >= 0 => Tier::Surface,
            -1 => Tier::OldKingdom,
            -2 => Tier::FirstIntermediate,
            -3 => Tier::MiddleKingdom,
            -4 => Tier::SecondIntermediate,
            -5 => Tier::NewKingdom,
            -6 => Tier::LatePeriod,
            _ => Tier::GrecoRoman, // z <= -7
        }
    }

    /// Whether this tier represents an index/surface floor (z >= 0).
    pub fn is_surface(&self) -> bool {
        matches!(self, Tier::Surface)
    }

    /// Whether this tier represents the deepest era (Greco-Roman or below).
    pub fn is_deepest(&self) -> bool {
        matches!(self, Tier::GrecoRoman)
    }
}

/// District color modifier (RGB offsets applied to base palette).
fn district_tint(district: &str) -> (i16, i16, i16) {
    match district {
        "temple" => (20, 10, -15),
        "necropolis" => (-15, -10, 25),
        "royal" => (25, 15, -10),
        "house_of_life" => (10, 15, -5),
        "admin" => (5, 0, -5),
        "school" => (15, 10, -10),
        "quarry" => (-10, -5, -5),
        "domestic" => (20, 5, -15),
        "unplaced" => (-10, -10, 5),
        "observatory" => (0, 0, 0),        // tier override handles palette
        "scribe_gallery" => (0, 0, 0),    // tier override handles palette
        _ => (0, 0, 0),
    }
}

/// Full palette: era base + district modifier.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Palette {
    pub tier: Tier,
    pub district_r: i16,
    pub district_g: i16,
    pub district_b: i16,
}

impl Palette {
    /// Create palette from z-level and district.
    pub fn from_room(z: i32, district: &str) -> Self {
        // Special districts override the z-derived tier entirely
        let tier = match district {
            "observatory" => Tier::Ethereal,
            "scribe_gallery" => Tier::Scribe,
            _ => Tier::from_z(z),
        };
        let (dr, dg, db) = district_tint(district);
        Palette { tier, district_r: dr, district_g: dg, district_b: db }
    }

    /// Legacy: create from tier alone (no district tint).
    pub fn from_tier(tier: Tier) -> Self {
        Palette { tier, district_r: 0, district_g: 0, district_b: 0 }
    }

    fn tint(&self, base: Color) -> Color {
        match base {
            Color::Rgb(r, g, b) => Color::Rgb(
                (r as i16 + self.district_r).clamp(0, 255) as u8,
                (g as i16 + self.district_g).clamp(0, 255) as u8,
                (b as i16 + self.district_b).clamp(0, 255) as u8,
            ),
            other => other,
        }
    }

    fn brighten(r: u8, g: u8, b: u8, amount: i16) -> Color {
        Color::Rgb(
            (r as i16 + amount).clamp(0, 255) as u8,
            (g as i16 + amount).clamp(0, 255) as u8,
            (b as i16 + amount).clamp(0, 255) as u8,
        )
    }

    // ── Base era colors ──────────────────────────────────────────

    pub fn wall_color(&self) -> Color {
        let base = match self.tier {
            Tier::Surface           => Color::Rgb(140, 115, 70),
            Tier::OldKingdom        => Color::Rgb(160, 130, 80),
            Tier::FirstIntermediate => Color::Rgb(100, 90, 75),
            Tier::MiddleKingdom     => Color::Rgb(150, 120, 70),
            Tier::SecondIntermediate=> Color::Rgb(90, 80, 70),
            Tier::NewKingdom        => Color::Rgb(85, 70, 150),
            Tier::LatePeriod        => Color::Rgb(75, 115, 85),
            Tier::GrecoRoman        => Color::Rgb(55, 125, 115),
            Tier::Ethereal          => Color::Rgb(180, 175, 210),   // bright lavender stone
            Tier::Scribe            => Color::Rgb(120, 190, 200),  // bright airy cyan
        };
        self.tint(base)
    }

    pub fn floor_color(&self) -> Color {
        let base = match self.tier {
            Tier::Surface           => Color::Rgb(100, 80, 40),
            Tier::OldKingdom        => Color::Rgb(120, 95, 50),
            Tier::FirstIntermediate => Color::Rgb(80, 70, 55),
            Tier::MiddleKingdom     => Color::Rgb(110, 85, 45),
            Tier::SecondIntermediate=> Color::Rgb(70, 60, 50),
            Tier::NewKingdom        => Color::Rgb(60, 50, 110),
            Tier::LatePeriod        => Color::Rgb(55, 85, 60),
            Tier::GrecoRoman        => Color::Rgb(40, 95, 85),
            Tier::Ethereal          => Color::Rgb(150, 145, 185),  // light lavender
            Tier::Scribe            => Color::Rgb(90, 160, 175),   // light teal
        };
        self.tint(base)
    }

    pub fn shelf_color(&self) -> Color {
        let base = match self.tier {
            Tier::Surface           => Color::Rgb(160, 120, 50),
            Tier::OldKingdom        => Color::Rgb(180, 140, 60),
            Tier::FirstIntermediate => Color::Rgb(120, 100, 65),
            Tier::MiddleKingdom     => Color::Rgb(170, 130, 55),
            Tier::SecondIntermediate=> Color::Rgb(110, 90, 60),
            Tier::NewKingdom        => Color::Rgb(100, 80, 165),
            Tier::LatePeriod        => Color::Rgb(90, 135, 85),
            Tier::GrecoRoman        => Color::Rgb(65, 165, 145),
            Tier::Ethereal          => Color::Rgb(200, 195, 235),  // pale violet shelves
            Tier::Scribe            => Color::Rgb(140, 210, 220),  // bright cyan shelves
        };
        self.tint(base)
    }

    pub fn accent_color(&self) -> Color {
        match self.tier {
            Tier::Surface           => Color::Rgb(200, 160, 40),
            Tier::OldKingdom        => Color::Rgb(220, 180, 40),
            Tier::FirstIntermediate => Color::Rgb(160, 140, 80),
            Tier::MiddleKingdom     => Color::Rgb(200, 160, 50),
            Tier::SecondIntermediate=> Color::Rgb(140, 120, 70),
            Tier::NewKingdom        => Color::Rgb(220, 180, 40),
            Tier::LatePeriod        => Color::Rgb(120, 180, 100),
            Tier::GrecoRoman        => Color::Rgb(80, 220, 200),
            Tier::Ethereal          => Color::Rgb(220, 215, 250),  // luminous white-violet
            Tier::Scribe            => Color::Rgb(100, 240, 240),  // bright scribe cyan
        }
    }

    // ── Feature-specific colors (fixed, not palette-dependent) ──

    pub fn shaft_color(&self) -> Color {
        Color::Rgb(80, 220, 200) // cyan — always
    }

    pub fn door_color(&self) -> Color {
        Color::Rgb(220, 180, 40) // gold — always
    }

    pub fn lectern_color(&self) -> Color {
        Color::Rgb(240, 235, 220) // warm white — always
    }

    pub fn article_display_color(&self) -> Color {
        Color::Rgb(220, 180, 80) // warm gold/parchment — always
    }

    pub fn column_color(&self) -> Color {
        // wall_color() + 20 brightness on each channel
        match self.wall_color() {
            Color::Rgb(r, g, b) => Self::brighten(r, g, b, 20),
            other => other,
        }
    }

    pub fn sconce_color(&self) -> Color {
        self.accent_color() // bright, warm — from palette
    }

    pub fn railing_color(&self) -> Color {
        self.accent_color()
    }

    pub fn vestibule_floor_color(&self) -> Color {
        let base = match self.tier {
            Tier::NewKingdom => Color::Rgb(40, 35, 75),
            Tier::GrecoRoman => Color::Rgb(30, 70, 65),
            Tier::LatePeriod => Color::Rgb(45, 65, 50),
            Tier::Ethereal   => Color::Rgb(140, 135, 175),
            Tier::Scribe     => Color::Rgb(80, 150, 165),
            _ => Color::Rgb(70, 55, 30),
        };
        self.tint(base)
    }

    pub fn staircase_color(&self) -> Color {
        Color::Rgb(220, 180, 40) // gold, matching door
    }

    pub fn closet_color(&self) -> Color {
        let base = match self.tier {
            Tier::NewKingdom => Color::Rgb(70, 65, 120),
            Tier::GrecoRoman => Color::Rgb(50, 90, 85),
            Tier::LatePeriod => Color::Rgb(55, 85, 60),
            _ => Color::Rgb(110, 80, 40),
        };
        self.tint(base)
    }

    pub fn mirror_color(&self) -> Color {
        Color::Rgb(200, 200, 220)
    }

    // ── Corpus fixture colors (FIXED — tier/district independent) ──
    // Bright and vibrant so they pop against architectural backdrop at any depth.

    /// Papyrus tablet — vivid amber
    pub fn papyrus_color(&self) -> Color {
        Color::Rgb(240, 200, 80)
    }

    /// Wall inscription — blazing gold
    pub fn inscription_color(&self) -> Color {
        Color::Rgb(255, 220, 60)
    }

    /// Scroll pedestal — warm cream
    pub fn scroll_color(&self) -> Color {
        Color::Rgb(235, 225, 180)
    }

    /// False door — vivid ritual crimson
    pub fn false_door_color(&self) -> Color {
        Color::Rgb(220, 75, 60)
    }

    /// Tomb alcove — rich violet
    pub fn tomb_alcove_color(&self) -> Color {
        Color::Rgb(140, 80, 180)
    }

    /// Vessel — bright terracotta
    pub fn vessel_color(&self) -> Color {
        Color::Rgb(230, 140, 65)
    }

    /// Ostrakon — warm ochre
    pub fn ostrakon_color(&self) -> Color {
        Color::Rgb(220, 190, 90)
    }

    /// Stele — bright silver
    pub fn stele_color(&self) -> Color {
        Color::Rgb(200, 195, 185)
    }

    /// Sculpture — vivid faience turquoise
    pub fn sculpture_color(&self) -> Color {
        Color::Rgb(60, 210, 185)
    }

    /// Coffin — vivid royal purple
    pub fn coffin_color(&self) -> Color {
        Color::Rgb(150, 80, 200)
    }

    /// Rock face — warm sienna
    pub fn rock_face_color(&self) -> Color {
        Color::Rgb(195, 120, 70)
    }

    // ── Environment colors (FIXED) ──────────────────────────────

    pub fn pool_color(&self) -> Color {
        Color::Rgb(60, 120, 180)
    }

    pub fn brazier_color(&self) -> Color {
        Color::Rgb(220, 140, 40)
    }

    pub fn portal_color(&self) -> Color {
        Color::Rgb(180, 60, 200)
    }

    // ── Corridor colors (darkened, recessive) ──────────────

    /// Corridor walls: era wall color darkened significantly — recedes behind content rooms
    pub fn corridor_wall_color(&self) -> Color {
        match self.wall_color() {
            Color::Rgb(r, g, b) => Self::brighten(r, g, b, -55),
            other => other,
        }
    }

    /// Corridor floor: era floor color darkened + cool shift — visually subordinate
    pub fn corridor_floor_color(&self) -> Color {
        match self.floor_color() {
            Color::Rgb(r, g, b) => Color::Rgb(
                (r as i16 - 30).clamp(0, 255) as u8,
                (g as i16 - 20).clamp(0, 255) as u8,
                (b as i16 + 5).clamp(0, 255) as u8,
            ),
            other => other,
        }
    }

    /// Corridor column: slightly brighter than corridor wall
    pub fn corridor_column_color(&self) -> Color {
        match self.corridor_wall_color() {
            Color::Rgb(r, g, b) => Self::brighten(r, g, b, 10),
            other => other,
        }
    }
}
