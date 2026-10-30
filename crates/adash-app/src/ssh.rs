//! SSH session state and actions for the 「主机」 pages.
//!
//! All network work happens on `adash-ssh` threads. The UI only sends requests
//! and drains replies on a timer, so a slow host never freezes a frame.

use crate::app::{Adash, Page};
use adash_ssh::{
    Authentication, ForwardHandle, ForwardRule, Host, Identity, IdentityKind, MonitorSample,
    ProfileStore, RemoteFile, Reply, Request, Snippet, Terminal, Worker,
};
use gpui::base::input::{InputState, TextareaState};
use gpui::prelude::*;
use gpui::*;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

/// Text fields for adding a host.
pub struct HostForm {
    pub name: Entity<InputState>,
    pub address: Entity<InputState>,
    pub port: Entity<InputState>,
    pub user: Entity<InputState>,
    pub key_path: Entity<InputState>,
    pub secret: Entity<InputState>,
}
/// Text fields for adding an identity.
pub struct IdentityForm {
    pub name: Entity<InputState>,
    pub key_path: Entity<InputState>,
    pub secret: Entity<InputState>,
}
/// Text fields for adding a snippet.
pub struct SnippetForm {
    pub name: Entity<InputState>,
    pub command: Entity<InputState>,
}
/// Text fields for adding a forward rule.
pub struct ForwardForm {
    pub name: Entity<InputState>,
    pub bind_port: Entity<InputState>,
    pub target_host: Entity<InputState>,
    pub target_port: Entity<InputState>,
}
impl HostForm {
    pub fn new(window: &mut Window, cx: &mut Context<Adash>) -> Self {
        let mut field = |placeholder: &'static str, cx: &mut Context<Adash>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
        };
        Self {
            name: field("名称，例如 web-01", cx),
            address: field("地址或域名", cx),
            port: field("22", cx),
            user: field("root", cx),
            key_path: field("私钥路径（可选）", cx),
            secret: cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(true)
                    .placeholder("密码或私钥口令（可选）")
            }),
        }
    }
}
impl IdentityForm {
    pub fn new(window: &mut Window, cx: &mut Context<Adash>) -> Self {
        let mut field = |placeholder: &'static str, cx: &mut Context<Adash>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
        };
        Self {
            name: field("身份名称", cx),
            key_path: field("私钥路径（私钥身份必填）", cx),
            secret: cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(true)
                    .placeholder("密码或口令（写入系统钥匙串）")
            }),
        }
    }
}
impl SnippetForm {
    pub fn new(window: &mut Window, cx: &mut Context<Adash>) -> Self {
        let mut field = |placeholder: &'static str, cx: &mut Context<Adash>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
        };
        Self {
            name: field("片段名称", cx),
            command: field("要执行的命令", cx),
        }
    }
}
impl ForwardForm {
    pub fn new(window: &mut Window, cx: &mut Context<Adash>) -> Self {
        let mut field = |placeholder: &'static str, cx: &mut Context<Adash>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
        };
        Self {
            name: field("规则名称", cx),
            bind_port: field("本地/远程端口", cx),
            target_host: field("目标主机，例如 127.0.0.1", cx),
            target_port: field("目标端口", cx),
        }
    }
}

/// Everything owned by one open host session.
pub struct SshSession {
    pub host: Host,
    pub worker: Option<Worker>,
    pub terminal: Option<Terminal>,
    pub pending_key: Option<String>,
    pub status: Option<String>,
    pub connecting: bool,
    pub connected: bool,
    pub last_monitor: std::time::Instant,
    pub files: Option<Vec<RemoteFile>>,
    pub directory: PathBuf,
    pub file_error: Option<String>,
    pub monitor: Option<MonitorSample>,
    pub previous_monitor: Option<MonitorSample>,
    pub monitor_error: Option<String>,
    pub exec_output: Option<String>,
    pub forward: Option<ForwardHandle>,
    pub forward_rx: Option<Receiver<Result<ForwardHandle, String>>>,
    pub forward_rule: Option<String>,
    pub focus: FocusHandle,
    pub editor: Entity<TextareaState>,
    pub editor_path: Option<PathBuf>,
    pub pending_editor_text: Option<String>,
    pub terminal_rx: Option<Receiver<Result<Terminal, String>>>,
}
impl SshSession {
    pub fn new(host: Host, window: &mut Window, cx: &mut Context<Adash>) -> Self {
        let editor = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(6, 24)
                .placeholder("连接主机后可编辑文件")
        });
        Self {
            host,
            worker: None,
            terminal: None,
            pending_key: None,
            status: None,
            connecting: false,
            connected: false,
            last_monitor: std::time::Instant::now(),
            files: None,
            directory: PathBuf::from("."),
            file_error: None,
            monitor: None,
            previous_monitor: None,
            monitor_error: None,
            exec_output: None,
            forward: None,
            forward_rx: None,
            forward_rule: None,
            focus: cx.focus_handle(),
            editor,
            editor_path: None,
            pending_editor_text: None,
            terminal_rx: None,
        }
    }
}

/// Slug for new profiles: readable, unique enough, and safe in a file.
fn slug(label: &str) -> String {
    let base: String = label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = base.trim_matches('-').to_owned();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.subsec_millis());
    format!(
        "{}-{stamp}",
        if trimmed.is_empty() { "host" } else { &trimmed }
    )
}

fn clear(value: &Entity<InputState>, window: &mut Window, cx: &mut Context<Adash>) {
    value.update(cx, |state, cx| state.set_value("", window, cx));
}

impl Adash {
    pub fn profiles_or_default(&self) -> &ProfileStore {
        &self.profiles
    }

    /// Add a host from the form. A password or path becomes an identity.
    pub fn add_host(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.host_form.name.read(cx).value().trim().to_owned();
        let address = self.host_form.address.read(cx).value().trim().to_owned();
        let port = self
            .host_form
            .port
            .read(cx)
            .value()
            .trim()
            .parse::<u16>()
            .unwrap_or(22);
        let username = self.host_form.user.read(cx).value().trim().to_owned();
        let key_path = self.host_form.key_path.read(cx).value().trim().to_owned();
        let secret = self.host_form.secret.read(cx).value().to_string();
        let username = if username.is_empty() {
            "root".to_owned()
        } else {
            username
        };
        let name = if name.is_empty() {
            address.clone()
        } else {
            name
        };
        let host = Host {
            id: slug(&name),
            name,
            address,
            port,
            username,
            identity: None,
        };
        if host.validate().is_err() {
            self.notice = Some("主机地址和用户名不能为空".to_owned());
            cx.notify();
            return;
        }
        let id = host.id.clone();
        self.profiles.data.hosts.insert(id.clone(), host);
        if !key_path.is_empty() || !secret.is_empty() {
            let identity_id = format!("identity-{id}");
            let kind = if key_path.is_empty() {
                IdentityKind::Password
            } else {
                IdentityKind::PrivateKey
            };
            if !secret.is_empty() && adash_ssh::store_secret(&identity_id, &secret).is_err() {
                self.notice = Some("已保存主机，但密码未能写入系统钥匙串".to_owned());
            }
            self.profiles.data.identities.insert(
                identity_id.clone(),
                Identity {
                    id: identity_id.clone(),
                    name: format!("{id} 身份"),
                    kind,
                    key_path: (!key_path.is_empty()).then(|| PathBuf::from(key_path)),
                },
            );
            if let Some(host) = self.profiles.data.hosts.get_mut(&id) {
                host.identity = Some(identity_id);
            }
        }
        let _ = self.profiles.save();
        for field in [
            &self.host_form.name,
            &self.host_form.address,
            &self.host_form.port,
            &self.host_form.user,
            &self.host_form.key_path,
            &self.host_form.secret,
        ] {
            clear(field, window, cx);
        }
        self.notice = Some("主机已保存".to_owned());
        cx.notify();
    }

    pub fn delete_host(&mut self, id: &str, cx: &mut Context<Self>) {
        self.profiles.data.hosts.remove(id);
        self.sessions.retain(|session| session.host.id != id);
        let _ = self.profiles.save();
        if matches!(self.page, Page::Session(_)) {
            self.page = Page::Hosts;
        }
        cx.notify();
    }

    /// Add an identity; the secret goes to the keyring, never to JSON.
    pub fn add_identity(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.identity_form.name.read(cx).value().trim().to_owned();
        let path = self
            .identity_form
            .key_path
            .read(cx)
            .value()
            .trim()
            .to_owned();
        let secret = self.identity_form.secret.read(cx).value().to_string();
        let kind = self.identity_kind.clone();
        if name.is_empty() {
            self.notice = Some("身份需要一个名称".to_owned());
            cx.notify();
            return;
        }
        if kind == IdentityKind::PrivateKey && path.is_empty() {
            self.notice = Some("私钥身份需要文件路径".to_owned());
            cx.notify();
            return;
        }
        let id = format!("id-{}", slug(&name));
        if !secret.is_empty()
            && let Err(error) = adash_ssh::store_secret(&id, &secret)
        {
            self.notice = Some(format!("无法写入系统钥匙串：{error:#}"));
            cx.notify();
            return;
        }
        if let Some(existing) = self.profiles.data.identities.get(&id)
            && existing.kind != kind
        {
            let _ = adash_ssh::delete_secret(&id);
        }
        self.profiles.data.identities.insert(
            id.clone(),
            Identity {
                id,
                name,
                kind,
                key_path: (!path.is_empty()).then(|| PathBuf::from(path)),
            },
        );
        let _ = self.profiles.save();
        for field in [
            &self.identity_form.name,
            &self.identity_form.key_path,
            &self.identity_form.secret,
        ] {
            clear(field, window, cx);
        }
        cx.notify();
    }

    pub fn delete_identity(&mut self, id: &str, cx: &mut Context<Self>) {
        self.profiles.data.identities.remove(id);
        let _ = adash_ssh::delete_secret(id);
        for host in self.profiles.data.hosts.values_mut() {
            if host.identity.as_deref() == Some(id) {
                host.identity = None;
            }
        }
        let _ = self.profiles.save();
        cx.notify();
    }

    pub fn add_snippet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.snippet_form.name.read(cx).value().trim().to_owned();
        let command = self.snippet_form.command.read(cx).value().trim().to_owned();
        if name.is_empty() || command.is_empty() {
            self.notice = Some("片段需要名称和命令".to_owned());
            cx.notify();
            return;
        }
        let id = format!("snippet-{}", slug(&name));
        self.profiles
            .data
            .snippets
            .insert(id.clone(), Snippet { id, name, command });
        let _ = self.profiles.save();
        for field in [&self.snippet_form.name, &self.snippet_form.command] {
            clear(field, window, cx);
        }
        cx.notify();
    }

    pub fn delete_snippet(&mut self, id: &str, cx: &mut Context<Self>) {
        self.profiles.data.snippets.remove(id);
        let _ = self.profiles.save();
        cx.notify();
    }

    pub fn add_forward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.forward_form.name.read(cx).value().trim().to_owned();
        let bind_port = self
            .forward_form
            .bind_port
            .read(cx)
            .value()
            .trim()
            .parse::<u16>()
            .unwrap_or(0);
        let target_host = self
            .forward_form
            .target_host
            .read(cx)
            .value()
            .trim()
            .to_owned();
        let target_port = self
            .forward_form
            .target_port
            .read(cx)
            .value()
            .trim()
            .parse::<u16>()
            .unwrap_or(0);
        if bind_port == 0 || target_port == 0 || target_host.is_empty() {
            self.notice = Some("转发需要本地端口、目标主机和目标端口".to_owned());
            cx.notify();
            return;
        }
        let id = format!("forward-{}", slug(&name));
        self.profiles.data.forwards.insert(
            id.clone(),
            ForwardRule {
                id,
                name,
                host_id: String::new(),
                direction: self.forward_direction,
                bind_port,
                target_host,
                target_port,
            },
        );
        let _ = self.profiles.save();
        for field in [
            &self.forward_form.name,
            &self.forward_form.bind_port,
            &self.forward_form.target_host,
            &self.forward_form.target_port,
        ] {
            clear(field, window, cx);
        }
        cx.notify();
    }

    pub fn delete_forward(&mut self, id: &str, cx: &mut Context<Self>) {
        self.profiles.data.forwards.remove(id);
        let _ = self.profiles.save();
        cx.notify();
    }

    /// Open or focus a session for a saved host.
    pub fn open_host(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.sessions.iter().position(|s| s.host.id == id) {
            self.page = Page::Session(index);
            self.active_session = Some(index);
            cx.notify();
            return;
        }
        let Some(host) = self.profiles.data.hosts.get(id).cloned() else {
            return;
        };
        let session = SshSession::new(host, window, cx);
        self.sessions.push(session);
        self.profiles
            .remember(id, self.store.data.settings.recent_limit);
        let _ = self.profiles.save();
        let index = self.sessions.len() - 1;
        self.page = Page::Session(index);
        self.active_session = Some(index);
        self.connect(index, cx);
        cx.notify();
    }

    fn known_hosts(&self) -> std::collections::BTreeMap<String, String> {
        self.profiles.data.known_hosts.clone()
    }

    /// Identity lookup is done before any session is mutably borrowed.
    fn authentication_for(&self, host: &Host) -> Result<Authentication, String> {
        let identity: Option<Identity> = host
            .identity
            .as_ref()
            .and_then(|id| self.profiles.data.identities.get(id))
            .cloned();
        Authentication::from_identity(identity.as_ref()).map_err(|error| format!("{error:#}"))
    }

    /// Start the data worker. Fingerprints are checked before authentication.
    pub fn connect(&mut self, index: usize, cx: &mut Context<Self>) {
        let known = self.known_hosts();
        let timeout = Duration::from_secs(self.store.data.settings.connect_timeout.clamp(2, 120));
        let Some(host) = self.sessions.get(index).map(|session| session.host.clone()) else {
            return;
        };
        let auth = match self.authentication_for(&host) {
            Ok(auth) => auth,
            Err(error) => {
                if let Some(session) = self.sessions.get_mut(index) {
                    session.status = Some(error);
                }
                cx.notify();
                return;
            }
        };
        if let Some(session) = self.sessions.get_mut(index) {
            session.connecting = true;
            session.status = Some("正在连接…".to_owned());
            session.worker = Some(Worker::spawn(host, auth, known, timeout));
        }
        cx.notify();
    }

    pub fn trust_host_key(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(session) = self.sessions.get_mut(index) else {
            return;
        };
        let Some(fingerprint) = session.pending_key.clone() else {
            return;
        };
        self.profiles
            .data
            .known_hosts
            .insert(session.host.endpoint(), fingerprint);
        let _ = self.profiles.save();
        session.pending_key = None;
        session.status = Some("已保存主机指纹，正在连接…".to_owned());
        self.connect(index, cx);
    }

    /// Start a PTY. The VT parser lives in `adash-ssh`; the GUI draws its screen.
    pub fn connect_terminal(&mut self, index: usize, cx: &mut Context<Self>) {
        let known = self.known_hosts();
        let timeout = Duration::from_secs(self.store.data.settings.connect_timeout.clamp(2, 120));
        let Some(host) = self.sessions.get(index).map(|session| session.host.clone()) else {
            return;
        };
        if self
            .sessions
            .get(index)
            .is_some_and(|session| session.terminal.is_some() || session.terminal_rx.is_some())
        {
            return;
        }
        let auth = match self.authentication_for(&host) {
            Ok(auth) => auth,
            Err(error) => {
                if let Some(session) = self.sessions.get_mut(index) {
                    session.status = Some(error);
                }
                cx.notify();
                return;
            }
        };
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("adash-terminal-connect".into())
            .spawn(move || {
                let result = adash_ssh::Connection::open(&host, auth, &known, timeout)
                    .and_then(|connection| Terminal::start(connection, 32, 120))
                    .map_err(|error| format!("{error:#}"));
                let _ = tx.send(result);
            })
            .ok();
        if let Some(session) = self.sessions.get_mut(index) {
            session.terminal_rx = Some(rx);
            session.status = Some("正在打开终端…".to_owned());
        }
        cx.notify();
    }

    pub fn refresh_files(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(session) = self.sessions.get(index) else {
            return;
        };
        let directory = session.directory.clone();
        if let Some(worker) = &session.worker {
            let _ = worker.send(Request::List(directory));
        }
        if let Some(session) = self.sessions.get_mut(index) {
            session.file_error = None;
        }
        cx.notify();
    }

    pub fn open_file(&mut self, index: usize, path: PathBuf, cx: &mut Context<Self>) {
        let Some(session) = self.sessions.get(index) else {
            return;
        };
        if let Some(worker) = &session.worker {
            let _ = worker.send(Request::Open(path));
        }
        cx.notify();
    }

    pub fn save_file(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(session) = self.sessions.get(index) else {
            return;
        };
        let Some(path) = session.editor_path.clone() else {
            return;
        };
        let text = session.editor.read(cx).value().to_string();
        if let Some(worker) = &session.worker {
            let _ = worker.send(Request::Write(path, text));
        }
        cx.notify();
    }

    pub fn run_on_host(&mut self, index: usize, command: String, cx: &mut Context<Self>) {
        let Some(session) = self.sessions.get_mut(index) else {
            return;
        };
        if command.trim().is_empty() {
            return;
        }
        if let Some(worker) = &session.worker {
            let _ = worker.send(Request::Exec(command));
            session.exec_output = Some("执行中…".to_owned());
        }
        cx.notify();
    }

    pub fn sample_monitor(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(session) = self.sessions.get(index) else {
            return;
        };
        if let Some(worker) = &session.worker {
            let _ = worker.send(Request::Monitor);
        }
        cx.notify();
    }

    pub fn toggle_forward(&mut self, index: usize, rule_id: &str, cx: &mut Context<Self>) {
        if let Some(session) = self.sessions.get_mut(index)
            && let Some(handle) = session.forward.take()
        {
            handle.stop();
            session.status = Some("端口转发已停止".to_owned());
            cx.notify();
            return;
        }
        let known = self.known_hosts();
        let timeout = Duration::from_secs(self.store.data.settings.connect_timeout.clamp(2, 120));
        let Some(rule) = self.profiles.data.forwards.get(rule_id).cloned() else {
            return;
        };
        let Some(host) = self.sessions.get(index).map(|session| session.host.clone()) else {
            return;
        };
        let auth = match self.authentication_for(&host) {
            Ok(auth) => auth,
            Err(error) => {
                if let Some(session) = self.sessions.get_mut(index) {
                    session.status = Some(error);
                }
                cx.notify();
                return;
            }
        };
        let (tx, rx) = mpsc::channel();
        let rule_id = rule_id.to_owned();
        std::thread::spawn(move || {
            let outcome = adash_ssh::Connection::open(&host, auth, &known, timeout)
                .and_then(|connection| ForwardHandle::start(connection, rule))
                .map_err(|error| format!("{error:#}"));
            let _ = tx.send(outcome);
        });
        if let Some(session) = self.sessions.get_mut(index) {
            session.forward_rx = Some(rx);
            session.forward_rule = Some(rule_id);
            session.status = Some("正在启动转发…".to_owned());
        }
        cx.notify();
    }

    /// Pick a local file and stream it into the current remote directory. An
    /// existing remote name is refused by the SFTP layer, never overwritten.
    pub fn upload_pick(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(session) = self.sessions.get(index) else {
            return;
        };
        let directory = session.directory.clone();
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("选择要上传的文件".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(local) = paths.into_iter().next() else {
                return;
            };
            let Some(name) = local.file_name().map(|name| name.to_os_string()) else {
                return;
            };
            let remote = directory.join(name);
            let _ = this.update(cx, |this, cx| {
                let Some(session) = this.sessions.get(index) else {
                    return;
                };
                let Some(worker) = &session.worker else {
                    return;
                };
                let _ = worker.send(Request::Upload(local, remote));
                if let Some(session) = this.sessions.get_mut(index) {
                    session.status = Some("正在上传…".to_owned());
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Pick a local destination for a remote file.
    pub fn download_pick(&mut self, index: usize, remote: PathBuf, cx: &mut Context<Self>) {
        let Some(name) = remote
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
        else {
            return;
        };
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let receiver = cx.prompt_for_new_path(&home, Some(&name));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(local))) = receiver.await else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                let Some(session) = this.sessions.get(index) else {
                    return;
                };
                let Some(worker) = &session.worker else {
                    return;
                };
                let _ = worker.send(Request::Download(remote, local));
                if let Some(session) = this.sessions.get_mut(index) {
                    session.status = Some("正在下载…".to_owned());
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn confirm_remote_delete(&mut self, cx: &mut Context<Self>) {
        let Some((index, path)) = self.pending_remote_delete.take() else {
            return;
        };
        if let Some(session) = self.sessions.get(index)
            && let Some(worker) = &session.worker
        {
            let _ = worker.send(Request::Remove(path));
        }
        if let Some(session) = self.sessions.get_mut(index) {
            session.status = Some("正在删除…".to_owned());
        }
        cx.notify();
    }

    /// Run one snippet on every selected host, in parallel. Each host opens its
    /// own connection, so results arrive independently and one failure does not
    /// cancel the rest.
    pub fn run_snippet_batch(&mut self, command: String, cx: &mut Context<Self>) {
        if self.batch_rx.is_some() {
            self.notice = Some("上一批还在执行".to_owned());
            cx.notify();
            return;
        }
        let selected: Vec<Host> = self
            .selected_hosts
            .iter()
            .filter_map(|id| self.profiles.data.hosts.get(id).cloned())
            .collect();
        if selected.is_empty() {
            self.notice = Some("先在「主机」页选择要批量执行的主机".to_owned());
            cx.notify();
            return;
        }
        let (tx, rx) = mpsc::channel();
        let timeout = Duration::from_secs(self.store.data.settings.connect_timeout.clamp(2, 120));
        let known = self.known_hosts();
        for host in &selected {
            let identity = host
                .identity
                .as_ref()
                .and_then(|id| self.profiles.data.identities.get(id))
                .cloned();
            let host = host.clone();
            let known = known.clone();
            let tx = tx.clone();
            let command = command.clone();
            std::thread::spawn(move || {
                let result = (|| -> Result<adash_ssh::ExecResult, String> {
                    let auth = Authentication::from_identity(identity.as_ref())
                        .map_err(|error| format!("{error:#}"))?;
                    let connection = adash_ssh::Connection::open(&host, auth, &known, timeout)
                        .map_err(|error| format!("{error:#}"))?;
                    connection
                        .execute(&command)
                        .map_err(|error| format!("{error:#}"))
                })();
                let _ = tx.send((host.name, result));
            });
        }
        self.batch_output.clear();
        self.batch_running = selected.len();
        self.batch_rx = Some(rx);
        self.notice = Some(format!("正在 {} 台主机上执行", selected.len()));
        cx.notify();
    }

    pub fn close_session(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.store.data.settings.confirm_close
            && self
                .sessions
                .get(index)
                .is_some_and(|session| session.connected || session.terminal.is_some())
        {
            self.pending_close = Some(index);
            cx.notify();
            return;
        }
        self.finish_close_session(index, cx);
    }

    pub fn finish_close_session(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.sessions.len() {
            return;
        }
        let mut session = self.sessions.remove(index);
        if let Some(terminal) = session.terminal.take() {
            terminal.close();
        }
        if let Some(forward) = session.forward.take() {
            forward.stop();
        }
        if let Some(worker) = session.worker.take() {
            let _ = worker.send(Request::Close);
        }
        self.active_session = None;
        self.page = Page::Hosts;
        self.pending_close = None;
        cx.notify();
    }

    /// Drain worker replies and PTY output. Called from one timer task.
    pub fn poll_ssh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut changed = false;
        let mut start_terminals = Vec::new();
        let interval =
            Duration::from_secs(self.store.data.settings.monitor_interval.clamp(5, 3600));
        for (index, session) in self.sessions.iter_mut().enumerate() {
            if let Some(rx) = &session.forward_rx {
                match rx.try_recv() {
                    Ok(Ok(handle)) => {
                        session.status =
                            Some(format!("转发已启动：127.0.0.1:{}", handle.bound_port));
                        session.forward = Some(handle);
                        session.forward_rx = None;
                        changed = true;
                    }
                    Ok(Err(error)) => {
                        session.forward_rx = None;
                        session.forward_rule = None;
                        session.status = Some(format!("转发失败：{error}"));
                        changed = true;
                    }
                    Err(TryRecvError::Disconnected) => {
                        session.forward_rx = None;
                        session.status = Some("转发线程已退出".to_owned());
                        changed = true;
                    }
                    Err(TryRecvError::Empty) => {}
                }
            }
            if let Some(rx) = &session.terminal_rx {
                match rx.try_recv() {
                    Ok(Ok(terminal)) => {
                        session.terminal = Some(terminal);
                        session.terminal_rx = None;
                        session.status = Some("终端已连接".to_owned());
                        changed = true;
                    }
                    Ok(Err(error)) => {
                        session.terminal_rx = None;
                        session.status = Some(error);
                        changed = true;
                    }
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => {
                        session.terminal_rx = None;
                        session.status = Some("终端连接线程已退出".to_owned());
                        changed = true;
                    }
                }
            }
            if let Some(terminal) = &session.terminal {
                if terminal
                    .changed
                    .swap(false, std::sync::atomic::Ordering::AcqRel)
                {
                    changed = true;
                }
                let status = terminal.status.lock().unwrap().clone();
                if status != "已连接" && session.status.as_deref() != Some(status.as_str()) {
                    session.status = Some(status);
                    changed = true;
                }
            }
            let Some(worker) = &session.worker else {
                continue;
            };
            let replies: Vec<_> = worker.replies.try_iter().collect();
            let mut closed = false;
            if session.connected && session.last_monitor.elapsed() >= interval {
                let _ = worker.send(Request::Monitor);
                session.last_monitor = std::time::Instant::now();
            }
            for reply in replies {
                match reply {
                    Reply::Connected => {
                        session.connecting = false;
                        session.connected = true;
                        session.status = Some("已连接".to_owned());
                        let _ = worker.send(Request::List(session.directory.clone()));
                        let _ = worker.send(Request::Monitor);
                        if self.store.data.settings.auto_shell {
                            start_terminals.push(index);
                        }
                        changed = true;
                    }
                    Reply::Done => {
                        session.status = Some("操作完成".to_owned());
                        let _ = worker.send(Request::List(session.directory.clone()));
                        changed = true;
                    }
                    Reply::Files(files) => {
                        session.files = Some(files);
                        session.file_error = None;
                        changed = true;
                    }
                    Reply::Monitor(sample) => {
                        session.previous_monitor = session.monitor.replace(sample);
                        session.monitor_error = None;
                        changed = true;
                    }
                    Reply::Exec(result) => {
                        session.exec_output = Some(format!(
                            "退出码 {}\n{}{}",
                            result.exit_status, result.stdout, result.stderr
                        ));
                        changed = true;
                    }
                    Reply::Opened(path, text) => {
                        session.editor_path = Some(path);
                        session.pending_editor_text = Some(text);
                        changed = true;
                    }
                    Reply::Error(error) => {
                        session.connecting = false;
                        session.status = Some(error);
                        changed = true;
                    }
                    Reply::HostKeyUnknown(fingerprint) => {
                        session.connecting = false;
                        session.pending_key = Some(fingerprint.clone());
                        session.status = Some(format!("首次连接，请核对指纹 {fingerprint}"));
                        changed = true;
                    }
                    Reply::HostKeyChanged { expected, actual } => {
                        session.connecting = false;
                        session.status = Some(format!(
                            "主机指纹已变更，连接被拒绝。原指纹 {expected}，当前 {actual}"
                        ));
                        changed = true;
                    }
                    Reply::Closed => {
                        closed = true;
                        changed = true;
                    }
                }
            }
            if closed {
                session.worker = None;
            }
            if !closed
                && session
                    .worker
                    .as_ref()
                    .is_some_and(|worker| !worker.alive.load(std::sync::atomic::Ordering::Acquire))
            {
                session.worker = None;
                session.connected = false;
                session.connecting = false;
                if session.status.as_deref() == Some("正在连接…") {
                    session.status = Some("连接线程已退出".to_owned());
                }
                changed = true;
            }
            if let Some(text) = session.pending_editor_text.take() {
                session
                    .editor
                    .update(cx, |state, cx| state.set_value(text, window, cx));
                changed = true;
            }
        }
        for index in start_terminals {
            self.connect_terminal(index, cx);
        }
        if let Some(rx) = &self.batch_rx {
            for (host, result) in rx.try_iter() {
                let text = match result {
                    Ok(result) => {
                        format!("{host} · 退出码 {}\n{}", result.exit_status, result.stdout)
                    }
                    Err(error) => format!("{host} · 失败\n{error}"),
                };
                self.batch_output.push(text);
                self.batch_running = self.batch_running.saturating_sub(1);
                changed = true;
            }
            if self.batch_running == 0 {
                self.batch_rx = None;
            }
        }
        if changed {
            cx.notify();
        }
    }
}
