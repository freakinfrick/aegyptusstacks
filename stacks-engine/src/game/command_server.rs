//! Game command server — Unix socket bridge for agent tool calls.
//!
//! The Librarian agent calls `stacks-game.py` via the terminal toolset.
//! That script connects to `/tmp/stacks-game.sock`, sends a JSON command,
//! and reads a JSON ack. This module handles the server side.
//!
//! Protocol: one JSON line per connection (request), one JSON line back (ack).

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::{mpsc, oneshot};

use super::hermes::GameCommand;

pub const SOCK_PATH: &str = "/tmp/stacks-game.sock";

// ── Types ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct CommandAck {
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl CommandAck {
    pub fn ok() -> Self {
        CommandAck { status: "ok", reason: None }
    }
    pub fn fail(reason: impl Into<String>) -> Self {
        CommandAck { status: "fail", reason: Some(reason.into()) }
    }
}

pub struct CommandRequest {
    pub command: GameCommand,
    pub ack_tx: oneshot::Sender<CommandAck>,
}

// ── Server ────────────────────────────────────────────────────────────

/// Start the command server. Returns the receiver for the main loop to drain.
/// Spawns a tokio task that accepts connections on the Unix socket.
pub fn start() -> mpsc::Receiver<CommandRequest> {
    // Remove stale socket file (ignore if absent)
    let _ = std::fs::remove_file(SOCK_PATH);

    let listener = match UnixListener::bind(SOCK_PATH) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[stacks] command_server: bind failed: {}", e);
            let (_tx, rx) = mpsc::channel(1);
            return rx;
        }
    };

    let (tx, rx) = mpsc::channel::<CommandRequest>(32);

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    let tx = tx.clone();
                    tokio::spawn(handle_connection(stream, tx));
                }
                Err(e) => {
                    eprintln!("[stacks] command_server: accept error: {}", e);
                }
            }
        }
    });

    rx
}

/// Remove the socket file (call during graceful shutdown).
pub fn cleanup() {
    let _ = std::fs::remove_file(SOCK_PATH);
}

// ── Per-connection handler ────────────────────────────────────────────

async fn handle_connection(
    stream: tokio::net::UnixStream,
    tx: mpsc::Sender<CommandRequest>,
) {
    let (reader, mut writer) = stream.into_split();
    let mut buf_reader = BufReader::new(reader);
    let mut line = String::new();

    // Read one JSON line
    match buf_reader.read_line(&mut line).await {
        Ok(0) | Err(_) => return, // EOF or error — client disconnected
        Ok(_) => {}
    }

    // Parse JSON
    let val: serde_json::Value = match serde_json::from_str(line.trim()) {
        Ok(v) => v,
        Err(_) => {
            let _ = write_ack(&mut writer, &CommandAck::fail("invalid JSON")).await;
            return;
        }
    };

    // Parse into GameCommand
    let command = match parse_command(&val) {
        Ok(cmd) => cmd,
        Err(reason) => {
            let _ = write_ack(&mut writer, &CommandAck::fail(reason)).await;
            return;
        }
    };

    // Create oneshot for ack
    let (ack_tx, ack_rx) = oneshot::channel();

    // Send to main loop
    if tx.send(CommandRequest { command, ack_tx }).await.is_err() {
        let _ = write_ack(&mut writer, &CommandAck::fail("game unavailable")).await;
        return;
    }

    // Wait for ack (15s timeout)
    let ack = match tokio::time::timeout(
        std::time::Duration::from_secs(15),
        ack_rx,
    ).await {
        Ok(Ok(ack)) => ack,
        Ok(Err(_)) => CommandAck::fail("game unavailable"),
        Err(_) => CommandAck::fail("timeout"),
    };

    let _ = write_ack(&mut writer, &ack).await;
}

async fn write_ack(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    ack: &CommandAck,
) -> Result<(), std::io::Error> {
    let mut json = serde_json::to_string(ack).unwrap_or_else(|_| r#"{"status":"fail"}"#.to_string());
    json.push('\n');
    writer.write_all(json.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

// ── Command parsing ───────────────────────────────────────────────────

/// Parse a JSON value into a GameCommand. Uses "cmd" as the discriminator
/// (not "type") to avoid touching GameCommand's serde config.
fn parse_command(val: &serde_json::Value) -> Result<GameCommand, String> {
    let cmd = val.get("cmd")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing \"cmd\" field".to_string())?;

    match cmd {
        "navigate" => {
            let coord = val.get("coord")
                .and_then(|v| v.as_array())
                .and_then(|arr| {
                    if arr.len() == 3 {
                        Some([
                            arr[0].as_i64()? as i32,
                            arr[1].as_i64()? as i32,
                            arr[2].as_i64()? as i32,
                        ])
                    } else {
                        None
                    }
                })
                .ok_or_else(|| "navigate: invalid coord (expected [Q,R,Z])".to_string())?;
            Ok(GameCommand::Navigate { coord })
        }
        "read" => {
            let id = val.get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "read: missing \"id\"".to_string())?
                .to_string();
            Ok(GameCommand::Read { id })
        }
        "gather" => {
            // Batch: {"cmd":"gather","ids":["id1","id2",...]}
            // Single: {"cmd":"gather","id":"id1"} (backward compat)
            if let Some(arr) = val.get("ids").and_then(|v| v.as_array()) {
                let ids: Vec<String> = arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                if ids.is_empty() {
                    return Err("gather: empty ids array".to_string());
                }
                Ok(GameCommand::GatherBatch { ids })
            } else {
                let id = val.get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "gather: missing \"id\" or \"ids\"".to_string())?
                    .to_string();
                Ok(GameCommand::Gather { id })
            }
        }
        "scribe" => {
            let volumes = val.get("volumes")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let topic = val.get("topic")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Ok(GameCommand::Scribe { volumes, topic })
        }
        other => Err(format!("unknown command: {}", other)),
    }
}
