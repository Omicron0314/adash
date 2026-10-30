//! The conversation: pi over RPC when it is available, a local responder when
//! it is not.
//!
//! The local responder is not a fake model. It answers from the same domain data
//! the pages render, and says so (`离线`), so a demo without a configured model
//! still shows real numbers instead of pretending.

use pi_rpc::{PiConfig, PiRecord, PiSession};
use serde_json::Value;
use std::path::PathBuf;

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

/// An extension asking the app to confirm a plan, delivered as
/// `extension_ui_request` with `method: "confirm"`.
#[derive(Debug, Clone)]
pub struct ConfirmRequest {
    /// The request id to answer.
    pub request_id: String,
    /// The plan the agent wants to apply.
    pub plan_id: String,
}

/// Where answers come from.
pub enum Backend {
    /// A live `pi --mode rpc` child.
    Pi(Box<PiSession>),
    /// Local responder, with the reason pi is unavailable.
    Offline(String),
}

/// Conversation + agent process state.
pub struct Agent {
    /// Live pi or the fallback.
    pub backend: Backend,
    /// A plan waiting for the user's click.
    pub pending: Option<ConfirmRequest>,
    /// Whether a run is in flight.
    pub streaming: bool,
    /// Tool calls seen in this run, for the status line.
    pub status: Option<String>,
}

impl Agent {
    /// Connect to pi with the restricted configuration from AGENTS.md.
    ///
    /// On any failure the app stays usable: the reason is kept for the banner
    /// and the local responder takes over.
    pub fn connect() -> Self {
        match Self::spawn_pi() {
            Ok(session) => Self {
                backend: Backend::Pi(Box::new(session)),
                pending: None,
                streaming: false,
                status: None,
            },
            Err(reason) => Self {
                backend: Backend::Offline(reason),
                pending: None,
                streaming: false,
                status: None,
            },
        }
    }

    fn spawn_pi() -> Result<PiSession, String> {
        let root = repo_root();
        let extension = root.join("agent/extensions/ucloud");
        let skills = root.join("agent/skills");
        let prompt = root.join("agent/prompts/system.md");
        if !extension.exists() {
            return Err(format!("找不到扩展目录 {}", extension.display()));
        }

        let tool = std::env::var("ADASH_TOOL").unwrap_or_else(|_| {
            // Same target dir as the app in a dev build.
            root.join("target/debug/adash-tool").display().to_string()
        });
        if !std::path::Path::new(&tool).exists() {
            return Err(format!(
                "找不到 adash-tool（{tool}），先 cargo build -p adash-tool"
            ));
        }

        let mut args = vec![
            "--no-builtin-tools".to_owned(),
            "-e".to_owned(),
            extension.display().to_string(),
            "--skill".to_owned(),
            skills.display().to_string(),
        ];
        if prompt.exists() {
            args.push("--append-system-prompt".to_owned());
            args.push(prompt.display().to_string());
        }
        let config = PiConfig {
            args,
            env: vec![("ADASH_TOOL".to_owned(), tool)],
            ..PiConfig::default()
        };
        PiSession::spawn(config).map_err(|error| format!("无法启动 pi：{error}"))
    }

    /// Whether the model is available.
    pub fn is_online(&self) -> bool {
        matches!(self.backend, Backend::Pi(_))
    }

    /// Why the app is offline.
    pub fn offline_reason(&self) -> Option<&str> {
        match &self.backend {
            Backend::Offline(reason) => Some(reason),
            Backend::Pi(_) => None,
        }
    }

    /// Send a prompt; returns the request id when pi accepted it.
    pub fn prompt(&mut self, text: &str) -> Option<String> {
        let Backend::Pi(session) = &mut self.backend else {
            return None;
        };
        match session.prompt(text) {
            Ok(id) => {
                self.streaming = true;
                self.status = Some("正在理解你的问题…".to_owned());
                Some(id)
            }
            Err(error) => {
                self.streaming = false;
                self.status = Some(format!("发送失败：{error}"));
                None
            }
        }
    }

    /// Answer an extension confirm request.
    pub fn answer_confirm(&mut self, request_id: &str, confirmed: bool) {
        if let Backend::Pi(session) = &self.backend {
            let _ = session.respond_confirm(request_id, confirmed);
        }
        self.pending = None;
    }

    /// Drain whatever pi has said since last frame.
    ///
    /// `text` receives streaming deltas; the returned value is `true` when the
    /// run finished (so the caller can stop polling).
    pub fn pump(&mut self, mut on_text: impl FnMut(&str), mut on_tool: impl FnMut(String)) -> bool {
        let Backend::Pi(session) = &self.backend else {
            return true;
        };
        let mut settled = false;
        for record in session.try_poll() {
            match record {
                PiRecord::Response(value) => {
                    if value.get("success").and_then(Value::as_bool) == Some(false) {
                        let error = value
                            .get("error")
                            .and_then(Value::as_str)
                            .unwrap_or("未知错误");
                        on_text(&format!("\n（pi 拒绝了这次请求：{error}）"));
                        settled = true;
                    }
                }
                PiRecord::UiRequest(value) => {
                    let method = value.get("method").and_then(Value::as_str).unwrap_or("");
                    if method == "confirm"
                        && value.get("title").and_then(Value::as_str) == Some("adash:apply-plan")
                    {
                        let request_id = value
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned();
                        let plan_id = value
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned();
                        self.pending = Some(ConfirmRequest {
                            request_id,
                            plan_id,
                        });
                        self.status = Some("等待你确认计划…".to_owned());
                    }
                }
                PiRecord::Event(value) => {
                    let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
                    match kind {
                        "message_update" => {
                            let event = value.get("assistantMessageEvent").unwrap_or(&value);
                            match event.get("type").and_then(Value::as_str) {
                                Some("text_delta") => {
                                    if let Some(delta) = event.get("delta").and_then(Value::as_str)
                                    {
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
                        "tool_execution_start" => {
                            let name = value
                                .get("toolName")
                                .and_then(Value::as_str)
                                .unwrap_or("tool");
                            self.status = Some(format!("调用 {name}…"));
                        }
                        "tool_execution_end" => {
                            let name = value
                                .get("toolName")
                                .and_then(Value::as_str)
                                .unwrap_or("tool");
                            let failed =
                                value.get("isError").and_then(Value::as_bool) == Some(true);
                            let millis = value.get("durationMs").and_then(Value::as_u64);
                            let mark = if failed { "!" } else { "✓" };
                            let timing = millis.map_or_else(String::new, |ms| format!(" · {ms}ms"));
                            on_tool(format!("{mark} {name}{timing}"));
                            self.status = Some(format!("{name} 完成"));
                        }
                        "agent_settled" | "agent_end" => {
                            settled = true;
                        }
                        "error" => {
                            let reason = value
                                .get("error")
                                .and_then(Value::as_str)
                                .unwrap_or("模型出错");
                            on_text(&format!("\n（{reason}）"));
                            settled = true;
                        }
                        _ => {}
                    }
                }
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
        if let Backend::Pi(session) = &self.backend {
            let _ = session.abort();
        }
        self.streaming = false;
        self.status = None;
    }
}

/// The repository root, so pi can be pointed at this repo's extension and skills.
pub fn repo_root() -> PathBuf {
    if let Ok(root) = std::env::var("ADASH_ROOT") {
        return PathBuf::from(root);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
