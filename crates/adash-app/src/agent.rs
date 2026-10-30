//! The conversation: the embedded pi SDK host when it is available, a local
//! responder when it is not.
//!
//! The SDK host is `agent/runtime` (Node). It owns the session, model, tools and
//! extension UI; this module only spawns it and turns its JSONL events into app
//! state. The local responder answers from the same domain data and says so, so
//! a missing model never leaves an empty bubble or invents numbers.

use agent_sdk::{AgentEvent, AgentHost, HostConfig};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Who said it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The user.
    User,
    /// The agent.
    Agent,
}

/// One turn in the conversation.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// Speaker.
    pub role: Role,
    /// Body text; empty while the agent is still thinking.
    pub text: String,
    /// Folded tool-call lines, e.g. 「✓ ucloud_inventory · 12ms」.
    pub notes: Vec<String>,
    /// Produced by the local responder rather than the model.
    pub offline: bool,
}

impl ChatMessage {
    /// A user message.
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            text: text.into(),
            notes: Vec::new(),
            offline: false,
        }
    }

    /// An agent message.
    pub fn agent(text: impl Into<String>, offline: bool) -> Self {
        Self {
            role: Role::Agent,
            text: text.into(),
            notes: Vec::new(),
            offline,
        }
    }
}

/// An extension asking the app to confirm a plan, delivered as `ui_request`
/// with `method: "confirm"`.
#[derive(Debug, Clone)]
pub struct ConfirmRequest {
    /// Correlation id of the SDK dialog.
    pub request_id: String,
    /// The plan the agent wants to apply.
    pub plan_id: String,
}

/// Where answers come from.
pub enum Backend {
    /// The embedded pi SDK host.
    Sdk(Box<AgentHost>),
    /// Local responder, with the reason the SDK is unavailable.
    Offline(String),
}

/// Conversation + agent process state.
pub struct Agent {
    /// The host or the fallback.
    pub backend: Backend,
    /// A plan waiting for the user.
    pub pending: Option<ConfirmRequest>,
    /// Whether a run is in flight.
    pub streaming: bool,
    /// One line of progress.
    pub status: Option<String>,
    /// Why the last prompt was refused (missing model key, provider error).
    pub failure: Option<String>,
    /// Model reported by the SDK host.
    pub model: Option<String>,
    /// Tools the SDK session exposes, as reported at startup.
    pub tools: Vec<String>,
}

impl Agent {
    /// Start the embedded SDK host with the restricted resource set.
    ///
    /// On any failure the app stays usable: the reason is kept for the banner
    /// and the local responder takes over.
    pub fn connect() -> Self {
        if std::env::var_os("ADASH_OFFLINE").is_some() {
            return Self::offline("ADASH_OFFLINE 已启用".to_owned());
        }
        match Self::spawn_host() {
            Ok(host) => Self {
                backend: Backend::Sdk(Box::new(host)),
                pending: None,
                streaming: false,
                status: Some("正在启动代理…".to_owned()),
                failure: None,
                model: None,
                tools: Vec::new(),
            },
            Err(reason) => Self::offline(reason),
        }
    }

    fn offline(reason: String) -> Self {
        Self {
            backend: Backend::Offline(reason),
            pending: None,
            streaming: false,
            status: None,
            failure: None,
            model: None,
            tools: Vec::new(),
        }
    }

    fn spawn_host() -> Result<AgentHost, String> {
        let root = repo_root();
        let script = root.join("agent/runtime/dist/index.js");
        if !script.exists() {
            return Err(format!(
                "找不到 SDK 宿主 {}，先运行 npm --prefix agent/runtime run build",
                script.display()
            ));
        }
        let extension = root.join("agent/extensions/ucloud/index.ts");
        if !extension.exists() {
            return Err(format!("找不到扩展 {}", extension.display()));
        }
        let tool = PathBuf::from(
            std::env::var("ADASH_TOOL")
                .unwrap_or_else(|_| root.join("target/debug/adash-tool").display().to_string()),
        );
        if !tool.exists() {
            return Err(format!(
                "找不到 adash-tool（{}），先 cargo build -p adash-tool",
                tool.display()
            ));
        }
        let session_dir = data_dir().map(|dir| dir.join("sessions"));
        let config = HostConfig {
            program: std::env::var("ADASH_NODE").unwrap_or_else(|_| "node".to_owned()),
            script,
            root,
            tool,
            model: std::env::var("ADASH_MODEL").ok(),
            session_dir,
        };
        AgentHost::spawn(config).map_err(|error| format!("无法启动代理宿主：{error}"))
    }

    /// Whether the SDK host is running (not necessarily ready yet).
    #[must_use]
    pub fn is_online(&self) -> bool {
        matches!(self.backend, Backend::Sdk(_))
    }

    /// Why the app is offline.
    #[must_use]
    pub fn offline_reason(&self) -> Option<&str> {
        match &self.backend {
            Backend::Offline(reason) => Some(reason),
            Backend::Sdk(_) => None,
        }
    }

    /// Send a prompt.
    pub fn prompt(&mut self, text: &str) -> bool {
        let Backend::Sdk(host) = &mut self.backend else {
            return false;
        };
        match host.prompt(text) {
            Ok(()) => {
                self.streaming = true;
                self.status = Some("正在理解你的问题…".to_owned());
                true
            }
            Err(error) => {
                self.streaming = false;
                self.status = Some(format!("发送失败：{error}"));
                false
            }
        }
    }

    pub fn answer_confirm(&mut self, id: &str, confirmed: bool) {
        if let Backend::Sdk(host) = &self.backend {
            let _ = host.confirm(id, confirmed);
        }
        self.pending = None;
    }

    /// Tell the SDK session what the user decided, so the model can summarise
    /// the real outcome instead of guessing. The GUI has already executed.
    pub fn inform(&self, text: &str) {
        if let Backend::Sdk(host) = &self.backend {
            let _ = host.send(&serde_json::json!({"type": "follow_up", "message": text}));
        }
    }

    /// Drain whatever the host has said since last frame.
    ///
    /// `text` receives streaming deltas; the returned value is `true` when the
    /// run finished (so the caller can stop polling).
    pub fn pump(&mut self, mut on_text: impl FnMut(&str), mut on_tool: impl FnMut(String)) -> bool {
        let Backend::Sdk(host) = &self.backend else {
            return true;
        };
        let mut settled = false;
        for event in host.try_poll() {
            match event {
                AgentEvent::StartupError(reason) => {
                    self.failure = Some(reason);
                    settled = true;
                }
                AgentEvent::Ready(value) => {
                    self.model = Some(format!(
                        "{}/{}",
                        value.get("provider").and_then(Value::as_str).unwrap_or(""),
                        value.get("model").and_then(Value::as_str).unwrap_or("")
                    ));
                    self.tools = value
                        .get("tools")
                        .and_then(Value::as_array)
                        .map(|tools| {
                            tools
                                .iter()
                                .filter_map(Value::as_str)
                                .map(ToOwned::to_owned)
                                .collect()
                        })
                        .unwrap_or_default();
                    self.status = None;
                }
                AgentEvent::UiRequest(value) => {
                    let method = value.get("method").and_then(Value::as_str).unwrap_or("");
                    if method == "confirm" {
                        let plan_id = value
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned();
                        self.pending = Some(ConfirmRequest {
                            request_id: value
                                .get("id")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_owned(),
                            plan_id,
                        });
                        self.status = Some("等待你确认计划…".to_owned());
                    }
                }
                AgentEvent::Event(value) => match value.get("type").and_then(Value::as_str) {
                    Some("message_update") => {
                        let event = value.get("assistantMessageEvent").unwrap_or(&value);
                        match event.get("type").and_then(Value::as_str) {
                            Some("text_delta") => {
                                if let Some(delta) = event.get("delta").and_then(Value::as_str) {
                                    on_text(delta);
                                }
                            }
                            Some("toolcall_start") => {
                                let name = event
                                    .get("toolName")
                                    .and_then(Value::as_str)
                                    .unwrap_or("tool");
                                self.status = Some(format!("调用 {name}…"));
                            }
                            _ => {}
                        }
                    }
                    Some("tool_execution_start") => {
                        let name = value
                            .get("toolName")
                            .and_then(Value::as_str)
                            .unwrap_or("tool");
                        self.status = Some(format!("调用 {name}…"));
                    }
                    Some("tool_execution_end") => {
                        let name = value
                            .get("toolName")
                            .and_then(Value::as_str)
                            .unwrap_or("tool");
                        let failed = value.get("isError").and_then(Value::as_bool) == Some(true);
                        let millis = value.get("durationMs").and_then(Value::as_u64);
                        let mark = if failed { "!" } else { "✓" };
                        let timing = millis.map_or_else(String::new, |ms| format!(" · {ms}ms"));
                        on_tool(format!("{mark} {name}{timing}"));
                        self.status = Some(format!("{name} 完成"));
                    }
                    Some("agent_settled") => settled = true,
                    Some("error") => {
                        let reason = value
                            .get("error")
                            .and_then(Value::as_str)
                            .unwrap_or("模型出错");
                        self.failure = Some(reason.to_owned());
                        settled = true;
                    }
                    _ => {}
                },
            }
        }
        if settled {
            self.streaming = false;
            self.status = None;
        }
        settled
    }

    /// Stop the current run.
    pub fn abort(&mut self) {
        if let Backend::Sdk(host) = &self.backend {
            let _ = host.abort();
        }
        self.streaming = false;
        self.status = None;
    }
}

/// The repository root, so the host can be pointed at this checkout.
#[must_use]
pub fn repo_root() -> PathBuf {
    if let Ok(root) = std::env::var("ADASH_ROOT") {
        return PathBuf::from(root);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Where adash keeps state and SDK sessions.
#[must_use]
pub fn data_dir() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("ADASH_DATA_DIR") {
        return Some(PathBuf::from(path));
    }
    dirs::data_dir().map(|dir| dir.join("adash"))
}

/// True when `path` looks like the built SDK host.
#[must_use]
pub fn is_host_script(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some("index.js")
}
