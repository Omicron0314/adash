//! Rust boundary for adash's embedded pi SDK host.
//!
//! The app does not attach to an external `pi` process. It runs one Node host
//! (`agent/runtime`) that embeds `createAgentSession` from the pi SDK, and talks
//! to it over strict LF-framed JSONL on stdio. The SDK owns models, tools,
//! sessions, extension UI and streaming; this crate owns framing, process
//! lifecycle and the command vocabulary.
//!
//! See `agent/runtime/src/index.ts` and docs/ARCHITECTURE.md §「代理」.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// One JSONL frame: a single line, without the trailing `\n`.
pub type Frame = String;

/// Split a byte buffer into complete frames, returning the unconsumed tail.
#[must_use]
pub fn split_frames(buffer: &[u8]) -> (Vec<Frame>, Vec<u8>) {
    let mut frames = Vec::new();
    let mut rest = Vec::new();
    for chunk in buffer.split_inclusive(|byte| *byte == b'\n') {
        if chunk.last() == Some(&b'\n') {
            let body = &chunk[..chunk.len() - 1];
            // Tolerate CRLF, as JSONL peers commonly emit it.
            let body = body.strip_suffix(b"\r").unwrap_or(body);
            let line = String::from_utf8_lossy(body).into_owned();
            if !line.is_empty() {
                frames.push(line);
            }
        } else {
            rest.extend_from_slice(chunk);
        }
    }
    (frames, rest)
}

/// How to launch the SDK host.
#[derive(Debug, Clone)]
pub struct HostConfig {
    /// Node.js executable; `node` by default.
    pub program: String,
    /// Path to the built host (`agent/runtime/dist/index.js`).
    pub script: PathBuf,
    /// Repository root, so the host can find the extension, skills and tools.
    pub root: PathBuf,
    /// Path to `adash-tool`; the extension shells out to it.
    pub tool: PathBuf,
    /// Optional model override, `provider/id`.
    pub model: Option<String>,
    /// Session directory. `None` lets the SDK choose inside the agent data dir.
    pub session_dir: Option<PathBuf>,
}

impl HostConfig {
    /// The full argument list, in a stable order.
    #[must_use]
    pub fn argv(&self) -> Vec<String> {
        vec![self.script.display().to_string()]
    }

    /// Environment handed to the SDK host. Never contains credentials.
    #[must_use]
    pub fn env(&self) -> Vec<(String, String)> {
        let mut env = vec![
            ("ADASH_ROOT".to_owned(), self.root.display().to_string()),
            ("ADASH_TOOL".to_owned(), self.tool.display().to_string()),
        ];
        if let Some(model) = &self.model {
            env.push(("ADASH_MODEL".to_owned(), model.clone()));
        }
        env
    }
}

/// What the SDK host said.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// Startup completed, with provider/model/tool list.
    Ready(Value),
    /// Startup failed; the app falls back to local answers.
    StartupError(String),
    /// Extension asked to confirm (`ui_request`).
    UiRequest(Value),
    /// Everything else: `message_update`, `tool_execution_*`, `agent_settled`, …
    Event(Value),
}

impl AgentEvent {
    fn classify(value: Value) -> Self {
        match value.get("type").and_then(Value::as_str) {
            Some("ready") => Self::Ready(value),
            Some("startup_error") => Self::StartupError(
                value
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("代理启动失败")
                    .to_owned(),
            ),
            Some("ui_request") => Self::UiRequest(value),
            _ => Self::Event(value),
        }
    }
}

/// A running SDK host.
pub struct AgentHost {
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    events: Receiver<AgentEvent>,
    config: HostConfig,
}

impl std::fmt::Debug for AgentHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentHost")
            .field("pid", &self.child.id())
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl AgentHost {
    /// Spawn the host. `start` is sent immediately with the session directory.
    pub fn spawn(config: HostConfig) -> std::io::Result<Self> {
        let mut command = Command::new(&config.program);
        command
            .args(config.argv())
            .current_dir(&config.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in config.env() {
            command.env(key, value);
        }
        let mut child = command.spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| std::io::Error::other("agent stdin unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("agent stdout unavailable"))?;
        let (sender, events) = mpsc::channel();
        spawn_reader(stdout, sender);
        // stderr is diagnostics only; drain it so the host never blocks.
        if let Some(stderr) = child.stderr.take() {
            spawn_stderr_drain(stderr);
        }
        let host = Self {
            child,
            stdin: Arc::new(Mutex::new(stdin)),
            events,
            config,
        };
        let mut start = json!({"type": "start"});
        if let Some(dir) = &host.config.session_dir {
            start["session_dir"] = json!(dir.display().to_string());
        }
        if let Some(model) = &host.config.model {
            start["model"] = json!(model);
        }
        host.send(&start)?;
        Ok(host)
    }

    /// The configuration this host runs with.
    #[must_use]
    pub fn config(&self) -> &HostConfig {
        &self.config
    }

    /// Write one command (one line) to the host.
    pub fn send(&self, value: &Value) -> std::io::Result<()> {
        let mut stdin = self
            .stdin
            .lock()
            .map_err(|_| std::io::Error::other("agent stdin poisoned"))?;
        let mut line = serde_json::to_vec(value).map_err(std::io::Error::other)?;
        line.push(b'\n');
        stdin.write_all(&line)?;
        stdin.flush()
    }

    /// Send a prompt.
    pub fn prompt(&self, message: &str) -> std::io::Result<()> {
        self.send(&json!({"type": "prompt", "message": message}))
    }

    /// Stop the current run.
    pub fn abort(&self) -> std::io::Result<()> {
        self.send(&json!({"type": "abort"}))
    }

    /// Resolve a plan confirmation requested by the embedded SDK.
    pub fn confirm(&self, id: &str, confirmed: bool) -> std::io::Result<()> {
        self.send(&json!({"type": "ui_response", "id": id, "confirmed": confirmed}))
    }

    /// Switch model mid-session (`provider/id`).
    pub fn set_model(&self, model: &str) -> std::io::Result<()> {
        self.send(&json!({"type": "set_model", "model": model}))
    }

    /// Non-blocking drain of everything the host has said since last call.
    pub fn try_poll(&self) -> Vec<AgentEvent> {
        self.events.try_iter().collect()
    }

    /// Whether the child has exited.
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Close stdin and wait for the child.
    pub fn shutdown(&mut self) -> std::io::Result<()> {
        let _ = self.send(&json!({"type": "dispose"}));
        let _ = self.child.kill();
        let _ = self.child.wait();
        Ok(())
    }
}

impl Drop for AgentHost {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn spawn_reader(stdout: std::process::ChildStdout, sender: Sender<AgentEvent>) {
    std::thread::Builder::new()
        .name("adash-agent-reader".to_owned())
        .spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = Vec::new();
            loop {
                line.clear();
                match reader.read_until(b'\n', &mut line) {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(_) => break,
                }
                while matches!(line.last(), Some(b'\n' | b'\r')) {
                    line.pop();
                }
                if line.is_empty() {
                    continue;
                }
                let value: Value = match serde_json::from_slice(&line) {
                    Ok(value) => value,
                    Err(_) => continue, // host diagnostics are not protocol
                };
                if sender.send(AgentEvent::classify(value)).is_err() {
                    break;
                }
            }
        })
        .ok();
}

fn spawn_stderr_drain(stderr: std::process::ChildStderr) {
    std::thread::Builder::new()
        .name("adash-agent-stderr".to_owned())
        .spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                line.clear();
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_partial_tail_for_next_read() {
        let (frames, rest) = split_frames(b"{\"a\":1}\n{\"b\":");
        assert_eq!(frames, vec!["{\"a\":1}".to_owned()]);
        assert_eq!(rest, b"{\"b\":");
    }

    #[test]
    fn splits_crlf_frames() {
        let (frames, rest) = split_frames(b"{\"a\":1}\r\n");
        assert_eq!(frames, vec!["{\"a\":1}".to_owned()]);
        assert!(rest.is_empty());
    }

    #[test]
    fn does_not_split_on_unicode_separators() {
        let line = "{\"message\":\"a\u{2028}b\"}\n";
        let (frames, rest) = split_frames(line.as_bytes());
        assert_eq!(frames.len(), 1);
        assert!(rest.is_empty());
    }

    #[test]
    fn host_is_launched_by_script_not_cli_mode() {
        let config = HostConfig {
            program: "node".to_owned(),
            script: PathBuf::from("/repo/agent/runtime/dist/index.js"),
            root: PathBuf::from("/repo"),
            tool: PathBuf::from("/repo/target/debug/adash-tool"),
            model: Some("magpie/auto".to_owned()),
            session_dir: None,
        };
        assert_eq!(config.argv(), vec!["/repo/agent/runtime/dist/index.js"]);
        assert!(config.env().iter().any(|(key, _)| key == "ADASH_TOOL"));
        assert!(!config.argv().iter().any(|arg| arg == "--mode"));
    }

    #[test]
    fn classifies_records_by_type() {
        assert!(matches!(
            AgentEvent::classify(json!({"type": "ready"})),
            AgentEvent::Ready(_)
        ));
        assert!(matches!(
            AgentEvent::classify(json!({"type": "ui_request", "method": "confirm"})),
            AgentEvent::UiRequest(_)
        ));
        assert!(matches!(
            AgentEvent::classify(json!({"type": "agent_settled"})),
            AgentEvent::Event(_)
        ));
        assert!(matches!(
            AgentEvent::classify(json!({"type": "startup_error", "error": "no model"})),
            AgentEvent::StartupError(_)
        ));
    }
}
