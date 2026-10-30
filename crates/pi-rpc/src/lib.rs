//! Client for `pi --mode rpc`: strict LF-framed JSONL over the child's stdio.
//!
//! The GUI never embeds Node. It spawns `pi --mode rpc` with
//! `--no-builtin-tools` and talks to it here: request/response correlation, the
//! event stream, and the extension-UI sub-protocol (confirm cards).
//!
//! Records are passed through as [`serde_json::Value`]: this crate owns framing,
//! lifecycle and correlation, not the (versioned) shape of assistant messages.
//! See <https://github.com/earendil-works/pi/blob/main/docs/rpc.md>.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
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
            // Tolerate CRLF, as the protocol asks clients to.
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

/// How to launch pi.
#[derive(Debug, Clone)]
pub struct PiConfig {
    /// The executable; `pi` on `PATH` by default.
    pub program: String,
    /// Extra arguments appended after the RPC ones.
    pub args: Vec<String>,
    /// Working directory; the app data dir by default.
    pub cwd: Option<PathBuf>,
    /// Keep pi's own session files (off by default: adash owns the history).
    pub session: bool,
    /// Extra environment variables (e.g. `ADASH_TOOL`).
    pub env: Vec<(String, String)>,
}

impl Default for PiConfig {
    fn default() -> Self {
        Self {
            program: "pi".to_owned(),
            args: Vec::new(),
            cwd: None,
            session: false,
            env: Vec::new(),
        }
    }
}

impl PiConfig {
    /// The full argument list, in a stable order.
    pub fn argv(&self) -> Vec<String> {
        let mut argv = vec!["--mode".to_owned(), "rpc".to_owned()];
        if !self.session {
            argv.push("--no-session".to_owned());
        }
        argv.extend(self.args.iter().cloned());
        argv
    }
}

/// What the child said.
#[derive(Debug, Clone)]
pub enum PiRecord {
    /// A response to one of our commands (`type: "response"`).
    Response(Value),
    /// An extension asking for user input (`extension_ui_request`).
    UiRequest(Value),
    /// Anything else: session events (`message_update`, `agent_settled`, …).
    Event(Value),
}

impl PiRecord {
    fn classify(value: Value) -> Self {
        match value.get("type").and_then(Value::as_str) {
            Some("response") => Self::Response(value),
            Some("extension_ui_request") => Self::UiRequest(value),
            _ => Self::Event(value),
        }
    }
}

/// A running `pi --mode rpc` child.
pub struct PiSession {
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    records: Receiver<PiRecord>,
    next_id: AtomicU64,
    config: PiConfig,
}

impl std::fmt::Debug for PiSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PiSession")
            .field("pid", &self.child.id())
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl PiSession {
    /// Spawn the child and start reading its stdout.
    pub fn spawn(config: PiConfig) -> std::io::Result<Self> {
        let mut command = Command::new(&config.program);
        command
            .args(config.argv())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(cwd) = &config.cwd {
            command.current_dir(cwd);
        }
        for (key, value) in &config.env {
            command.env(key, value);
        }

        let mut child = command.spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| std::io::Error::other("pi stdin unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("pi stdout unavailable"))?;

        let (sender, records) = mpsc::channel();
        spawn_reader(stdout, sender);
        // stderr is diagnostics only; drain it so pi never blocks on a full pipe.
        if let Some(stderr) = child.stderr.take() {
            spawn_stderr_drain(stderr);
        }

        Ok(Self {
            child,
            stdin: Arc::new(Mutex::new(stdin)),
            records,
            next_id: AtomicU64::new(1),
            config,
        })
    }

    /// Spawn with the default configuration.
    pub fn connect() -> std::io::Result<Self> {
        Self::spawn(PiConfig::default())
    }

    /// The configuration this session runs with.
    pub fn config(&self) -> &PiConfig {
        &self.config
    }

    /// Next request id.
    pub fn next_id(&self) -> String {
        format!("adash-{}", self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    /// Write one record (one line) to the child.
    pub fn send(&self, value: &Value) -> std::io::Result<()> {
        let mut stdin = self
            .stdin
            .lock()
            .map_err(|_| std::io::Error::other("pi stdin poisoned"))?;
        let mut line = serde_json::to_vec(value).map_err(std::io::Error::other)?;
        line.push(b'\n');
        stdin.write_all(&line)?;
        stdin.flush()
    }

    /// Send a prompt; returns the request id to correlate the response.
    pub fn prompt(&self, message: &str) -> std::io::Result<String> {
        let id = self.next_id();
        self.send(&json!({"id": id, "type": "prompt", "message": message}))?;
        Ok(id)
    }

    /// Abort the current run.
    pub fn abort(&self) -> std::io::Result<()> {
        self.send(&json!({"type": "abort"}))
    }

    /// Answer an `extension_ui_request` confirm dialog.
    pub fn respond_confirm(&self, id: &str, confirmed: bool) -> std::io::Result<()> {
        self.send(&json!({
            "type": "extension_ui_response",
            "id": id,
            "confirmed": confirmed,
        }))
    }

    /// Answer an `extension_ui_request` select/input dialog.
    pub fn respond_value(&self, id: &str, value: &str) -> std::io::Result<()> {
        self.send(&json!({
            "type": "extension_ui_response",
            "id": id,
            "value": value,
        }))
    }

    /// Answer an `extension_ui_request` by cancelling it.
    pub fn respond_cancelled(&self, id: &str) -> std::io::Result<()> {
        self.send(&json!({
            "type": "extension_ui_response",
            "id": id,
            "cancelled": true,
        }))
    }

    /// Non-blocking drain of everything the child has said since last call.
    pub fn try_poll(&self) -> Vec<PiRecord> {
        let mut records = Vec::new();
        while let Ok(record) = self.records.try_recv() {
            records.push(record);
        }
        records
    }

    /// Block until one record arrives, or `None` when the child is gone.
    pub fn recv(&self) -> Option<PiRecord> {
        self.records.recv().ok()
    }

    /// Whether the child has exited.
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Close stdin and wait for the child.
    pub fn shutdown(&mut self) -> std::io::Result<()> {
        // Dropping the write end is the documented orderly shutdown.
        if let Ok(mut stdin) = self.stdin.lock() {
            let _ = stdin.flush();
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        Ok(())
    }
}

impl Drop for PiSession {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn spawn_reader(stdout: std::process::ChildStdout, sender: Sender<PiRecord>) {
    std::thread::Builder::new()
        .name("adash-pi-reader".to_owned())
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
                    Err(_) => continue, // diagnostics on stdout are not protocol
                };
                if sender.send(PiRecord::classify(value)).is_err() {
                    break;
                }
            }
        })
        .ok();
}

fn spawn_stderr_drain(stderr: std::process::ChildStderr) {
    std::thread::Builder::new()
        .name("adash-pi-stderr".to_owned())
        .spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                // Deliberately not logged as protocol; keep the last lines only
                // when a diagnostic layer is added (see ROADMAP M5).
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
    fn argv_keeps_the_agent_restricted() {
        let config = PiConfig::default();
        assert_eq!(config.argv(), vec!["--mode", "rpc", "--no-session"]);
    }

    #[test]
    fn classifies_records_by_type() {
        assert!(matches!(
            PiRecord::classify(json!({"type": "response", "success": true})),
            PiRecord::Response(_)
        ));
        assert!(matches!(
            PiRecord::classify(json!({"type": "extension_ui_request", "method": "confirm"})),
            PiRecord::UiRequest(_)
        ));
        assert!(matches!(
            PiRecord::classify(json!({"type": "agent_settled"})),
            PiRecord::Event(_)
        ));
    }
}
