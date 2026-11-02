//! Async HTTP client for the BFS JSON-RPC server (port 8081)

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

// --- Error type ---

#[derive(Debug, thiserror::Error)]
pub enum BfsError {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("RPC error {code}: {message}")]
    Rpc { code: i64, message: String },
    #[error("not found: {0}")]
    NotFound(String),
    #[error("parse error: {0}")]
    Parse(String),
}

// --- JSON-RPC envelope types ---

#[derive(Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    method: String,
    params: serde_json::Value,
    id: u64,
}

#[derive(Deserialize)]
struct JsonRpcResponse {
    result: Option<serde_json::Value>,
    error: Option<RpcError>,
}

#[derive(Deserialize)]
struct RpcError {
    code: i64,
    message: String,
}

// --- Response types matching ACTUAL BFS server responses ---

/// A door connecting two coordinates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoorEdge {
    pub from_coord: [i32; 3],
    pub to_coord: [i32; 3],
    pub direction: String,
}

/// A wall edge between coordinates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallEdge {
    pub from_coord: [i32; 3],
    pub to_coord: [i32; 3],
    pub proximity_score: f64,
}

/// A shaft edge (vertical connection)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShaftEdge {
    pub from_coord: [i32; 3],
    pub to_coord: [i32; 3],
    pub direction: String,
}

/// A volume with its physical type classification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeTyped {
    pub id: String,
    pub physical_type: String,
}

/// Full metadata for a BFS room (matches actual server response).
/// Hub rooms (rotundas, atria) return a leaner schema — missing fields get defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomMetadata {
    pub room_key: String,
    pub q: i32,
    pub r: i32,
    pub z: i32,
    pub room_type: String,
    pub district: String,
    #[serde(default)]
    pub volume_count: u32,
    #[serde(default)]
    pub door_count: u32,
    #[serde(default)]
    pub palette: String,
    #[serde(default)]
    pub z_band: i32,
    /// Shelves: each shelf is a list of canonical IDs
    #[serde(default)]
    pub shelves: Vec<Vec<String>>,
    /// Flat list of all volume canonical IDs in this room
    #[serde(default)]
    pub volumes: Vec<String>,
    /// Typed volumes with physical_type classification (from BFS server)
    #[serde(default)]
    pub volumes_typed: Vec<VolumeTyped>,
    #[serde(default)]
    pub door_edges: Vec<DoorEdge>,
    #[serde(default)]
    pub wall_edges: Vec<WallEdge>,
    #[serde(default)]
    pub shaft_edges: Vec<ShaftEdge>,
    /// Dominant physical type per shelf (e.g. ["unknown", "scroll", "stele", "object"])
    #[serde(default)]
    pub shelf_types: Vec<String>,
    /// Cluster identity for room naming (e.g. "necropolis_cluster_4")
    #[serde(default)]
    pub cluster_id: String,
    /// Signature lemmata — Egyptian transliterations that define this cluster's topic
    #[serde(default)]
    pub cluster_signature: Vec<String>,
}

/// A single sentence record from a volume
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentenceRecord {
    pub id: i64,
    pub canonical_id: String,
    pub sequence_num: i32,
    pub transliteration: Option<String>,
    pub translation: HashMap<String, String>,
}

/// Full content of a volume
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeContent {
    pub canonical_id: String,
    pub conventional_name: Option<String>,
    pub sentences: Vec<SentenceRecord>,
    #[serde(default)]
    pub sentence_count: u32,
    #[serde(default)]
    pub has_transliteration: bool,
    #[serde(default)]
    pub has_translation: bool,
    #[serde(default)]
    pub translation_languages: Vec<String>,
}

/// A search result hit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub canonical_id: String,
    pub conventional_name: Option<String>,
    pub score: f64,
    pub snippet: Option<String>,
    pub coord: Option<[i32; 3]>,
    pub district: Option<String>,
    pub z_band: Option<i32>,
}

// --- Client ---

/// BFS JSON-RPC client (Arc-wrapped internals for cheap cloning)
#[derive(Clone)]
pub struct BfsClient {
    pub base_url: String,
    client: reqwest::Client,
    request_id: Arc<AtomicU64>,
}

impl BfsClient {
    /// Create a new client pointing at the given base URL
    pub fn new(base_url: &str) -> Self {
        BfsClient {
            base_url: base_url.to_string(),
            client: reqwest::Client::new(),
            request_id: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Send a JSON-RPC call and deserialize the result
    async fn rpc_call<T: DeserializeOwned>(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<T, BfsError> {
        let id = self.request_id.fetch_add(1, Ordering::Relaxed);
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            method: method.to_string(),
            params,
            id,
        };

        let url = format!("{}/rpc", self.base_url);
        let response: JsonRpcResponse = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await?
            .json()
            .await?;

        if let Some(err) = response.error {
            return Err(BfsError::Rpc {
                code: err.code,
                message: err.message,
            });
        }

        match response.result {
            Some(value) => {
                serde_json::from_value(value).map_err(|e| BfsError::Parse(e.to_string()))
            }
            None => Err(BfsError::Parse("null result without error".to_string())),
        }
    }

    /// Fetch room metadata by hex coordinates
    pub async fn get_room(&self, q: i32, r: i32, z: i32) -> Result<RoomMetadata, BfsError> {
        self.rpc_call("get_room", serde_json::json!({"q": q, "r": r, "z": z})).await
    }

    /// Batch-fetch room metadata for multiple coordinates.
    /// Returns a Vec in the same order as input; None for coords with no room.
    /// Falls back to individual get_room calls if server lacks batch method.
    pub async fn get_rooms(&self, coords: &[[i32; 3]]) -> Result<Vec<Option<RoomMetadata>>, BfsError> {
        if coords.is_empty() {
            return Ok(Vec::new());
        }
        let coords_json: Vec<serde_json::Value> = coords
            .iter()
            .map(|c| serde_json::json!([c[0], c[1], c[2]]))
            .collect();
        let result: Result<Vec<Option<RoomMetadata>>, BfsError> =
            self.rpc_call("get_rooms", serde_json::json!({"coords": coords_json})).await;

        match result {
            Ok(v) => Ok(v),
            Err(BfsError::Rpc { code: -32601, .. }) => {
                // Method not found — fall back to individual calls
                self.get_rooms_sequential(coords).await
            }
            Err(e) => Err(e),
        }
    }

    /// Fallback: fetch rooms one at a time (for servers without batch support).
    async fn get_rooms_sequential(&self, coords: &[[i32; 3]]) -> Result<Vec<Option<RoomMetadata>>, BfsError> {
        let mut results = Vec::with_capacity(coords.len());
        for c in coords {
            match self.get_room(c[0], c[1], c[2]).await {
                Ok(meta) => results.push(Some(meta)),
                Err(BfsError::Rpc { .. }) => results.push(None),
                Err(e) => return Err(e),
            }
        }
        Ok(results)
    }

    /// Fetch full volume content by canonical ID
    pub async fn get_volume(&self, canonical_id: &str) -> Result<VolumeContent, BfsError> {
        self.rpc_call("get_volume", serde_json::json!({"canonical_id": canonical_id})).await
    }

    /// Get neighboring door edges from a coordinate
    pub async fn get_neighbors(&self, q: i32, r: i32, z: i32) -> Result<Vec<DoorEdge>, BfsError> {
        self.rpc_call("neighbors", serde_json::json!({"q": q, "r": r, "z": z})).await
    }

    /// Search the corpus
    pub async fn search(&self, query: &str, limit: u32) -> Result<Vec<SearchHit>, BfsError> {
        self.rpc_call("search", serde_json::json!({"query": query, "limit": limit})).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn test_get_room() {
        let client = BfsClient::new("http://localhost:8081");
        let room = client.get_room(1, -1, -3).await.unwrap();
        assert_eq!(room.q, 1);
        assert_eq!(room.r, -1);
        assert_eq!(room.z, -3);
        println!("room_key={} type={} district={} volumes={} doors={}",
            room.room_key, room.room_type, room.district, room.volume_count, room.door_edges.len());
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_volume() {
        let client = BfsClient::new("http://localhost:8081");
        let vol = client.get_volume("aeg_oraec_774").await.unwrap();
        assert!(!vol.sentences.is_empty());
        println!("volume={} sentences={}", vol.canonical_id, vol.sentences.len());
    }
}
