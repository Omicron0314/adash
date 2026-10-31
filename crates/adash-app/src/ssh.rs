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
    pub identity: Option<String>,
    pub editing: Option<String>,
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
    pub command: Entity<TextareaState>,
}
/// Text fields for adding a forward rule.
pub struct ForwardForm {
    pub host_id: Option<String>,
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
            identity: None,
            editing: None,
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
            command: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .auto_grow(3, 10)
                    .placeholder("要执行的命令，可输入多行")
            }),
        }
    }
}
impl ForwardForm {
    pub fn new(window: &mut Window, cx: &mut Context<Adash>) -> Self {
        let mut field = |placeholder: &'static str, cx: &mut Context<Adash>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
        };
        Self {
            host_id: None,
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
    /// Closed slots are retained so asynchronous operations cannot switch host
    /// when another session is removed from the sidebar.
    pub closed: bool,
    pub worker: Option<Worker>,
    pub terminal: Option<Terminal>,
    pub pending_key: Option<String>,
    pub status: Option<String>,
    pub connecting: bool,
    pub want_terminal: bool,
    pub connected: bool,
    pub last_monitor: std::time::Instant,
    pub files: Option<Vec<RemoteFile>>,
    pub directory: PathBuf,
    pub directory_input: Entity<InputState>,
    pub remote_name: Entity<InputState>,
    pub transfer: Option<(u64, u64)>,
    pub file_error: Option<String>,
    pub monitor: Option<MonitorSample>,
    pub previous_monitor: Option<MonitorSample>,
    pub monitor_error: Option<String>,
    pub exec_output: Option<String>,
    pub forwards: std::collections::BTreeMap<String, ForwardHandle>,
    pub forward_rx: std::collections::BTreeMap<String, Receiver<Result<ForwardHandle, String>>>,
    pub forward_errors: std::collections::BTreeMap<String, String>,
    pub focus: FocusHandle,
    pub terminal_input: Entity<crate::terminal_input::TerminalInput>,
    pub editor: Entity<TextareaState>,
    pub editor_path: Option<PathBuf>,
    pub pending_editor_text: Option<String>,
    pub editor_original: String,
    pub terminal_rx: Option<Receiver<Result<Terminal, String>>>,
}
impl SshSession {
    pub fn new(host: Host, window: &mut Window, cx: &mut Context<Adash>) -> Self {
        let editor = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(6, 24)
                .placeholder("连接主机后可编辑文件")
        });
        cx.observe(&editor, |_, _, cx| cx.notify()).detach();
        let directory_input = cx.new(|cx| InputState::new(window, cx));
        directory_input.update(cx, |state, cx| state.set_value(".", window, cx));
        let remote_name = cx.new(|cx| InputState::new(window, cx));
        let focus = cx.focus_handle();
        let terminal_input = cx.new(|_| crate::terminal_input::TerminalInput::new(focus.clone()));
        cx.observe(&terminal_input, |_, _, cx| cx.notify()).detach();
        Self {
            host,
            closed: false,
            worker: None,
            terminal: None,
            pending_key: None,
            status: None,
            connecting: false,
            want_terminal: false,
            connected: false,
            last_monitor: std::time::Instant::now(),
            files: None,
            directory: PathBuf::from("."),
            directory_input,
            remote_name,
            transfer: None,
            file_error: None,
            monitor: None,
            previous_monitor: None,
            monitor_error: None,
            exec_output: None,
            forwards: Default::default(),
            forward_rx: Default::default(),
            forward_errors: Default::default(),
            focus,
            terminal_input,
            editor,
            editor_path: None,
            pending_editor_text: None,
            editor_original: String::new(),
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
        .map_or(0, |duration| duration.as_nanos());
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

    fn update_profiles(
        &mut self,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut adash_ssh::Profiles),
    ) -> bool {
        if let Err(error) = self.profiles.update(edit) {
            self.notice = Some(format!("主机配置保存失败：{error:#}"));
            cx.notify();
            return false;
        }
        true
    }

    pub fn edit_host(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .sessions
            .iter()
            .any(|session| !session.closed && session.host.id == id)
        {
            self.notice = Some("请先关闭该主机会话，再编辑连接信息".into());
            cx.notify();
            return;
        }
        let Some(host) = self.profiles.data.hosts.get(id).cloned() else {
            return;
        };
        for (field, value) in [
            (&self.host_form.name, host.name),
            (&self.host_form.address, host.address),
            (&self.host_form.port, host.port.to_string()),
            (&self.host_form.user, host.username),
        ] {
            field.update(cx, |state, cx| state.set_value(value, window, cx));
        }
        clear(&self.host_form.key_path, window, cx);
        clear(&self.host_form.secret, window, cx);
        self.host_form.identity = host.identity;
        self.host_form.editing = Some(id.to_owned());
        cx.notify();
    }

    pub fn cancel_host_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        self.host_form.identity = None;
        self.host_form.editing = None;
        cx.notify();
    }

    /// Persist a complete target only after its credentials are stored. Failed
    /// keyring/config writes retain the form and never report success.
    pub fn add_host(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.host_form.name.read(cx).value().trim().to_owned();
        let address = self.host_form.address.read(cx).value().trim().to_owned();
        let port_text = self.host_form.port.read(cx).value();
        let port = if port_text.trim().is_empty() {
            22
        } else {
            match port_text.trim().parse::<u16>() {
                Ok(port) if port > 0 => port,
                _ => {
                    self.notice = Some("端口必须为 1–65535 的整数".into());
                    cx.notify();
                    return;
                }
            }
        };
        let username = self.host_form.user.read(cx).value().trim().to_owned();
        let key_path = self.host_form.key_path.read(cx).value().trim().to_owned();
        let secret = self.host_form.secret.read(cx).value().to_string();
        let name = if name.is_empty() {
            address.clone()
        } else {
            name
        };
        let id = self
            .host_form
            .editing
            .clone()
            .unwrap_or_else(|| slug(&name));
        let mut host = Host {
            id: id.clone(),
            name,
            address,
            port,
            username: if username.is_empty() {
                "root".into()
            } else {
                username
            },
            identity: self.host_form.identity.clone(),
        };
        if let Err(error) = host.validate() {
            self.notice = Some(error.to_string());
            cx.notify();
            return;
        }
        let identity = if !key_path.is_empty() || !secret.is_empty() {
            // A fresh identity id: editing credentials must not overwrite the
            // previous keyring entry before the host profile is persisted.
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| duration.as_nanos());
            let identity_id = format!("identity-{}-{stamp}", slug(&id));
            if !secret.is_empty()
                && let Err(error) = adash_ssh::store_secret(&identity_id, &secret)
            {
                self.notice = Some(format!("密码未能写入系统钥匙串：{error:#}"));
                cx.notify();
                return;
            }
            host.identity = Some(identity_id.clone());
            Some(Identity {
                id: identity_id,
                name: format!("{} 身份", host.name),
                kind: if key_path.is_empty() {
                    IdentityKind::Password
                } else {
                    IdentityKind::PrivateKey
                },
                key_path: (!key_path.is_empty()).then(|| PathBuf::from(key_path)),
            })
        } else {
            None
        };
        let cleanup = identity.as_ref().map(|identity| identity.id.clone());
        let previous_identity = self
            .host_form
            .editing
            .as_ref()
            .and_then(|editing| self.profiles.data.hosts.get(editing))
            .and_then(|host| host.identity.clone());
        let new_identity = host.identity.clone();
        if !self.update_profiles(cx, |profiles| {
            if let Some(identity) = identity {
                profiles.identities.insert(identity.id.clone(), identity);
            }
            profiles.hosts.insert(id, host);
            if let Some(old) = &previous_identity
                && new_identity.as_deref() != Some(old)
                && old.starts_with("identity-")
                && !profiles
                    .hosts
                    .values()
                    .any(|host| host.identity.as_deref() == Some(old))
            {
                profiles.identities.remove(old);
            }
        }) {
            if let Some(id) = cleanup {
                let _ = adash_ssh::delete_secret(&id);
            }
            return;
        }
        if let Some(old) = previous_identity
            && new_identity.as_deref() != Some(&old)
            && old.starts_with("identity-")
            && !self
                .profiles
                .data
                .hosts
                .values()
                .any(|host| host.identity.as_deref() == Some(&old))
        {
            let _ = adash_ssh::delete_secret(&old);
        }
        self.cancel_host_edit(window, cx);
        self.notice = Some("主机已保存".into());
        cx.notify();
    }

    pub fn delete_host(&mut self, id: &str, cx: &mut Context<Self>) {
        if self
            .sessions
            .iter()
            .any(|session| !session.closed && session.host.id == id)
        {
            self.notice = Some("请先关闭该主机会话，再删除主机配置".into());
            cx.notify();
            return;
        }
        let old_identity = self
            .profiles
            .data
            .hosts
            .get(id)
            .and_then(|host| host.identity.clone());
        if self.update_profiles(cx, |profiles| {
            profiles.hosts.remove(id);
            profiles.forwards.retain(|_, rule| rule.host_id != id);
            profiles.recent.retain(|value| value != id);
            if let Some(old) = &old_identity
                && old.starts_with("identity-")
                && !profiles
                    .hosts
                    .values()
                    .any(|host| host.identity.as_deref() == Some(old))
            {
                profiles.identities.remove(old);
            }
        }) {
            self.selected_hosts.remove(id);
        } else {
            cx.notify();
            return;
        }
        if let Some(old) = old_identity
            && old.starts_with("identity-")
            && !self
                .profiles
                .data
                .hosts
                .values()
                .any(|host| host.identity.as_deref() == Some(&old))
            && let Err(error) = adash_ssh::delete_secret(&old)
        {
            self.notice = Some(format!("主机已删除，但钥匙串清理失败：{error:#}"));
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
        if kind == IdentityKind::Password && secret.is_empty() {
            self.notice = Some("密码身份需要填写密码".into());
            cx.notify();
            return;
        }
        if kind == IdentityKind::Agent && !secret.is_empty() {
            self.notice = Some("SSH Agent 身份无需密码，请先清空密码字段".into());
            cx.notify();
            return;
        }
        let id = format!("id-{}", slug(&name));
        let existing = self.profiles.data.identities.get(&id).cloned();
        let previous_secret = existing
            .as_ref()
            .and_then(|_| adash_ssh::load_secret(&id).ok().flatten());
        if !secret.is_empty()
            && let Err(error) = adash_ssh::store_secret(&id, &secret)
        {
            self.notice = Some(format!("无法写入系统钥匙串：{error:#}"));
            cx.notify();
            return;
        }
        let identity = Identity {
            id: id.clone(),
            name,
            kind: kind.clone(),
            key_path: (!path.is_empty()).then(|| PathBuf::from(path)),
        };
        if !self.update_profiles(cx, |profiles| {
            profiles.identities.insert(id.clone(), identity);
        }) {
            if !secret.is_empty() {
                let restored = match &previous_secret {
                    Some(previous) => adash_ssh::store_secret(&id, previous),
                    None => adash_ssh::delete_secret(&id),
                };
                if let Err(error) = restored {
                    self.notice =
                        Some(format!("身份配置未保存，且旧钥匙串条目恢复失败：{error:#}"));
                }
            }
            return;
        }
        // Remove a stale password only after the new profile is durable. A
        // private-key identity with a blank passphrase must not inherit the
        // previous password entry.
        if secret.is_empty()
            && (kind == IdentityKind::Agent
                || existing
                    .as_ref()
                    .is_some_and(|identity| identity.kind != kind))
            && let Err(error) = adash_ssh::delete_secret(&id)
        {
            self.notice = Some(format!("身份已保存，但旧钥匙串条目清理失败：{error:#}"));
            cx.notify();
            return;
        }
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
        if self
            .profiles
            .data
            .hosts
            .values()
            .any(|host| host.identity.as_deref() == Some(id))
        {
            self.notice = Some("仍有主机使用该身份，请先在主机编辑中更换身份".into());
            cx.notify();
            return;
        }
        if !self.update_profiles(cx, |profiles| {
            profiles.identities.remove(id);
        }) {
            return;
        }
        if let Err(error) = adash_ssh::delete_secret(id) {
            self.notice = Some(format!("身份已移除，钥匙串清理失败：{error:#}"));
        }
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
        if !self.update_profiles(cx, |profiles| {
            profiles
                .snippets
                .insert(id.clone(), Snippet { id, name, command });
        }) {
            return;
        }
        clear(&self.snippet_form.name, window, cx);
        self.snippet_form
            .command
            .update(cx, |state, cx| state.set_value("", window, cx));
        cx.notify();
    }

    pub fn delete_snippet(&mut self, id: &str, cx: &mut Context<Self>) {
        self.update_profiles(cx, |profiles| {
            profiles.snippets.remove(id);
        });
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
        let host_id = self.forward_form.host_id.clone().or_else(|| {
            self.active_session.and_then(|index| {
                self.sessions
                    .get(index)
                    .filter(|session| !session.closed)
                    .map(|session| session.host.id.clone())
            })
        });
        let Some(host_id) = host_id.filter(|id| self.profiles.data.hosts.contains_key(id)) else {
            self.notice = Some("请选择转发连接的主机".into());
            cx.notify();
            return;
        };
        let id = format!("forward-{}", slug(&name));
        let direction = self.forward_direction;
        if !self.update_profiles(cx, |profiles| {
            profiles.forwards.insert(
                id.clone(),
                ForwardRule {
                    id,
                    name,
                    host_id,
                    direction,
                    bind_port,
                    target_host,
                    target_port,
                },
            );
        }) {
            return;
        }
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
        if !self.update_profiles(cx, |profiles| {
            profiles.forwards.remove(id);
        }) {
            return;
        }
        for session in &mut self.sessions {
            session.forwards.remove(id);
            session.forward_rx.remove(id);
            session.forward_errors.remove(id);
        }
        cx.notify();
    }

    /// Open or focus a session for a saved host.
    pub fn open_host(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self
            .sessions
            .iter()
            .position(|s| !s.closed && s.host.id == id)
        {
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
    fn authentication_for(&self, host: &Host) -> Result<Option<Identity>, String> {
        match host.identity.as_ref() {
            Some(id) => Ok(Some(
                self.profiles
                    .data
                    .identities
                    .get(id)
                    .cloned()
                    .ok_or_else(|| "登录身份已不存在，请重新选择".to_owned())?,
            )),
            None => Ok(None),
        }
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
            if session.closed || session.connecting {
                return;
            }
            session.terminal_input.update(cx, |input, _| input.reset());
            session.terminal = None;
            session.terminal_rx = None;
            session.forwards.clear();
            session.forward_rx.clear();
            session.forward_errors.clear();
            session.connected = false;
            session.pending_key = None;
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
        let previous = self.profiles.data.known_hosts.clone();
        self.profiles
            .data
            .known_hosts
            .insert(session.host.endpoint(), fingerprint);
        if let Err(error) = self.profiles.save() {
            self.profiles.data.known_hosts = previous;
            self.notice = Some(format!("无法保存主机指纹：{error:#}"));
            cx.notify();
            return;
        }
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
        if !self
            .sessions
            .get(index)
            .is_some_and(|session| session.connected)
        {
            if let Some(session) = self.sessions.get_mut(index) {
                session.want_terminal = true;
            }
            self.connect(index, cx);
            return;
        }
        if self.sessions.get(index).is_some_and(|session| {
            session
                .terminal
                .as_ref()
                .is_some_and(Terminal::is_connected)
                || session.terminal_rx.is_some()
        }) {
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
                let result = Authentication::from_identity(auth.as_ref())
                    .and_then(|auth| adash_ssh::Connection::open(&host, auth, &known, timeout))
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
        if self.editor_dirty(index, cx) {
            self.pending_discard_file = Some((index, path));
            cx.notify();
            return;
        }
        self.request_file(index, path, cx);
    }
    pub fn request_file(&mut self, index: usize, path: PathBuf, cx: &mut Context<Self>) {
        let Some(session) = self.sessions.get(index).filter(|session| !session.closed) else {
            return;
        };
        if let Some(worker) = &session.worker {
            let _ = worker.send(Request::Open(path));
        }
        cx.notify();
    }
    pub fn editor_dirty(&self, index: usize, cx: &App) -> bool {
        self.sessions.get(index).is_some_and(|session| {
            session.editor_path.is_some()
                && session.editor.read(cx).value().as_str() != session.editor_original
        })
    }
    pub fn jump_directory(&mut self, index: usize, parent: bool, cx: &mut Context<Self>) {
        let Some(session) = self.sessions.get(index).filter(|session| !session.closed) else {
            return;
        };
        let path = if parent {
            session
                .directory
                .parent()
                .unwrap_or(std::path::Path::new("/"))
                .to_path_buf()
        } else {
            PathBuf::from(session.directory_input.read(cx).value().trim())
        };
        if let Some(worker) = &session.worker {
            let _ = worker.send(Request::List(path));
        }
        cx.notify();
    }
    pub fn begin_remote_name(
        &mut self,
        index: usize,
        path: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.sessions.get(index).filter(|session| !session.closed) else {
            return;
        };
        let name = path
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        session
            .remote_name
            .update(cx, |state, cx| state.set_value(name, window, cx));
        self.pending_remote_name = Some((index, path));
        cx.notify();
    }
    pub fn confirm_remote_name(&mut self, cx: &mut Context<Self>) {
        let Some((index, path)) = self.pending_remote_name.clone() else {
            return;
        };
        let Some(session) = self.sessions.get(index).filter(|session| !session.closed) else {
            self.pending_remote_name = None;
            cx.notify();
            return;
        };
        let name = session.remote_name.read(cx).value().trim().to_owned();
        if name.is_empty() || name.contains('/') || matches!(name.as_str(), "." | "..") {
            self.notice = Some("请输入一个名称，不能包含 / 或为 .、..".into());
            cx.notify();
            return;
        }
        if let Some(worker) = &session.worker {
            let request = match path {
                Some(path) => {
                    let target = path.with_file_name(name);
                    Request::Rename(path, target)
                }
                None => Request::Mkdir(session.directory.join(name)),
            };
            if !worker.send(request) {
                self.notice = Some("文件连接已关闭，请重新连接".into());
            }
        }
        self.pending_remote_name = None;
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
        if let Some(session) = self.sessions.get_mut(index) {
            if session.forward_rx.remove(rule_id).is_some() {
                cx.notify();
                return;
            }
            if let Some(handle) = session.forwards.remove(rule_id) {
                handle.stop();
                session.status = Some("端口转发已停止".to_owned());
                cx.notify();
                return;
            }
        }
        if !self
            .sessions
            .get(index)
            .is_some_and(|session| !session.closed && session.connected)
        {
            self.notice = Some("请先完成主机连接与指纹确认，再启动转发".into());
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
            let outcome = Authentication::from_identity(auth.as_ref())
                .and_then(|auth| adash_ssh::Connection::open(&host, auth, &known, timeout))
                .and_then(|connection| ForwardHandle::start(connection, rule))
                .map_err(|error| format!("{error:#}"));
            let _ = tx.send(outcome);
        });
        if let Some(session) = self.sessions.get_mut(index) {
            session.forward_errors.remove(&rule_id);
            session.forward_rx.insert(rule_id, rx);
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
        if session.transfer.is_some() {
            self.notice = Some("当前已有文件传输，请等待完成或取消后重试".into());
            cx.notify();
            return;
        }
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
                    session.transfer = Some((0, 0));
                    session.status = Some("正在上传…".to_owned());
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Pick a local destination for a remote file.
    pub fn download_pick(&mut self, index: usize, remote: PathBuf, cx: &mut Context<Self>) {
        if self
            .sessions
            .get(index)
            .is_some_and(|session| session.transfer.is_some())
        {
            self.notice = Some("当前已有文件传输，请等待完成或取消后重试".into());
            cx.notify();
            return;
        }
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
                    session.transfer = Some((0, 0));
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
        let count = selected.len();
        let tasks: std::collections::VecDeque<_> = selected
            .into_iter()
            .map(|host| {
                let identity = host
                    .identity
                    .as_ref()
                    .and_then(|id| self.profiles.data.identities.get(id))
                    .cloned();
                (host, identity)
            })
            .collect();
        let tasks = std::sync::Arc::new(std::sync::Mutex::new(tasks));
        for _ in 0..count.min(8) {
            let tasks = tasks.clone();
            let known = known.clone();
            let tx = tx.clone();
            let command = command.clone();
            std::thread::spawn(move || {
                loop {
                    let Some((host, identity)) = tasks.lock().unwrap().pop_front() else {
                        break;
                    };
                    let result = (|| -> Result<adash_ssh::ExecResult, String> {
                        let auth = Authentication::from_identity(identity.as_ref())
                            .map_err(|error| format!("{error:#}"))?;
                        let connection = adash_ssh::Connection::open(&host, auth, &known, timeout)
                            .map_err(|error| format!("{error:#}"))?;
                        connection
                            .execute(&command)
                            .map_err(|error| format!("{error:#}"))
                    })();
                    if tx.send((host.name, result)).is_err() {
                        break;
                    }
                }
            });
        }
        self.batch_output.clear();
        self.batch_running = count;
        self.batch_rx = Some(rx);
        self.notice = Some(format!("正在 {count} 台主机上执行（最多同时连接 8 台）"));
        cx.notify();
    }

    pub fn close_session(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.editor_dirty(index, cx)
            || self.store.data.settings.confirm_close
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
        let session = &mut self.sessions[index];
        session.closed = true;
        session.terminal_input.update(cx, |input, _| input.reset());
        session.connected = false;
        session.connecting = false;
        session.terminal_rx = None;
        session.forward_rx.clear();
        if let Some(terminal) = session.terminal.take() {
            terminal.close();
        }
        session.forwards.clear();
        if let Some(worker) = session.worker.take() {
            let _ = worker.send(Request::Close);
        }
        if self.active_session == Some(index) {
            self.active_session = None;
        }
        if self.page == Page::Session(index) {
            self.page = Page::Hosts;
        }
        self.pending_close = None;
        cx.notify();
    }

    /// Drain worker replies and PTY output. Called from one timer task.
    pub fn poll_ssh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut changed = false;
        let mut start_terminals = Vec::new();
        let interval =
            Duration::from_secs(self.store.data.settings.monitor_interval.clamp(5, 3600));
        for (index, session) in self
            .sessions
            .iter_mut()
            .enumerate()
            .filter(|(_, session)| !session.closed)
        {
            let forward_replies: Vec<_> = session
                .forward_rx
                .iter()
                .filter_map(|(id, rx)| match rx.try_recv() {
                    Ok(result) => Some((id.clone(), result)),
                    Err(TryRecvError::Disconnected) => {
                        Some((id.clone(), Err("转发线程已退出".into())))
                    }
                    Err(TryRecvError::Empty) => None,
                })
                .collect();
            for (id, result) in forward_replies {
                session.forward_rx.remove(&id);
                match result {
                    Ok(handle) => {
                        session.status =
                            Some(format!("转发已启动：127.0.0.1:{}", handle.bound_port));
                        session.forwards.insert(id, handle);
                    }
                    Err(error) => {
                        session.forward_errors.insert(id, error.clone());
                        session.status = Some(format!("转发失败：{error}"));
                    }
                }
                changed = true;
            }
            if let Some(rx) = &session.terminal_rx {
                match rx.try_recv() {
                    Ok(Ok(terminal)) => {
                        session.terminal = Some(terminal);
                        session.terminal_rx = None;
                        session.status = Some("终端已连接".to_owned());
                        if self.page == Page::Session(index) && self.session_tab == 0 {
                            session.focus.focus(window, cx);
                        }
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
                        if self.store.data.settings.auto_shell || session.want_terminal {
                            session.want_terminal = false;
                            start_terminals.push(index);
                        }
                        changed = true;
                    }
                    Reply::Done => {
                        session.transfer = None;
                        session.status = Some("操作完成".to_owned());
                        let _ = worker.send(Request::List(session.directory.clone()));
                        changed = true;
                    }
                    Reply::Files { path, entries } => {
                        session.directory = path.clone();
                        session.directory_input.update(cx, |state, cx| {
                            state.set_value(path.display().to_string(), window, cx)
                        });
                        session.files = Some(entries);
                        session.file_error = None;
                        changed = true;
                    }
                    Reply::Monitor(sample) => {
                        session.previous_monitor = session.monitor.replace(sample);
                        session.monitor_error = None;
                        changed = true;
                    }
                    Reply::Renamed(from, to) => {
                        if let Some(path) = &session.editor_path
                            && let Ok(relative) = path.strip_prefix(&from)
                        {
                            session.editor_path = Some(to.join(relative));
                        }
                        let _ = worker.send(Request::List(session.directory.clone()));
                        session.status = Some("重命名完成".into());
                        changed = true;
                    }
                    Reply::Saved(path, text) => {
                        if session.editor_path.as_ref() == Some(&path) {
                            session.editor_original = text;
                        }
                        session.status = Some("文件已保存".into());
                        let _ = worker.send(Request::List(session.directory.clone()));
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
                        session.editor_original = text.clone();
                        session.pending_editor_text = Some(text);
                        changed = true;
                    }
                    Reply::OperationError(operation, error) => {
                        match operation {
                            adash_ssh::Operation::Files | adash_ssh::Operation::Transfer => {
                                session.file_error = Some(error.clone())
                            }
                            adash_ssh::Operation::Monitor => {
                                session.monitor_error = Some(error.clone())
                            }
                            adash_ssh::Operation::Exec => {
                                session.exec_output = Some(format!("执行失败：{error}"))
                            }
                        }
                        session.transfer = None;
                        session.status = Some(error);
                        changed = true;
                    }
                    Reply::TransferProgress { done, total } => {
                        session.transfer = Some((done, total));
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
