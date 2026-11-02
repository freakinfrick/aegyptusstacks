use bfs_client::VolumeTyped;

/// Tallied physical-type composition of a room's corpus volumes.
/// Drives the biome stamping engine: what gets placed where.
#[derive(Clone, Debug, Default)]
pub struct RoomComposition {
    pub papyrus: u32,
    pub wall_inscription: u32,
    pub scroll: u32,
    pub false_door: u32,
    pub tomb: u32,
    pub vessel: u32,
    pub ostrakon: u32,
    pub stele: u32,
    pub sculpture: u32,
    pub coffin: u32,
    pub rock: u32,
    pub other: u32,
}

impl RoomComposition {
    /// Parse volumes_typed from BFS server into composition tallies.
    /// Maps physical_type labels (German + English + BFS server types) to categories.
    pub fn from_volumes_typed(volumes: &[VolumeTyped]) -> Self {
        let mut comp = RoomComposition::default();
        for vol in volumes {
            match vol.physical_type.as_str() {
                // Papyrus / writing surfaces → wall-mounted tablet display
                "Papyrus" | "papyrus"
                | "writing_tablet" | "writing_board" | "writing_medium" => comp.papyrus += 1,

                // Wall inscriptions
                "Wand" | "wall" | "wall_inscription" => comp.wall_inscription += 1,

                // Scrolls → floor pedestal
                "Schriftrolle" | "scroll" => comp.scroll += 1,

                // False doors / doorways
                "Scheintür" | "false_door" | "Scheintuer" | "doorway" => comp.false_door += 1,

                // Tomb / chamber types
                "Grab" | "tomb" | "chamber" => comp.tomb += 1,

                // Vessels / objects → floor display
                "Gefäß" | "vessel" | "Gefaess" | "object" => comp.vessel += 1,

                // Ostraka (note: BFS returns "ostracon" singular)
                "Ostrakon" | "ostrakon" | "ostracon" => comp.ostrakon += 1,

                // Stelae (various shapes)
                "Stele" | "stele" | "rounded_stele" | "rectangular_stele" => comp.stele += 1,

                // Sculpture
                "Skulptur" | "sculpture" => comp.sculpture += 1,

                // Coffin / sarcophagus
                "Sarg" | "coffin" => comp.coffin += 1,

                // Rock-cut / geological
                "Fels" | "rock" | "rock_cut" => comp.rock += 1,

                // Unknown, undetermined, or unmapped → treat as other (placed like papyrus)
                _ => comp.other += 1,
            }
        }
        comp
    }

    /// Total wall-mountable items (placed on R-1 ring).
    /// "other" types are treated as papyrus for placement purposes.
    pub fn wall_mountable(&self) -> u32 {
        self.papyrus + self.wall_inscription + self.false_door + self.tomb + self.other
    }

    /// Total floor features (placed on inner rings).
    pub fn floor_features(&self) -> u32 {
        self.scroll + self.vessel + self.stele
    }

    /// Total volume count represented.
    pub fn total(&self) -> u32 {
        self.papyrus + self.wall_inscription + self.scroll + self.false_door
            + self.tomb + self.vessel + self.ostrakon + self.stele
            + self.sculpture + self.coffin + self.rock + self.other
    }

    /// Whether this composition has any typed data at all.
    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }
}

/// Deterministic seed from room coordinates (for scatter placement).
pub fn room_seed(q: i32, r: i32, z: i32) -> u64 {
    (q.wrapping_mul(10007) as u64)
        .wrapping_add(r.wrapping_mul(997) as u64)
        .wrapping_add(z.wrapping_mul(31) as u64)
}

/// Simple linear congruential generator for deterministic placement.
pub struct RoomRng {
    state: u64,
}

impl RoomRng {
    pub fn new(seed: u64) -> Self {
        RoomRng { state: seed.wrapping_add(1) }
    }

    /// Next pseudo-random u64.
    pub fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.state
    }

    /// Random index in [0, max).
    pub fn next_usize(&mut self, max: usize) -> usize {
        if max == 0 { return 0; }
        (self.next() % max as u64) as usize
    }
}
