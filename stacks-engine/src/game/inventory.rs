use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GatheredVolume {
    pub canonical_id: String,
    pub room_coord: [i32; 3],
    pub turn: u64,
    pub name: String,
    pub physical_type: String,
    pub district: String,
}

pub struct Inventory {
    pub volumes: Vec<GatheredVolume>,
}

impl Inventory {
    pub fn new() -> Self { Inventory { volumes: Vec::new() } }

    pub fn gather(
        &mut self,
        id: String,
        room: [i32; 3],
        turn: u64,
        name: String,
        physical_type: String,
        district: String,
    ) -> bool {
        if self.volumes.iter().any(|v| v.canonical_id == id) {
            return false;
        }
        self.volumes.push(GatheredVolume {
            canonical_id: id, room_coord: room, turn,
            name, physical_type, district,
        });
        true
    }

    pub fn clear(&mut self) { self.volumes.clear(); }

    pub fn ids(&self) -> Vec<String> {
        self.volumes.iter().map(|v| v.canonical_id.clone()).collect()
    }

    pub fn len(&self) -> usize { self.volumes.len() }

    pub fn is_empty(&self) -> bool { self.volumes.is_empty() }
}
