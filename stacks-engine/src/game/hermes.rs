//! Hermes agent bridge — FIFO-based JSON-RPC communication with the Librarian.

use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::sync::mpsc;
use serde::Deserialize;

use super::engine::Engine;
use super::world::WorldMap;

// ── Constants ──────────────────────────────────────────────────────────

const FIFO_IN: &str = "/tmp/stacks-hermes.in";
const FIFO_OUT: &str = "/tmp/stacks-hermes.out";
const TMUX_SESSION: &str = "stacks-hermes";
const GATEWAY_TIMEOUT_SECS: u64 = 120;

type BridgeResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

// ── Core Types ─────────────────────────────────────────────────────────

pub struct HermesBridge {
    fifo_writer: BufWriter<tokio::fs::File>,
    reader_rx: mpsc::UnboundedReceiver<HermesEvent>,
    session_id: Option<String>,
    request_id: AtomicU64,
    pub status: BridgeStatus,
    tmux_session: String,
}

#[derive(Debug)]
pub enum BridgeStatus {
    Spawning,
    Ready,
    Disconnected,
    Error(String),
}

#[derive(Debug)]
pub enum HermesEvent {
    GatewayReady,
    SessionCreated { session_id: String },
    SessionInfo { model: String, context_max: u64 },
    MessageStart,
    Delta { text: String },
    Complete { text: String, status: String, context_used: u64, context_max: u64 },
    StatusUpdate { text: String },
    ToolStart { name: String, context: String },
    ToolComplete { name: String, preview: String },
    ToolProgress { name: String, preview: String },
    ThinkingDelta { text: String },
    Error { message: String },
    Response { id: u64, result: serde_json::Value },
}

/// Summary of a prior session returned by session.list.
#[derive(Clone, Debug)]
pub struct SessionSummary {
    pub session_id: String,
    pub title: String,
    pub started_at: String,
    pub message_count: u64,
}

#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub text: String,
    /// True if this message contains reasoning/thinking text (rendered differently).
    pub is_thinking: bool,
}

impl ChatMessage {
    pub fn new(role: ChatRole, text: String) -> Self {
        ChatMessage { role, text, is_thinking: false }
    }
    pub fn thinking(role: ChatRole, text: String) -> Self {
        ChatMessage { role, text, is_thinking: true }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ChatRole {
    User,
    Librarian,
    System,
}

#[derive(Clone, Debug)]
pub enum ChatStatus {
    Idle,
    Streaming,
    WaitingForAgent,
    Error(String),
}

// ── Context Header ─────────────────────────────────────────────────────

pub struct ContextHeader {
    pub location: String,
    pub coord: String,
    pub shelves: String,
    pub doors: String,
    pub shafts: String,
    pub session: String,
    pub inventory: String,
}

impl ContextHeader {
    /// Build context header from current engine + world state.
    /// Target: ~200 tokens prepended to every prompt.submit.
    pub fn build(engine: &Engine, world: &WorldMap, interaction_count: u32, inventory_ids: &[String]) -> Self {
        let coord = engine.current_room;
        let z = coord[2];

        if let Some(room) = world.rooms.get(&coord) {
            let meta = &room.info;

            // Room name: cluster_id > cluster_signature > fallback
            let name = if !meta.cluster_id.is_empty() {
                meta.cluster_id.clone()
            } else if !meta.cluster_signature.is_empty() {
                meta.cluster_signature.join(", ")
            } else {
                format!("room_{}_{}_{}", coord[0], coord[1], coord[2])
            };

            // Shelves: count + volume IDs so the Librarian can reference texts directly
            let shelves = if let Some(shelf_data) = world.shelves.get(&coord) {
                let n = shelf_data.len();
                let all_ids: Vec<&str> = shelf_data.iter()
                    .flat_map(|s| s.iter().map(|id| id.as_str()))
                    .collect();
                let vol_count = all_ids.len();
                if vol_count > 0 {
                    // Show all IDs (up to 30), truncate for very large rooms
                    let shown: Vec<&str> = all_ids.iter().take(30).copied().collect();
                    let id_list = shown.join(", ");
                    let suffix = if vol_count > 30 {
                        format!(" (+{} more)", vol_count - 30)
                    } else {
                        String::new()
                    };
                    format!("{} shelves, {} volumes: {}{}", n, vol_count, id_list, suffix)
                } else {
                    "no shelves".to_string()
                }
            } else {
                "no shelves".to_string()
            };

            // Doors: horizontal edges with destination names
            let doors: Vec<String> = meta
                .door_edges
                .iter()
                .filter(|e| e.direction != "UP" && e.direction != "DOWN")
                .map(|e| {
                    let dest_name = world
                        .rooms
                        .get(&e.to_coord)
                        .map(|r| {
                            if !r.info.cluster_id.is_empty() {
                                r.info.cluster_id.clone()
                            } else {
                                r.info.room_type.clone()
                            }
                        })
                        .unwrap_or_else(|| {
                            format!("{},{},{}", e.to_coord[0], e.to_coord[1], e.to_coord[2])
                        });
                    format!("{} \u{2192} \"{}\"", e.direction, dest_name)
                })
                .collect();
            let doors_str = if doors.is_empty() {
                "none".to_string()
            } else {
                doors.join(", ")
            };

            // Shafts: vertical connections from shaft_edges + UP/DOWN door_edges
            let mut all_shafts: Vec<String> = meta
                .shaft_edges
                .iter()
                .map(|e| format!("{} \u{2192} \"z={}\"", e.direction, e.to_coord[2]))
                .collect();
            let shaft_doors: Vec<String> = meta
                .door_edges
                .iter()
                .filter(|e| e.direction == "UP" || e.direction == "DOWN")
                .map(|e| format!("{} \u{2192} \"z={}\"", e.direction, e.to_coord[2]))
                .collect();
            all_shafts.extend(shaft_doors);
            let shafts_str = if all_shafts.is_empty() {
                "none".to_string()
            } else {
                all_shafts.join(", ")
            };

            let rooms_visited = world.rooms.len() as u32;

            let inventory = format_inventory(inventory_ids);

            ContextHeader {
                location: format!(
                    "{} / {} / z={} / room \"{}\"",
                    meta.district, meta.room_type, z, name
                ),
                coord: format!("{},{},{}", coord[0], coord[1], coord[2]),
                shelves,
                doors: doors_str,
                shafts: shafts_str,
                session: format!(
                    "{} rooms visited, interaction {}",
                    rooms_visited, interaction_count
                ),
                inventory,
            }
        } else {
            let inventory = format_inventory(inventory_ids);

            ContextHeader {
                location: format!("unknown / unknown / z={} / room \"unknown\"", z),
                coord: format!("{},{},{}", coord[0], coord[1], coord[2]),
                shelves: "no shelves".to_string(),
                doors: "none".to_string(),
                shafts: "none".to_string(),
                session: format!(
                    "{} rooms visited, interaction {}",
                    world.rooms.len(),
                    interaction_count
                ),
                inventory,
            }
        }
    }

    /// Format as multi-line header for prepending to user message.
    pub fn format(&self) -> String {
        format!(
            "[LOCATION: {}]\n[COORD: {}]\n[SHELVES: {}]\n[DOORS: {}]\n[SHAFTS: {}]\n[SESSION: {}]\n[INVENTORY: {}]\n[COMMANDS: where TEXT (locate room) | grep PATTERN --field transliteration|translation|name|token --limit N | search QUERY | room-texts Q,R,Z | navigate Q,R,Z | gather ID1 ID2 | read ID]",
            self.location, self.coord, self.shelves, self.doors, self.shafts, self.session, self.inventory
        )
    }
}

// ── HermesBridge Implementation ────────────────────────────────────────

impl HermesBridge {
    /// Create FIFOs, spawn tmux session with shim, start FIFO reader task.
    pub async fn spawn() -> BridgeResult<Self> {
        // 1. Kill any stale tmux session + processes holding FIFOs
        let _ = tokio::process::Command::new("tmux")
            .args(["kill-session", "-t", TMUX_SESSION])
            .output()
            .await;
        // Kill ANY process holding FIFOs — catches orphaned debug sessions,
        // stale shims, etc. that tmux kill-session won't reach.
        let _ = tokio::process::Command::new("fuser")
            .args(["-k", FIFO_IN, FIFO_OUT])
            .stderr(std::process::Stdio::null())
            .output()
            .await;
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let _ = tokio::fs::remove_file(FIFO_IN).await;
        let _ = tokio::fs::remove_file(FIFO_OUT).await;

        // 2. Create named FIFOs
        let s = tokio::process::Command::new("mkfifo")
            .arg(FIFO_IN)
            .status()
            .await?;
        if !s.success() {
            return Err(format!("mkfifo {} failed: {}", FIFO_IN, s).into());
        }
        let s = tokio::process::Command::new("mkfifo")
            .arg(FIFO_OUT)
            .status()
            .await?;
        if !s.success() {
            return Err(format!("mkfifo {} failed: {}", FIFO_OUT, s).into());
        }

        // 3. Spawn tmux session running the shim
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let python = "python3".to_string();
        let shim = format!(
            "{}/librarian/stacks-hermes-shim.py",
            std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_else(|_| ".".into())
        );

        // Verify shim + python exist before spawning
        if !std::path::Path::new(&python).exists() {
            return Err(format!("Python not found: {}", python).into());
        }
        if !std::path::Path::new(&shim).exists() {
            return Err(format!("Shim not found: {}", shim).into());
        }

        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| ".".to_string());

        // Use stacks profile for tool/skill restrictions + model config
        let hermes_home = format!("{}/.hermes/profiles/stacks", home);
        let shell_cmd = format!(
            "HERMES_HOME={} HERMES_GATEWAY_SESSION=1 HERMES_EXEC_ASK=1 TERMINAL_CWD={} {} {}",
            hermes_home, cwd, python, shim
        );

        let tmux_status = tokio::process::Command::new("tmux")
            .args(["new-session", "-d", "-s", TMUX_SESSION, &shell_cmd])
            .status()
            .await?;
        if !tmux_status.success() {
            return Err(format!("tmux new-session failed: {}", tmux_status).into());
        }

        // 4. Open FIFOs — output FIRST (reading), then input (writing).
        //    The shim opens output-FIFO for writing first (gateway.ready),
        //    so we must open it for reading first to unblock the shim.
        //
        //    IMPORTANT: Must use blocking std::fs::File::open (via spawn_blocking),
        //    NOT tokio::fs::File::open. Tokio opens FIFOs in O_NONBLOCK mode,
        //    which on a read-side FIFO returns immediately with an fd that
        //    yields instant EOF if no writer has connected yet. Blocking open
        //    correctly waits until the writer (shim) connects.
        let fifo_timeout = std::time::Duration::from_secs(GATEWAY_TIMEOUT_SECS);

        let fifo_out_path = FIFO_OUT.to_string();
        let std_reader = tokio::time::timeout(
            fifo_timeout,
            tokio::task::spawn_blocking(move || std::fs::File::open(&fifo_out_path)),
        )
        .await
        .map_err(|_| -> Box<dyn std::error::Error + Send + Sync> {
            "Timed out waiting for Librarian gateway to start (FIFO open)".into()
        })??
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
            format!("Failed to open output FIFO: {}", e).into()
        })?;
        let fifo_reader = tokio::fs::File::from_std(std_reader);

        let fifo_in_path = FIFO_IN.to_string();
        let std_writer = tokio::time::timeout(
            fifo_timeout,
            tokio::task::spawn_blocking(move || {
                std::fs::OpenOptions::new().write(true).open(&fifo_in_path)
            }),
        )
        .await
        .map_err(|_| -> Box<dyn std::error::Error + Send + Sync> {
            "Timed out opening input FIFO".into()
        })??
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
            format!("Failed to open input FIFO: {}", e).into()
        })?;
        let fifo_writer = BufWriter::new(tokio::fs::File::from_std(std_writer));

        // 5. Spawn reader task
        let (tx, mut rx) = mpsc::unbounded_channel();
        tokio::spawn(reader_task(fifo_reader, tx));

        // 6. Wait for GatewayReady with timeout
        let mut status = BridgeStatus::Spawning;
        let deadline =
            tokio::time::Instant::now() + std::time::Duration::from_secs(GATEWAY_TIMEOUT_SECS);
        loop {
            match tokio::time::timeout_at(deadline, rx.recv()).await {
                Ok(Some(HermesEvent::GatewayReady)) => {
                    status = BridgeStatus::Ready;
                    break;
                }
                Ok(Some(HermesEvent::Error { message })) => {
                    status = BridgeStatus::Error(message);
                    break;
                }
                Ok(None) => {
                    status =
                        BridgeStatus::Error("Channel closed before gateway ready".to_string());
                    break;
                }
                Err(_) => {
                    status = BridgeStatus::Error("Librarian unavailable (timeout)".to_string());
                    break;
                }
                _ => continue,
            }
        }

        // Fail fast if gateway never became ready — surface the real error
        // instead of letting create_session hit a broken pipe.
        if let BridgeStatus::Error(ref msg) = status {
            // Read shim log for post-mortem (survives tmux session death)
            let shim_log = tokio::fs::read_to_string("/tmp/stacks-hermes-shim.log")
                .await
                .unwrap_or_default();
            let last_lines: String = shim_log
                .lines()
                .rev()
                .take(5)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join(" | ");
            let detail = if last_lines.is_empty() {
                format!("{} (no shim log)", msg)
            } else {
                format!("{} [shim: {}]", msg, last_lines)
            };
            eprintln!("[stacks] bridge spawn failed: {}", detail);
            return Err(detail.into());
        }

        Ok(HermesBridge {
            fifo_writer,
            reader_rx: rx,
            session_id: None,
            request_id: AtomicU64::new(1),
            status,
            tmux_session: TMUX_SESSION.to_string(),
        })
    }

    /// Send session.create, inject system prompt, store session_id.
    pub async fn create_session(&mut self, cols: u16) -> BridgeResult<()> {
        let id = self
            .write_request("session.create", serde_json::json!({ "cols": cols }))
            .await?;

        // Wait for matching response
        loop {
            match self.reader_rx.recv().await {
                Some(HermesEvent::Response { id: rid, result }) if rid == id => {
                    if let Some(sid) = result.get("session_id").and_then(|v| v.as_str()) {
                        self.session_id = Some(sid.to_string());
                    }
                    self.status = BridgeStatus::Ready;
                    return Ok(());
                }
                Some(HermesEvent::Error { message }) => {
                    return Err(message.into());
                }
                None => return Err("Channel closed".into()),
                _ => continue,
            }
        }
    }

    /// Build context header + send prompt.submit.
    pub async fn send_message(
        &mut self,
        user_text: &str,
        context: &ContextHeader,
    ) -> BridgeResult<()> {
        let sid = self.session_id.clone().ok_or("No active session")?;
        let full_text = format!("{}\n\nUser says: {}", context.format(), user_text);
        self.write_request(
            "prompt.submit",
            serde_json::json!({
                "session_id": sid,
                "text": full_text,
            }),
        )
        .await?;
        Ok(())
    }

    /// Non-blocking poll for next event from output FIFO.
    pub fn try_recv(&mut self) -> Option<HermesEvent> {
        self.reader_rx.try_recv().ok()
    }

    /// Send session.compress when token budget exceeded.
    pub async fn compress(&mut self) -> BridgeResult<()> {
        let sid = self.session_id.clone().ok_or("No active session")?;
        self.write_request("session.compress", serde_json::json!({ "session_id": sid }))
            .await?;
        Ok(())
    }

    /// Fetch list of prior sessions from hermes SQLite store.
    pub async fn list_sessions(&mut self) -> BridgeResult<Vec<SessionSummary>> {
        let id = self.write_request("session.list", serde_json::json!({})).await?;
        loop {
            match self.reader_rx.recv().await {
                Some(HermesEvent::Response { id: rid, result }) if rid == id => {
                    let sessions = result.get("sessions")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter().filter_map(|s| {
                                Some(SessionSummary {
                                    session_id: s.get("session_key")
                                        .or_else(|| s.get("id"))
                                        .and_then(|v| v.as_str())?.to_string(),
                                    title: s.get("title")
                                        .and_then(|v| v.as_str())
                                        .filter(|t| !t.is_empty())
                                        .unwrap_or("(untitled)")
                                        .to_string(),
                                    started_at: s.get("started_at")
                                        .map(|v| {
                                            if let Some(s) = v.as_str() { s.to_string() }
                                            else { v.to_string() }
                                        })
                                        .unwrap_or_default(),
                                    message_count: s.get("message_count")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0),
                                })
                            }).collect()
                        })
                        .unwrap_or_default();
                    return Ok(sessions);
                }
                Some(HermesEvent::Error { message }) => return Err(message.into()),
                None => return Err("Channel closed".into()),
                _ => continue,
            }
        }
    }

    /// Resume a prior session — returns (chat messages, model name, context_max) for UI.
    pub async fn resume_session(&mut self, session_id: &str) -> BridgeResult<(Vec<ChatMessage>, Option<String>, u64)> {
        // Close current session if active
        if let Some(ref sid) = self.session_id {
            let _ = self.write_request("session.close", serde_json::json!({ "session_id": sid })).await;
        }

        let id = self.write_request(
            "session.resume",
            serde_json::json!({ "session_id": session_id }),
        ).await?;

        loop {
            match self.reader_rx.recv().await {
                Some(HermesEvent::Response { id: rid, result }) if rid == id => {
                    // Store new session id
                    if let Some(sid) = result.get("session_id").and_then(|v| v.as_str()) {
                        self.session_id = Some(sid.to_string());
                    }
                    // Extract model info from response
                    let info = result.get("info");
                    let model = info
                        .and_then(|i| i.get("model"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let context_max = info
                        .and_then(|i| i.get("usage"))
                        .and_then(|u| u.get("context_max"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    // Parse messages array into ChatMessages
                    let messages = result.get("messages")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter().filter_map(|m| {
                                let role_str = m.get("role")?.as_str()?;
                                let role = match role_str {
                                    "user" => ChatRole::User,
                                    "assistant" => ChatRole::Librarian,
                                    _ => return None, // skip tool messages
                                };
                                let raw = m.get("text")
                                    .or_else(|| m.get("content"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                if raw.is_empty() { return None; }
                                // Strip context header from user messages
                                let text = if role == ChatRole::User {
                                    if let Some(pos) = raw.find("User says: ") {
                                        raw[pos + 11..].to_string()
                                    } else {
                                        raw.to_string()
                                    }
                                } else {
                                    raw.to_string()
                                };
                                if text.is_empty() { return None; }
                                Some(ChatMessage::new(role, text))
                            }).collect()
                        })
                        .unwrap_or_default();
                    return Ok((messages, model, context_max));
                }
                Some(HermesEvent::Error { message }) => return Err(message.into()),
                None => return Err("Channel closed".into()),
                _ => continue,
            }
        }
    }

    /// Close session, kill tmux, remove FIFOs.
    pub async fn close(&mut self) {
        if let Some(sid) = self.session_id.take() {
            let _ = self
                .write_request("session.close", serde_json::json!({ "session_id": sid }))
                .await;
            // Let hermes persist state before killing tmux
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        let _ = tokio::process::Command::new("tmux")
            .args(["kill-session", "-t", &self.tmux_session])
            .status()
            .await;

        // Kill any lingering processes holding FIFOs (shim may outlive tmux kill)
        let _ = tokio::process::Command::new("fuser")
            .args(["-k", FIFO_IN, FIFO_OUT])
            .stderr(std::process::Stdio::null())
            .output()
            .await;

        let _ = tokio::fs::remove_file(FIFO_IN).await;
        let _ = tokio::fs::remove_file(FIFO_OUT).await;

        self.status = BridgeStatus::Disconnected;
    }

    /// Serialize JSON-RPC request, write to FIFO, flush. Returns request id.
    async fn write_request(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> BridgeResult<u64> {
        let id = self.request_id.fetch_add(1, Ordering::Relaxed);
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let mut line = serde_json::to_string(&request)?;
        line.push('\n');
        self.fifo_writer.write_all(line.as_bytes()).await?;
        self.fifo_writer.flush().await?;
        Ok(id)
    }
}

// ── Reader Task ────────────────────────────────────────────────────────

/// Background task: reads newline-delimited JSON from output FIFO,
/// parses into HermesEvent, sends on mpsc channel. On EOF sends disconnect.
async fn reader_task(file: tokio::fs::File, tx: mpsc::UnboundedSender<HermesEvent>) {
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                if line.trim().is_empty() {
                    continue;
                }
                let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line) else {
                    continue;
                };
                if let Some(event) = parse_json_rpc_event(&msg) {
                    if tx.send(event).is_err() {
                        break; // receiver dropped
                    }
                }
            }
            Ok(None) => {
                // EOF — shim closed the write end of the FIFO
                let _ = tx.send(HermesEvent::Error {
                    message: "Librarian gateway disconnected (EOF)".to_string(),
                });
                break;
            }
            Err(e) => {
                // I/O error on the FIFO
                let _ = tx.send(HermesEvent::Error {
                    message: format!("Librarian gateway read error: {}", e),
                });
                break;
            }
        }
    }
}

/// Map a parsed JSON-RPC message to a HermesEvent.
fn parse_json_rpc_event(msg: &serde_json::Value) -> Option<HermesEvent> {
    // Response (has "id") — reply to a request we sent
    if let Some(id) = msg.get("id").and_then(|v| v.as_u64()) {
        if let Some(err) = msg.get("error") {
            let message = err
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error")
                .to_string();
            return Some(HermesEvent::Error { message });
        }
        let result = msg
            .get("result")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        return Some(HermesEvent::Response { id, result });
    }

    // Event notification (no "id", method = "event")
    let method = msg.get("method")?.as_str()?;
    if method != "event" {
        return None;
    }

    let params = msg.get("params")?;
    let event_type = params.get("type")?.as_str()?;
    let payload = params.get("payload");

    match event_type {
        "gateway.ready" => Some(HermesEvent::GatewayReady),
        "session.info" => {
            let model = payload?
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let context_max = payload?
                .get("usage")
                .and_then(|u| u.get("context_max"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            Some(HermesEvent::SessionInfo { model, context_max })
        }
        "message.start" => Some(HermesEvent::MessageStart),
        "message.delta" => {
            let text = payload?.get("text")?.as_str()?.to_string();
            Some(HermesEvent::Delta { text })
        }
        "message.complete" => {
            let text = payload?.get("text")?.as_str()?.to_string();
            let status = payload?
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("complete")
                .to_string();
            let usage = payload?.get("usage");
            // Hermes usage keys: "context_used", "context_max" (not prompt_tokens)
            let context_used = usage
                .and_then(|u| u.get("context_used"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let context_max = usage
                .and_then(|u| u.get("context_max"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            Some(HermesEvent::Complete { text, status, context_used, context_max })
        }
        "status.update" => {
            let text = payload?.get("text")?.as_str()?.to_string();
            Some(HermesEvent::StatusUpdate { text })
        }
        "tool.start" => {
            let name = payload?.get("name").and_then(|v| v.as_str()).unwrap_or("tool").to_string();
            let context = payload?.get("context").and_then(|v| v.as_str()).unwrap_or("").to_string();
            Some(HermesEvent::ToolStart { name, context })
        }
        "tool.complete" => {
            let name = payload?.get("name").and_then(|v| v.as_str()).unwrap_or("tool").to_string();
            let preview = payload?.get("preview").and_then(|v| v.as_str()).unwrap_or("").to_string();
            Some(HermesEvent::ToolComplete { name, preview })
        }
        "tool.progress" | "tool.generating" => {
            let name = payload?.get("name").and_then(|v| v.as_str()).unwrap_or("tool").to_string();
            let preview = payload?.get("preview").and_then(|v| v.as_str()).unwrap_or("").to_string();
            Some(HermesEvent::ToolProgress { name, preview })
        }
        "thinking.delta" | "reasoning.delta" => {
            let text = payload?.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if text.is_empty() { None } else { Some(HermesEvent::ThinkingDelta { text }) }
        }
        "error" => {
            let message = payload?.get("message")?.as_str()?.to_string();
            Some(HermesEvent::Error { message })
        }
        _ => None,
    }
}

// ── Game Commands ──────────────────────────────────────────────────────

/// Commands the Librarian can embed in responses as fenced JSON blocks.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum GameCommand {
    #[serde(rename = "navigate")]
    Navigate { coord: [i32; 3] },
    #[serde(rename = "read")]
    Read { id: String },
    #[serde(rename = "gather")]
    Gather { id: String },
    /// Batch gather — used by stacks-game.py socket protocol only.
    #[serde(rename = "gather_batch")]
    GatherBatch { ids: Vec<String> },
    #[serde(rename = "scribe")]
    Scribe {
        volumes: Vec<String>,
        #[serde(default)]
        topic: String,
    },
}

/// Format inventory IDs for the context header line.
fn format_inventory(ids: &[String]) -> String {
    if ids.is_empty() {
        return "empty".to_string();
    }
    let n = ids.len();
    if n <= 8 {
        format!("{} gathered volumes \u{2014} {}", n, ids.join(", "))
    } else {
        let shown: Vec<&str> = ids.iter().take(2).map(|s| s.as_str()).collect();
        format!("{} gathered volumes \u{2014} {} (+{} more)", n, shown.join(", "), n - 2)
    }
}

/// Scan response text for game commands in fenced code blocks.
///
/// Supported formats:
/// - ```json blocks with GameCommand JSON
/// - ```bash blocks containing `stacks-game.py` invocations
/// - Bare lines containing `stacks-game.py` (unfenced)
pub fn parse_game_commands(response: &str) -> Vec<GameCommand> {
    let mut commands = Vec::new();
    let mut in_json = false;
    let mut in_bash = false;
    let mut json_lines: Vec<String> = Vec::new();

    for line in response.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```json") {
            in_json = true;
            json_lines.clear();
        } else if trimmed.starts_with("```bash") || trimmed.starts_with("```sh") {
            in_bash = true;
        } else if trimmed == "```" && in_json {
            in_json = false;
            let json_str = json_lines.join("\n");
            if let Ok(cmd) = serde_json::from_str::<GameCommand>(&json_str) {
                commands.push(cmd);
            }
        } else if trimmed == "```" && in_bash {
            in_bash = false;
        } else if in_json {
            json_lines.push(line.to_string());
        } else if in_bash || !in_json {
            // Parse stacks-game.py commands from bash blocks or bare lines
            if let Some(cmd) = parse_bash_game_command(trimmed) {
                commands.push(cmd);
            }
        }
    }
    commands
}

/// Parse a single `stacks-game.py` invocation from a bash line.
///
/// Formats:
///   python3 .../stacks-game.py navigate 3,-16,-1
///   python3 .../stacks-game.py read aeg_oraec_27
///   python3 .../stacks-game.py gather id1 id2 id3
///   python3 .../stacks-game.py scribe "topic" id1 id2
fn parse_bash_game_command(line: &str) -> Option<GameCommand> {
    // Find "stacks-game.py" and extract everything after it
    let marker = "stacks-game.py";
    let idx = line.find(marker)?;
    let after = line[idx + marker.len()..].trim();
    let parts: Vec<&str> = after.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    match parts[0] {
        "navigate" if parts.len() >= 2 => {
            let coord_parts: Vec<&str> = parts[1].split(',').collect();
            if coord_parts.len() == 3 {
                let q = coord_parts[0].parse::<i32>().ok()?;
                let r = coord_parts[1].parse::<i32>().ok()?;
                let z = coord_parts[2].parse::<i32>().ok()?;
                Some(GameCommand::Navigate { coord: [q, r, z] })
            } else {
                None
            }
        }
        "read" if parts.len() >= 2 => {
            Some(GameCommand::Read { id: parts[1].to_string() })
        }
        "gather" if parts.len() >= 2 => {
            let ids: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
            if ids.len() == 1 {
                Some(GameCommand::Gather { id: ids.into_iter().next().unwrap() })
            } else {
                Some(GameCommand::GatherBatch { ids })
            }
        }
        "scribe" if parts.len() >= 3 => {
            // scribe "topic" id1 id2 ...
            let topic = parts[1].trim_matches('"').to_string();
            let volumes: Vec<String> = parts[2..].iter().map(|s| s.to_string()).collect();
            Some(GameCommand::Scribe { volumes, topic })
        }
        _ => None,
    }
}

/// Remove game-command code blocks from response text, keeping narrative.
/// Strips both ```json blocks with GameCommand and ```bash blocks with stacks-game.py.
pub fn strip_commands(response: &str) -> String {
    let mut result = Vec::new();
    let mut in_json = false;
    let mut in_bash_game = false;

    for line in response.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```json") {
            in_json = true;
        } else if (trimmed.starts_with("```bash") || trimmed.starts_with("```sh"))
            && !in_json
        {
            in_bash_game = true;
        } else if trimmed == "```" && in_json {
            in_json = false;
        } else if trimmed == "```" && in_bash_game {
            in_bash_game = false;
        } else if in_bash_game {
            // Only strip lines that are game commands; keep non-game bash
            if trimmed.contains("stacks-game.py") {
                // stripped
            } else {
                result.push(line);
            }
        } else if !in_json {
            // Also strip bare stacks-game.py lines outside code blocks
            if !trimmed.contains("stacks-game.py") {
                result.push(line);
            }
        }
    }
    result.join("\n").trim().to_string()
}

// ── Token Budget ───────────────────────────────────────────────────────

/// Tracks context usage for session compression decisions.
/// Updated from hermes `message.complete` events (`context_used` / `context_max`).
pub struct TokenBudget {
    pub context_used: u64,
    pub context_window: u64,
}

impl TokenBudget {
    pub fn new() -> Self {
        TokenBudget {
            context_used: 0,
            context_window: 262144,
        }
    }

    pub fn update(&mut self, context_used: u64, context_max: u64) {
        self.context_used = context_used;
        if context_max > 0 {
            self.context_window = context_max;
        }
    }

    /// Compress at 70% context usage.
    pub fn should_compress(&self) -> bool {
        self.context_used as f64 > self.context_window as f64 * 0.70
    }

    /// Force new session at 90% context usage.
    pub fn should_new_session(&self) -> bool {
        self.context_used as f64 > self.context_window as f64 * 0.90
    }
}
