//! ScribeBridge — subprocess-based bridge to kimi-scribe.py for article synthesis.

use tokio::io::AsyncBufReadExt;
use tokio::sync::mpsc;

/// Status of the scribe synthesis pipeline.
#[derive(Debug, Clone)]
pub enum ScribeStatus {
    Idle,
    Fetching,
    Reasoning,
    Writing,
    Complete,
    Error(String),
}

/// Events emitted by the scribe subprocess.
#[derive(Debug)]
pub enum ScribeEvent {
    Status { text: String },
    Meta { title: String, volume_count: u32, passage_count: u32 },
    ModelSwitch { name: String },
    Reasoning { text: String },
    Delta { text: String },
    Complete { text: String, word_count: u32, file: String },
    Error { message: String },
}

/// Bridge to the kimi-scribe.py subprocess.
pub struct ScribeBridge {
    child: Option<tokio::process::Child>,
    reader_rx: mpsc::UnboundedReceiver<ScribeEvent>,
    pub status: ScribeStatus,
    pub article_text: String,
    pub article_title: String,
    pub article_file: String,
    pub word_count: u32,
    pub model: String,
    pub volume_count: u32,
    pub passage_count: u32,
}

impl ScribeBridge {
    pub async fn spawn(
        volume_ids: &[String],
        topic: &str,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let cwd = std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_else(|_| ".".into());
        let python = "python3".to_string();
        let shim = format!(
            "{}/librarian/kimi-scribe.py", cwd
        );

        let api_key = load_scribe_api_key(&cwd);

        let mut child = tokio::process::Command::new(&python)
            .arg(&shim)
            .args(["--volumes", &volume_ids.join(",")])
            .args(["--topic", topic])
            .env("SCRIBE_API_KEY", &api_key)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(scribe_reader_task(stdout, tx));

        // Extract model name for display
        let model_name = std::env::var("HERMES_NOUS_MODEL")
            .unwrap_or_else(|_| "moonshotai/kimi-k2-thinking".to_string());

        Ok(ScribeBridge {
            child: Some(child),
            reader_rx: rx,
            status: ScribeStatus::Fetching,
            article_text: String::new(),
            article_title: String::new(),
            article_file: String::new(),
            word_count: 0,
            model: model_name,
            volume_count: volume_ids.len() as u32,
            passage_count: 0,
        })
    }

    pub fn try_recv(&mut self) -> Option<ScribeEvent> {
        self.reader_rx.try_recv().ok()
    }

    pub async fn kill(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill().await;
        }
    }
}

/// Background task: reads JSON-lines from scribe subprocess stdout.
async fn scribe_reader_task(
    stdout: tokio::process::ChildStdout,
    tx: mpsc::UnboundedSender<ScribeEvent>,
) {
    let reader = tokio::io::BufReader::new(stdout);
    let mut lines = reader.lines();

    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let event_type = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let event = match event_type {
            "status" => {
                let text = msg.get("text").and_then(|v| v.as_str())
                    .unwrap_or("").to_string();
                ScribeEvent::Status { text }
            }
            "meta" => ScribeEvent::Meta {
                title: msg.get("title").and_then(|v| v.as_str())
                    .unwrap_or("").to_string(),
                volume_count: msg.get("volume_count").and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32,
                passage_count: msg.get("passage_count").and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32,
            },
            "model" => {
                let name = msg.get("name").and_then(|v| v.as_str())
                    .unwrap_or("").to_string();
                ScribeEvent::ModelSwitch { name }
            }
            "reasoning" => {
                let text = msg.get("text").and_then(|v| v.as_str())
                    .unwrap_or("").to_string();
                ScribeEvent::Reasoning { text }
            }
            "delta" => {
                let text = msg.get("text").and_then(|v| v.as_str())
                    .unwrap_or("").to_string();
                ScribeEvent::Delta { text }
            }
            "complete" => ScribeEvent::Complete {
                text: msg.get("text").and_then(|v| v.as_str())
                    .unwrap_or("").to_string(),
                word_count: msg.get("word_count").and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32,
                file: msg.get("file").and_then(|v| v.as_str())
                    .unwrap_or("").to_string(),
            },
            "error" => ScribeEvent::Error {
                message: msg.get("text").and_then(|v| v.as_str())
                    .unwrap_or("unknown error").to_string(),
            },
            _ => continue,
        };
        let is_complete = matches!(&event, ScribeEvent::Complete { .. });
        if tx.send(event).is_err() { break; }
        if is_complete { return; }
    }
    // Only emit error if the stream ended without a Complete event
    let _ = tx.send(ScribeEvent::Error {
        message: "Scribe process ended unexpectedly".to_string(),
    });
}

/// Read NOUS_API_KEY from ~/Aegyptus/.env file.
fn load_scribe_api_key(stacks_root: &str) -> String {
    if let Ok(key) = std::env::var("SCRIBE_API_KEY") {
        if !key.is_empty() { return key; }
    }
    let env_path = format!("{}/.env", stacks_root);
    if let Ok(contents) = std::fs::read_to_string(&env_path) {
        for line in contents.lines() {
            let trimmed = line.trim();
            if let Some(val) = trimmed.strip_prefix("SCRIBE_API_KEY=") {
                return val.trim().trim_matches('"').trim_matches('\'').to_string();
            }
        }
    }
    String::new()
}
