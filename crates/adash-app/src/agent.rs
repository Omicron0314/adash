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
    Starting(std::sync::mpsc::Receiver<Result<AgentHost, String>>),
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
    queued_prompt: Option<String>,
}

impl Agent {
    /// Start the embedded SDK host with the restricted resource set.
    ///
    /// On any failure the app stays usable: the reason is kept for the banner
    /// and the local responder takes over.
    pub fn connect(mode: ucloud_api::Mode) -> Self {
        #[cfg(feature = "ui-test-hooks")]
        if std::env::var_os("ADASH_OFFLINE").is_some() {
            return Self::offline("测试构建已启用离线模式".into());
        }
        let mut settings = match crate::ai::AiSettings::load() {
            Ok(settings) => settings,
            Err(error) => return Self::offline(error),
        };
        if settings.endpoint.is_empty() {
            return Self::offline("请在账户页填写 Endpoint、API key 和模型".into());
        }
        if let Err(error) = settings.validate() {
            return Self::offline(error);
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(Self::spawn_host(settings, mode));
        });
        Self {
            backend: Backend::Starting(rx),
            pending: None,
            streaming: false,
            status: Some("正在连接 AI…".into()),
            failure: None,
            model: None,
            tools: Vec::new(),
            queued_prompt: None,
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
            queued_prompt: None,
        }
    }

    fn spawn_host(
        settings: crate::ai::AiSettings,
        mode: ucloud_api::Mode,
    ) -> Result<AgentHost, String> {
        let root = adash_paths::resource_dir()
            .ok_or("未找到应用资源，请检查安装目录或设置 ADASH_RESOURCES")?;
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
        let tool =
            adash_paths::tool_path().ok_or("未找到 adash-tool，请将其放在应用同目录或 PATH 中")?;
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
            mock: mode == ucloud_api::Mode::Mock,
            session_dir,
        };
        let key = settings.secret()?;
        AgentHost::spawn_with_provider(config, Some(settings.startup(&key)))
            .map_err(|_| "无法启动代理宿主，请检查 Node.js 是否已安装".into())
    }

    /// Whether the SDK host is running (not necessarily ready yet).
    #[must_use]
    pub fn is_online(&self) -> bool {
        matches!(self.backend, Backend::Sdk(_) | Backend::Starting(_))
    }

    /// Why the app is offline.
    #[must_use]
    pub fn offline_reason(&self) -> Option<&str> {
        match &self.backend {
            Backend::Offline(reason) => Some(reason),
            Backend::Sdk(_) | Backend::Starting(_) => None,
        }
    }

    /// Send a prompt.
    pub fn prompt(&mut self, text: &str) -> bool {
        if matches!(self.backend, Backend::Starting(_)) {
            self.queued_prompt = Some(text.into());
            self.streaming = true;
            return true;
        }
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
        if let Backend::Starting(rx) = &self.backend {
            match rx.try_recv() {
                Ok(Ok(host)) => {
                    self.backend = Backend::Sdk(Box::new(host));
                    if let Some(text) = self.queued_prompt.take() {
                        self.prompt(&text);
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return false,
                result => {
                    let reason = match result {
                        Ok(Err(reason)) => reason,
                        _ => "AI 连接任务已退出".into(),
                    };
                    self.backend = Backend::Offline(reason.clone());
                    self.failure = Some(reason);
                    self.streaming = false;
                    self.status = None;
                    self.queued_prompt = None;
                    return true;
                }
            }
        }
        let Backend::Sdk(host) = &self.backend else {
            return true;
        };
        let mut settled = false;
        let mut offline = None;
        for event in host.try_poll() {
            match event {
                AgentEvent::Closed => {
                    let reason = "AI 宿主已退出，请检查 Node.js 版本与应用资源".to_owned();
                    self.failure = Some(reason.clone());
                    offline = Some(reason);
                    settled = true;
                }
                AgentEvent::StartupError(reason) => {
                    self.failure = Some(reason.clone());
                    offline = Some(reason);
                    settled = true;
                }
                AgentEvent::Ready(value) => {
                    self.model = value
                        .get("model")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
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
        if let Some(reason) = offline {
            self.backend = Backend::Offline(reason);
        }
        if settled {
            self.streaming = false;
            self.status = None;
        }
        settled
    }

    /// Stop the current run.
    pub fn abort(&mut self) {
        self.queued_prompt = None;
        if matches!(self.backend, Backend::Starting(_)) {
            self.backend = Backend::Offline("AI 启动已取消，可在设置中重新连接".into());
            self.streaming = false;
            self.status = None;
            return;
        }
        if let Backend::Sdk(host) = &self.backend {
            if let Err(error) = host.abort() {
                self.failure = Some(error.to_string());
                self.streaming = false;
            } else {
                self.status = Some("正在停止…".into());
            }
        } else {
            self.streaming = false;
            self.status = None;
        }
    }
}

/// Where adash keeps state and SDK sessions.
#[must_use]
pub fn data_dir() -> Option<PathBuf> {
    Some(adash_paths::data_dir())
}

/// True when `path` looks like the built SDK host.
#[must_use]
pub fn is_host_script(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some("index.js")
}
