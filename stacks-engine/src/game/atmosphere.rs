//! Egyptian atmosphere: era names, district display names, and evocative text.

use super::composition::RoomComposition;

/// Map z-level to chronological era display string.
pub fn era_name_from_z(z: i32) -> &'static str {
    match z {
        0 => "The Surface",
        1 => "Cluster Index",
        2 => "District Overview",
        3 => "The Observatory",
        -1 => "Old Kingdom (c. 2686\u{2013}2181 BCE)",
        -2 => "First Intermediate (c. 2181\u{2013}2055 BCE)",
        -3 => "Middle Kingdom (c. 2055\u{2013}1650 BCE)",
        -4 => "Second Intermediate (c. 1650\u{2013}1550 BCE)",
        -5 => "New Kingdom (c. 1550\u{2013}1069 BCE)",
        -6 => "Late Period (c. 1069\u{2013}332 BCE)",
        _ => "Greco-Roman (c. 332 BCE\u{2013}395 CE)", // z <= -7
    }
}

/// Map district key to Egyptian display name with transliteration.
pub fn display_district(district: &str) -> &'static str {
    match district {
        "temple" => "Temple Precinct",
        "necropolis" => "Necropolis (\u{1E2B}r.t-n\u{1E6F}r)",
        "house_of_life" => "House of Life (pr-\u{A725}n\u{1E2B})",
        "admin" => "Administrative Quarter",
        "school" => "The School (\u{A725}.t n sb\u{A3})",
        "quarry" => "Quarry Expeditions",
        "royal" => "Royal Precinct",
        "domestic" => "Domestic Quarter",
        "observatory" => "The Observatory",
        "index" => "Cluster Index",
        "overview" => "District Overview",
        "unplaced" => "Unplaced Archives",
        _ => "Unknown District",
    }
}

/// Generate atmospheric entry text based on room metadata.
/// Priority order: first match wins.
pub fn atmospheric_text(room_type: &str, district: &str, volume_count: u32, z: i32) -> &'static str {
    if volume_count == 0 {
        return "Empty chamber. What was written here did not survive.";
    }
    if volume_count <= 5 {
        return "Bare walls. Few texts endure from this place.";
    }
    if district == "necropolis" {
        return "The false door faces west. Beyond it: silence.";
    }
    if district == "quarry" {
        return "Chisel marks on raw stone. Expedition graffiti.";
    }
    if district == "temple" && volume_count > 20 {
        return "Columns rise between the offering formulae.";
    }
    if district == "school" {
        return "Student exercises on broken pottery. The lessons continue.";
    }
    if district == "domestic" {
        return "Household objects. The daily life of the Nile.";
    }
    if room_type == "terminus" {
        return "A dead end. The knowledge gathered here is complete.";
    }
    if room_type == "rotunda" {
        return "A crossroads. Passages branch in many directions.";
    }
    if room_type == "vault" {
        return "Deep archive. The walls hold what time could not erase.";
    }
    if z <= -5 {
        return "These inscriptions predate most of what you have read.";
    }
    if volume_count >= 40 {
        return "A vast collection. The shelves are full.";
    }
    match room_type {
        "gallery" => "A gallery of texts. Walk among them.",
        "hallway" => "A passage between chambers.",
        "chamber" => "A quiet chamber.",
        _ => "You have entered a new room.",
    }
}

/// Derive room name from cluster signature lemmata.
/// The name IS the corpus — derived from the defining words of the texts within.
pub fn room_name_from_cluster(cluster_id: &str, cluster_signature: &[String], room_type: &str) -> String {
    if !cluster_signature.is_empty() {
        // Use top 2-3 signature lemmata as the room's identity
        let lemmata: Vec<&str> = cluster_signature.iter()
            .take(3)
            .map(|s| s.as_str())
            .collect();
        let joined = lemmata.join(" · ");
        format!("Chamber of {} ({})", joined, room_type)
    } else if !cluster_id.is_empty() && cluster_id != "?" {
        // Fall back to cluster_id parsing: "district_cluster_N" or "district_unclustered"
        let parts: Vec<&str> = cluster_id.split('_').collect();
        if parts.len() >= 2 && parts.last() == Some(&"unclustered") {
            format!("{} Archive ({})", capitalize(parts[0]), room_type)
        } else if parts.len() >= 3 {
            format!("{} Collection {} ({})", capitalize(parts[0]), parts.last().unwrap_or(&""), room_type)
        } else {
            format!("{} ({})", capitalize(cluster_id), room_type)
        }
    } else {
        format!("Unnamed {} ", room_type)
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

/// Build concise composition summary for status bar (top 3 types).
pub fn composition_summary(comp: &RoomComposition) -> String {
    let mut parts = Vec::new();
    if comp.papyrus > 0 { parts.push(format!("Papyri: {}", comp.papyrus)); }
    if comp.stele > 0 { parts.push(format!("Stelae: {}", comp.stele)); }
    if comp.scroll > 0 { parts.push(format!("Scrolls: {}", comp.scroll)); }
    if comp.coffin > 0 { parts.push(format!("Coffins: {}", comp.coffin)); }
    if comp.ostrakon > 0 { parts.push(format!("Ostraka: {}", comp.ostrakon)); }
    if comp.vessel > 0 { parts.push(format!("Vessels: {}", comp.vessel)); }
    if comp.wall_inscription > 0 { parts.push(format!("Inscriptions: {}", comp.wall_inscription)); }
    if comp.sculpture > 0 { parts.push(format!("Sculptures: {}", comp.sculpture)); }
    if comp.other > 0 { parts.push(format!("Other: {}", comp.other)); }
    parts.truncate(3);
    parts.join(", ")
}
