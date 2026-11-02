//! Room typology classification per BFS Architectural Procgen Spec §9.1
//!
//! Classifies galleries into architectural types based on door arrangement,
//! z-tier, and content signals.

use crate::render::palette::Tier;
use super::world::{GalleryLayout, WallState};

/// Architectural room type (Spec §9.1 + §10 anomalies)
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RoomType {
    /// Standard gallery — default type
    Gallery,
    /// Two opposite doors, ribbon continuity — long corridor feel
    Hallway,
    /// Two corner doors, loop cluster — intimate chamber
    Chamber,
    /// Index floor room (z > 0) — open, skylit
    Atrium,
    /// Index floor hub with express shafts — wide open rotunda
    Rotunda,
    /// Deep vault (z < -100) — reinforced, dim
    Vault,
    /// High-centrality landmark, single door — dead end
    Terminus,
    /// 3+ doors, exceptional branching point
    Junction,
    // --- Anomalies (Spec §10) ---
    /// Ground-truth / canonical reference room — all shelves curved, red palette
    CrimsonHexagon,
    /// Self-referencing content loop — all walls are mirrors, no shaft
    MirrorGallery,
}

impl RoomType {
    /// Classify a room from its layout, tier, and content signals.
    pub fn classify(
        layout: &GalleryLayout,
        tier: Tier,
        volume_count: u32,
        room_type_hint: &str,
    ) -> Self {
        // Check for anomalies from BFS metadata hints
        match room_type_hint {
            "crimson_hexagon" => return RoomType::CrimsonHexagon,
            "mirror_gallery" => return RoomType::MirrorGallery,
            _ => {}
        }

        let door_count = layout.walls.iter()
            .filter(|s| **s == WallState::FreeDoor)
            .count();

        // Junction: 3+ doors (exceptional)
        if door_count >= 3 {
            return RoomType::Junction;
        }

        // Terminus: single door, high-centrality landmark
        if door_count <= 1 {
            return RoomType::Terminus;
        }

        // Tier-based overrides (z > 0 = index floors, z <= -7 = vault)
        match tier {
            Tier::Surface => {
                // Could be z=0 or z>0. Use room_type_hint for atrium/rotunda.
            }
            Tier::GrecoRoman => return RoomType::Vault,
            _ => {}
        }

        // BFS room_type hints for index floors
        match room_type_hint {
            "atrium" => return RoomType::Atrium,
            "rotunda" => return RoomType::Rotunda,
            _ => {}
        }

        // Door arrangement analysis (2 doors)
        let door_indices: Vec<usize> = layout.walls.iter().enumerate()
            .filter(|(_, s)| **s == WallState::FreeDoor)
            .map(|(i, _)| i)
            .collect();

        if door_indices.len() == 2 {
            let diff = (door_indices[1] as i32 - door_indices[0] as i32).rem_euclid(6);
            match diff {
                3 => return RoomType::Hallway,       // opposite walls → ribbon
                2 | 4 => return RoomType::Chamber,   // corner walls → loop
                _ => {}
            }
        }

        RoomType::Gallery
    }

    /// Short display label for status bar
    pub fn label(&self) -> &'static str {
        match self {
            RoomType::Gallery => "Gallery",
            RoomType::Hallway => "Hallway",
            RoomType::Chamber => "Chamber",
            RoomType::Atrium => "Atrium",
            RoomType::Rotunda => "Rotunda",
            RoomType::Vault => "Vault",
            RoomType::Terminus => "Terminus",
            RoomType::Junction => "Junction",
            RoomType::CrimsonHexagon => "Crimson Hexagon",
            RoomType::MirrorGallery => "Mirror Gallery",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bfs_client::DoorEdge;

    fn layout_with_doors(dirs: &[&str]) -> GalleryLayout {
        let edges: Vec<DoorEdge> = dirs.iter().map(|d| DoorEdge {
            from_coord: [0,0,0],
            to_coord: [0,0,0],
            direction: d.to_string(),
        }).collect();
        GalleryLayout::from_door_edges(&edges)
    }

    #[test]
    fn test_hallway_opposite_doors() {
        let layout = layout_with_doors(&["E", "W"]); // walls 0 and 3 = opposite
        let ty = RoomType::classify(&layout, Tier::Surface, 50, "");
        assert_eq!(ty, RoomType::Hallway);
    }

    #[test]
    fn test_chamber_corner_doors() {
        let layout = layout_with_doors(&["E", "NW"]); // walls 0 and 2 = corner
        let ty = RoomType::classify(&layout, Tier::Surface, 50, "");
        assert_eq!(ty, RoomType::Chamber);
    }

    #[test]
    fn test_vault_deep_z() {
        let layout = layout_with_doors(&["E", "W"]);
        let ty = RoomType::classify(&layout, Tier::GrecoRoman, 50, "");
        assert_eq!(ty, RoomType::Vault);
    }

    #[test]
    fn test_atrium_from_hint() {
        let layout = layout_with_doors(&["E", "W"]);
        let ty = RoomType::classify(&layout, Tier::Surface, 10, "atrium");
        assert_eq!(ty, RoomType::Atrium);
    }

    #[test]
    fn test_terminus_single_door() {
        let layout = layout_with_doors(&["E"]);
        let ty = RoomType::classify(&layout, Tier::Surface, 100, "");
        assert_eq!(ty, RoomType::Terminus);
    }

    #[test]
    fn test_crimson_hexagon_hint() {
        let layout = layout_with_doors(&["E", "W"]);
        let ty = RoomType::classify(&layout, Tier::Surface, 50, "crimson_hexagon");
        assert_eq!(ty, RoomType::CrimsonHexagon);
    }
}
