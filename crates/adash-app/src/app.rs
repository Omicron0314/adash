//! Application shell: state, routing, and the three product pages plus the
//! host-connection pages.
//!
//! Ordering of work follows AGENTS.md: data is loaded off the UI path where it
//! can fail (UCloud calls are synchronous today and bounded by a 20s timeout),
//! and the agent is pumped from a short foreground timer instead of blocking.

use crate::agent::{Agent, ChatMessage, ConfirmRequest};
use crate::i18n::{self, nav};
use crate::ssh::{ForwardForm, HostForm, IdentityForm, SnippetForm, SshSession};
use crate::widgets;
use adash_core::executor;
use adash_core::flow::{Flow, FlowRun};
use adash_core::inventory::Inventory;
use adash_core::optimizer::Finding;
use adash_core::plan::{Plan, create_action};
use adash_core::project::{ProjectView, project_views};
use adash_core::resource::{ResourceId, ResourceKind, money};
use adash_core::state::{ProjectRecord, Store, now_secs};
use adash_core::{Billing, PlanKind, Resource};
use adash_ssh::{ForwardDirection, IdentityKind, ProfileStore};
use gpui::base::input::InputState;
use gpui::prelude::*;
use gpui::*;
use gpui_kit_material::prelude::*;
use gpui_kit_material::theme;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;
use ucloud_api::{Mode, UcloudClient};

/// A top-level destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// Agent conversation plus the account overview.
    Home,
    /// Everything already created in this account.
    Resources,
    /// A project's service graph.
    Flows,
    /// Saved SSH hosts.
    Hosts,
    /// SSH identities (transport lands in M6).
    Identities,
    /// Reusable command snippets.
    Snippets,
    /// Port forwarding rules.
    PortForward,
    /// Credentials, theme, language.
    Settings,
    /// One open host session, by index into [`Adash::sessions`].
    Session(usize),
}

impl Page {
    /// Product pages, in sidebar order.
    pub const PRODUCT: [Self; 4] = [Self::Home, Self::Resources, Self::Hosts, Self::Flows];
    /// Host-connection pages, in sidebar order.
    pub const HOSTS: [Self; 3] = [Self::Identities, Self::Snippets, Self::PortForward];

    /// Stable id for element ids and tests.
    pub fn id(self) -> String {
        match self {
            Self::Home => "home".to_owned(),
            Self::Resources => "resources".to_owned(),
            Self::Flows => "flows".to_owned(),
            Self::Hosts => "hosts".to_owned(),
            Self::Identities => "identities".to_owned(),
            Self::Snippets => "snippets".to_owned(),
            Self::PortForward => "port-forward".to_owned(),
            Self::Settings => "settings".to_owned(),
            Self::Session(index) => format!("session-{index}"),
        }
    }

    /// Sidebar label (sessions use their host name instead).
    pub fn label(self) -> &'static str {
        match self {
            Self::Home => nav::HOME,
            Self::Resources => nav::RESOURCES,
            Self::Flows => nav::FLOWS,
            Self::Hosts => nav::HOSTS,
            Self::Identities => nav::IDENTITIES,
            Self::Snippets => nav::SNIPPETS,
            Self::PortForward => nav::PORT_FORWARD,
            Self::Settings => nav::SETTINGS,
            Self::Session(_) => nav::HOSTS,
        }
    }
}

/// Resource filters, shared by the page and the overview's deep links.
#[derive(Debug, Clone, Default)]
pub struct Filters {
    /// Selected products; empty means "all".
    pub kinds: BTreeSet<ResourceKind>,
    /// Region id.
    pub region: Option<String>,
    /// Project slug.
    pub project: Option<String>,
}

/// Playback state of the flow canvas.
#[derive(Debug, Clone, Default)]
pub struct Replay {
    /// Currently highlighted run.
    pub run: Option<String>,
    /// Number of steps revealed so far.
    pub step: usize,
    /// Whether the timer is advancing steps.
    pub playing: bool,
}

/// Root view.
pub struct Adash {
    /// Current page.
    pub page: Page,
    /// Backend, when credentials loaded.
    pub client: Option<UcloudClient>,
    /// Live or mock.
    pub mode: Mode,
    /// Why the client is missing.
    pub client_error: Option<String>,
    /// Last inventory read.
    pub inventory: Inventory,
    /// Bills and balance.
    pub billing: Billing,
    /// Project views (resources + budget).
    pub projects: Vec<ProjectView>,
    /// Waste findings.
    pub findings: Vec<Finding>,
    /// Local state: plans, approvals, runs.
    pub store: Store,
    /// Conversation.
    pub messages: Vec<ChatMessage>,
    /// pi process or the local responder.
    pub agent: Agent,
    /// Chat input.
    pub input: Entity<InputState>,
    /// Resource search box.
    pub search: Entity<InputState>,
    /// Resource filters.
    pub filters: Filters,
    /// Resource shown in the side panel.
    pub selected: Option<ResourceId>,
    /// Project shown on the flow page.
    pub flow_project: Option<String>,
    /// Flow playback.
    pub replay: Replay,
    /// Which surface of a host session is showing (0 terminal, 1 files, 2 monitor).
    pub session_tab: usize,
    /// Compact flow layout (the 「适应窗口」 toggle).
    pub flow_compact: bool,
    /// Node selected on the flow canvas.
    pub flow_selected: Option<String>,
    /// Saved SSH hosts, identities, snippets, forwards and trusted keys.
    pub profiles: ProfileStore,
    /// Why the profile file could not load.
    pub profile_error: Option<String>,
    /// Open host sessions.
    pub sessions: Vec<SshSession>,
    /// Add-host fields.
    pub host_form: HostForm,
    /// Add-identity fields and the chosen kind.
    pub identity_form: IdentityForm,
    /// Chosen identity kind.
    pub identity_kind: IdentityKind,
    /// Add-snippet fields.
    pub snippet_form: SnippetForm,
    /// Add-forward fields and direction.
    pub forward_form: ForwardForm,
    /// Chosen forward direction.
    pub forward_direction: ForwardDirection,
    /// One-line command to run on the active host.
    pub run_command: Entity<InputState>,
    /// Keeps the SSH reply drain alive.
    pub ssh_timer: Option<Task<()>>,
    /// 0 通用 / 1 终端 / 2 连接 / 3 AI.
    pub settings_category: usize,
    pub ai_settings: crate::ai::AiSettings,
    pub ai_form: crate::ai::AiForm,
    pub ai_error: Option<String>,
    pub ai_saving: bool,
    /// Last focused SSH session; retained when opening snippets or forwards.
    pub active_session: Option<usize>,
    pub selected_hosts: BTreeSet<String>,
    pub pending_close: Option<usize>,
    pub pending_exit: bool,
    pub batch_output: Vec<String>,
    pub batch_rx:
        Option<std::sync::mpsc::Receiver<(String, Result<adash_ssh::ExecResult, String>)>>,
    pub batch_running: usize,
    pub pending_remote_delete: Option<(usize, PathBuf)>,
    pub pending_remote_name: Option<(usize, Option<PathBuf>)>,
    pub pending_discard_file: Option<(usize, PathBuf)>,
    pub pending_forget_keys: bool,
    last_dialog: Option<(&'static str, String, &'static str, bool)>,
    /// One-shot message under the title bar.
    pub notice: Option<String>,
    /// Whether a refresh is in flight.
    pub loading: bool,
    /// Unix seconds of the last refresh.
    pub last_refresh: i64,
    /// The agent pump task, kept alive while streaming.
    pub pump: Option<Task<()>>,
}

impl Adash {
    /// Build the shell: load the account, open local state, connect the agent.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (client, client_error, mode) = match UcloudClient::from_env() {
            Ok(client) => {
                let mode = client.mode();
                (Some(client), None, mode)
            }
            Err(error) => (None, Some(error.to_string()), Mode::Mock),
        };

        let mut store = Store::open_default();
        if mode == Mode::Mock && store.data.plans.is_empty() && store.data.projects.is_empty() {
            executor::seed_demo(&mut store);
            let _ = store.save();
        }

        let input = cx.new(|cx| InputState::new(window, cx));
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("搜索名称或 id"));
        let run_command =
            cx.new(|cx| InputState::new(window, cx).placeholder("在主机上执行命令，回车运行"));
        let profile_path = crate::agent::data_dir()
            .unwrap_or_default()
            .join("hosts.json");
        let (profiles, profile_error) = match ProfileStore::open(profile_path.clone()) {
            Ok(store) => (store, None),
            Err(error) => (
                ProfileStore::unavailable(profile_path),
                Some(format!("{error:#}")),
            ),
        };
        let (ai_settings, ai_error) = match crate::ai::AiSettings::load() {
            Ok(settings) => (settings, None),
            Err(error) => (crate::ai::AiSettings::default(), Some(error)),
        };
        let ai_form = crate::ai::AiForm::new(&ai_settings, window, cx);
        let host_form = HostForm::new(window, cx);
        let identity_form = IdentityForm::new(window, cx);
        let snippet_form = SnippetForm::new(window, cx);
        let forward_form = ForwardForm::new(window, cx);

        let mut app = Self {
            page: Page::Home,
            client,
            mode,
            client_error,
            inventory: Inventory::default(),
            billing: Billing::default(),
            projects: Vec::new(),
            findings: Vec::new(),
            store,
            messages: vec![ChatMessage::agent(welcome(mode), true)],
            agent: Agent::connect(mode),
            input,
            search,
            filters: Filters::default(),
            selected: None,
            flow_project: None,
            replay: Replay::default(),
            session_tab: 0,
            flow_compact: false,
            flow_selected: None,
            profiles,
            profile_error,
            sessions: Vec::new(),
            host_form,
            identity_form,
            identity_kind: IdentityKind::Agent,
            snippet_form,
            forward_form,
            forward_direction: ForwardDirection::Local,
            run_command,
            ssh_timer: None,
            settings_category: 0,
            ai_settings,
            ai_form,
            ai_error,
            ai_saving: false,
            active_session: None,
            selected_hosts: BTreeSet::new(),
            pending_close: None,
            pending_exit: false,
            batch_output: Vec::new(),
            batch_rx: None,
            batch_running: 0,
            pending_remote_delete: None,
            pending_remote_name: None,
            pending_discard_file: None,
            pending_forget_keys: false,
            last_dialog: None,
            notice: None,
            loading: false,
            last_refresh: 0,
            pump: None,
        };

        if let Some(reason) = app.agent.offline_reason()
            && let Some(welcome) = app.messages.first_mut()
        {
            welcome.text.push_str(&format!("\n（{reason}）"));
        }
        cx.observe(&app.search, |_, _, cx| cx.notify()).detach();
        // Enter sends; Shift+Enter stays available for a future multi-line
        // composer. The subscription is dropped with the view.
        cx.subscribe_in(
            &app.input,
            window,
            |this, _, event: &gpui::base::input::InputEvent, window, cx| {
                if matches!(
                    event,
                    gpui::base::input::InputEvent::PressEnter { shift: false, .. }
                ) {
                    this.send_input(window, cx);
                }
            },
        )
        .detach();
        app.ssh_timer = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                if this
                    .update_in(cx, |this, window, cx| {
                        this.poll_ssh(window, cx);
                        gpui_kit_material::theme::advance_component_theme(
                            crate::palette::resolve(&this.store.data.settings.appearance, cx),
                            1.,
                            window,
                            cx,
                        );
                        if !this.agent.streaming {
                            let previous = (
                                this.agent.model.clone(),
                                this.agent.status.clone(),
                                this.agent.offline_reason().map(ToOwned::to_owned),
                            );
                            this.agent.pump(|_| {}, |_| {});
                            let current = (
                                this.agent.model.clone(),
                                this.agent.status.clone(),
                                this.agent.offline_reason().map(ToOwned::to_owned),
                            );
                            if previous != current {
                                cx.notify();
                            }
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
        gpui_kit_material::theme::advance_component_theme(
            crate::palette::resolve(&app.store.data.settings.appearance, cx),
            1.,
            window,
            cx,
        );
        app.refresh();
        app.flow_project = app.default_flow_project();
        #[cfg(feature = "ui-test-hooks")]
        app.apply_startup_env(window, cx);
        let view = cx.entity().downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            view.update(cx, |this, cx| {
                if this
                    .sessions
                    .iter()
                    .enumerate()
                    .any(|(index, session)| !session.closed && this.editor_dirty(index, cx))
                    || this.store.data.settings.confirm_close
                        && this.sessions.iter().any(|session| {
                            !session.closed
                                && (session.connected
                                    || session
                                        .terminal
                                        .as_ref()
                                        .is_some_and(adash_ssh::Terminal::is_connected))
                        })
                {
                    this.pending_exit = true;
                    cx.notify();
                    false
                } else {
                    true
                }
            })
            .unwrap_or(true)
        });
        app
    }

    /// Re-read the account and recompute everything derived from it.
    pub fn refresh(&mut self) {
        let Some(client) = &self.client else {
            return;
        };
        self.loading = true;
        let inventory = Inventory::load(client);
        let billing = Billing::load(client, &inventory);
        self.projects = project_views(
            &inventory,
            &billing,
            self.store.data.projects.values().cloned(),
        );
        self.findings = adash_core::optimizer::analyze(&inventory);
        self.billing = billing;
        self.inventory = inventory;
        self.last_refresh = now_secs();
        self.loading = false;
        if self.flow_project.is_none() {
            self.flow_project = self.default_flow_project();
        }
    }

    /// Development aid for docs and manual tests:
    /// `ADASH_START_PAGE=resources|flows|settings` plus
    /// `ADASH_START_SELECTED=<kind:id>` and `ADASH_START_FLOW=<slug>`.
    /// It only chooses where the window opens; it never changes capabilities.
    #[cfg(feature = "ui-test-hooks")]
    fn apply_startup_env(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Ok(key) = std::env::var("ADASH_START_SELECTED")
            && let Some((kind, id)) = key.split_once(':')
            && let Some(kind) = ResourceKind::ALL
                .into_iter()
                .find(|candidate| candidate.as_str() == kind)
        {
            self.selected = Some(ResourceId::new(kind, id));
        }
        if let Ok(slug) = std::env::var("ADASH_START_FLOW") {
            self.flow_project = Some(slug);
        }
        if let Ok(appearance) = std::env::var("ADASH_DEMO_APPEARANCE") {
            self.store.data.settings.appearance = appearance;
        }
        if let Ok(category) = std::env::var("ADASH_START_SETTINGS") {
            self.settings_category = category.parse::<usize>().unwrap_or(0).min(3);
        }

        self.page = match std::env::var("ADASH_START_PAGE").as_deref() {
            Ok("resources") => Page::Resources,
            Ok("hosts") => Page::Hosts,
            Ok("flows") => Page::Flows,
            Ok("settings") => Page::Settings,
            Ok("identities") => Page::Identities,
            Ok("snippets") => Page::Snippets,
            Ok("port-forward") => Page::PortForward,
            Ok("session") => {
                let key = self
                    .selected
                    .as_ref()
                    .map(ResourceId::key)
                    .unwrap_or_default();
                if let Some(resource) = self
                    .inventory
                    .resources
                    .iter()
                    .find(|resource| resource.id.key() == key)
                    .cloned()
                {
                    self.open_session(&resource, window, cx);
                }
                self.page
            }
            _ => Page::Home,
        };

        if let Ok(id) = std::env::var("ADASH_START_HOST") {
            self.open_host(&id, window, cx);
            if let Some(index) = self.active_session
                && let Some(session) = self.sessions.get_mut(index)
                && let Ok(path) = std::env::var("ADASH_START_REMOTE_DIR")
            {
                session.directory = PathBuf::from(path);
            }
            if let Ok(tab) = std::env::var("ADASH_START_SESSION_TAB") {
                self.session_tab = tab.parse::<usize>().unwrap_or(0).min(2);
            }
            // Demo hook: show a confirmation surface so its layout and focus
            // trap can be checked in a native capture. It performs no change.
            if std::env::var("ADASH_DEMO_DIALOG").is_ok()
                && let Some(index) = self.active_session
            {
                self.begin_remote_name(index, None, window, cx);
            }
        }
        // Demo/screenshot hooks: run one local question and start a replay so
        // the state is reproducible without clicking. Both are read-only.
        if let Ok(question) = std::env::var("ADASH_DEMO_ASK") {
            self.ask(question, cx);
        }
        if std::env::var_os("ADASH_DEMO_REPLAY").is_some()
            && let Some(flow) = self.current_flow()
            && let Some(run) = flow.runs.last().cloned()
        {
            self.start_replay(run.id, cx);
        }
    }

    fn default_flow_project(&self) -> Option<String> {
        self.inventory.flow_projects().first().cloned()
    }

    /// The flow of the selected project, with runs.
    pub fn current_flow(&self) -> Option<Flow> {
        let slug = self.flow_project.as_deref()?;
        Some(Flow::build(
            &self.inventory,
            slug,
            self.store.project(slug),
            self.store.runs_of(slug),
        ))
    }

    /// The run currently being replayed.
    pub fn replay_run(&self) -> Option<FlowRun> {
        let run_id = self.replay.run.clone()?;
        let project = self.flow_project.as_deref()?;
        self.store
            .runs_of(project)
            .into_iter()
            .find(|run| run.id == run_id)
    }

    /// The plan the user still has to decide on, newest first.
    pub fn pending_plan(&self) -> Option<&Plan> {
        self.store
            .data
            .plans
            .iter()
            .filter(|plan| !self.store.data.approvals.contains_key(&plan.id))
            .max_by_key(|plan| plan.created_at)
    }

    /// Send whatever is in the chat input.
    pub fn send_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.agent.streaming {
            self.notice = Some("正在生成回答；可点击「停止」后再发送".into());
            cx.notify();
            return;
        }
        let value = self.input.read(cx).value().trim().to_owned();
        if value.is_empty() {
            return;
        }
        self.input
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.ask(value, cx);
    }

    /// Ask the agent (or the local responder) one question.
    pub fn ask(&mut self, text: String, cx: &mut Context<Self>) {
        if self.agent.streaming {
            self.notice = Some("请等待当前回答完成，或点击停止".into());
            cx.notify();
            return;
        }
        self.messages.push(ChatMessage::user(text.clone()));
        if self.agent.prompt(&text) {
            self.messages.push(ChatMessage::agent("", false));
            self.start_pump(cx);
            cx.notify();
            return;
        }

        let (answer, notes) = self.offline_answer(&text);
        let mut message = ChatMessage::agent(answer, true);
        message.notes = notes;
        self.messages.push(message);
        cx.notify();
    }

    /// Poll pi until it settles.
    fn start_pump(&mut self, cx: &mut Context<Self>) {
        if self.pump.is_some() {
            return;
        }
        self.pump = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(120))
                    .await;
                let Ok(Some(done)) = this.update(cx, |this, cx| this.pump_once(cx)) else {
                    break;
                };
                if done {
                    break;
                }
            }
            let _ = this.update(cx, |this, cx| {
                this.pump = None;
                cx.notify();
            });
        }));
    }

    /// Drain agent records; `Some(true)` when the run is over.
    fn pump_once(&mut self, cx: &mut Context<Self>) -> Option<bool> {
        let mut text = String::new();
        let mut notes = Vec::new();
        let pending: Option<ConfirmRequest>;
        let settled = {
            let agent = &mut self.agent;
            let mut on_text = |delta: &str| text.push_str(delta);
            let mut on_tool = |line: String| notes.push(line);
            let settled = agent.pump(&mut on_text, &mut on_tool);
            pending = agent.pending.clone();
            settled
        };

        let streamed = !text.is_empty() || !notes.is_empty();
        if pending.is_some() || !notes.is_empty() || settled {
            self.store = Store::open(self.store.path().to_path_buf());
        }
        if let Some(reason) = self.agent.failure.take() {
            let question = self
                .messages
                .iter()
                .rev()
                .find(|message| message.role == crate::agent::Role::User)
                .map(|message| message.text.clone());
            if let Some(question) = question {
                let (answer, notes) = self.offline_answer(&question);
                if let Some(message) = self.messages.last_mut()
                    && message.role == crate::agent::Role::Agent
                {
                    message.text = answer;
                    message.offline = true;
                    message.notes = notes;
                    message.notes.push(if reason.starts_with("模型不可用") {
                        reason
                    } else {
                        format!("模型不可用：{reason}")
                    });
                }
                cx.notify();
            }
        }
        if !text.is_empty()
            && let Some(last) = self.messages.last_mut()
            && last.role == crate::agent::Role::Agent
        {
            last.text.push_str(&text);
            last.offline = false;
        }
        if !notes.is_empty()
            && let Some(last) = self.messages.last_mut()
            && last.role == crate::agent::Role::Agent
        {
            last.notes.extend(notes);
        }

        if let Some(request) = pending {
            // Surface the plan card: the store already holds the plan the agent
            // just generated, so the card renders from local state.
            self.notice = Some("代理请求执行一个计划，请在卡片中确认".to_owned());
            let _ = request;
        }

        if settled {
            self.messages.retain(|message| {
                !(message.role == crate::agent::Role::Agent && message.text.is_empty())
            });
            self.refresh();
        }
        // Streamed deltas are only visible if the view is told to repaint.
        if streamed {
            cx.notify();
        }
        Some(settled)
    }

    /// SDK confirmations resume the same pending tool call; offline Mock
    /// confirmations use the same expiry/hash/one-time-approval checks.
    pub fn decide_plan(&mut self, plan_id: String, approve: bool, cx: &mut Context<Self>) {
        let pending = self.agent.pending.clone();
        if !approve {
            if let Some(request) = pending {
                self.agent.answer_confirm(&request.request_id, false);
            }
            self.store.take_plan(&plan_id);
            if let Err(error) = self.store.save() {
                self.notice = Some(error);
            } else {
                self.notice = Some("计划已取消，云上没有任何变更".to_owned());
            }
            cx.notify();
            return;
        }
        if let Err(error) = self.store.plan_for_apply(&plan_id) {
            self.notice = Some(error);
            cx.notify();
            return;
        }
        if pending.is_none() && self.mode != Mode::Mock {
            self.notice = Some("真实账户的写操作暂未接入。计划已保留，没有修改云资源。".to_owned());
            cx.notify();
            return;
        }
        let Some(token) = self.store.approve(&plan_id, adash_core::plan::TTL_SECS) else {
            self.notice = Some("计划已不存在，请重新生成".to_owned());
            cx.notify();
            return;
        };
        if let Err(error) = self.store.save() {
            self.notice = Some(error);
            cx.notify();
            return;
        }
        if let Some(request) = pending {
            self.agent.answer_confirm(&request.request_id, true);
            self.notice = Some("已确认，代理正在执行批准的计划".to_owned());
            self.start_pump(cx);
        } else {
            if !self.store.consume_approval(&plan_id, &token) {
                self.notice = Some("批准已失效，请重新生成计划".to_owned());
                cx.notify();
                return;
            }
            match executor::apply_mock(&mut self.store, &self.inventory, &plan_id) {
                Ok(outcome) => {
                    let _ = self.store.save();
                    self.messages.push(ChatMessage::agent(
                        format!(
                            "已记录 Mock 执行，未修改真实云资源。项目 {} 的服务图已更新。",
                            outcome.project
                        ),
                        true,
                    ));
                    self.refresh();
                    self.page = Page::Flows;
                    self.flow_project = Some(outcome.project);
                }
                Err(error) => self.notice = Some(format!("执行失败：{error}")),
            }
        }
        cx.notify();
    }

    /// Open (or focus) a host session from the resources page. Cloud hosts are
    /// persisted as SSH targets so they also appear on the 主机 page.
    pub fn open_session(
        &mut self,
        resource: &Resource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = format!("cloud-{}", resource.id.key());
        if let Some(index) = self
            .sessions
            .iter()
            .position(|session| !session.closed && session.host.id == id)
        {
            self.page = Page::Session(index);
            cx.notify();
            return;
        }
        let Some(address) = resource
            .public_ip
            .clone()
            .or_else(|| resource.private_ip.clone())
        else {
            self.notice = Some("这台主机没有可用 IP，先在云资源页绑定 EIP".to_owned());
            cx.notify();
            return;
        };
        let host = adash_ssh::Host {
            id: id.clone(),
            name: resource.name.clone(),
            address,
            port: 22,
            username: "root".to_owned(),
            identity: None,
        };
        self.profiles.data.hosts.insert(id.clone(), host.clone());
        let _ = self.profiles.save();
        self.sessions.push(SshSession::new(host, window, cx));
        let index = self.sessions.len() - 1;
        self.page = Page::Session(index);
        self.active_session = Some(index);
        // Mock IPs are documentation data, not connection targets.
        if self.mode != Mode::Mock {
            self.connect(index, cx);
        }
        cx.notify();
    }

    /// Start replaying a run: highlight one step per 700ms.
    pub fn start_replay(&mut self, run_id: String, cx: &mut Context<Self>) {
        self.replay = Replay {
            run: Some(run_id),
            step: 0,
            playing: true,
        };
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(700))
                    .await;
                let Ok(keep_going) = this.update(cx, |this, cx| {
                    let total = this.replay_run().map_or(0, |run| run.steps.len());
                    if !this.replay.playing || this.replay.step >= total {
                        this.replay.playing = false;
                        return false;
                    }
                    this.replay.step += 1;
                    cx.notify();
                    true
                }) else {
                    break;
                };
                if !keep_going {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    /// A local, data-backed answer for when pi is not available.
    fn offline_answer(&mut self, prompt: &str) -> (String, Vec<String>) {
        let mut notes = vec!["离线应答 · 未调用模型".to_owned()];
        let text = prompt.to_lowercase();

        if text.contains("浪费") || text.contains("省钱") || text.contains("优化") {
            if self.findings.is_empty() {
                return ("没有发现闲置资源，账户挺干净的。".to_owned(), notes);
            }
            let plan = adash_core::optimizer::plan(&self.findings);
            let mut lines = vec![format!(
                "找到 {} 处可以省的钱，合计 {} / 月：",
                self.findings.len(),
                money(
                    self.findings
                        .iter()
                        .map(|finding| finding.monthly_savings_cents)
                        .sum()
                )
            )];
            for finding in &self.findings {
                lines.push(format!(
                    "· {} —— {}（{}）",
                    finding.title,
                    finding.detail,
                    finding.savings_text()
                ));
            }
            lines.push("已生成计划卡片，确认后我会记录执行。".to_owned());
            notes.push("本地计划 · ucloud_plan_optimize".to_owned());
            self.store.insert_plan(plan);
            let _ = self.store.save();
            return (lines.join("\n"), notes);
        }

        if text.contains("流程") || text.contains("服务") || text.contains("架构") {
            let Some(slug) = self.flow_project.clone() else {
                return ("还没有可以画图的项目。".to_owned(), notes);
            };
            let flow = adash_core::flow::Flow::build(
                &self.inventory,
                &slug,
                self.store.project(&slug),
                self.store.runs_of(&slug),
            );
            let mut lines = vec![format!(
                "{} 共 {} 个节点、{} 条连线：",
                flow.name,
                flow.nodes.len(),
                flow.edges.len()
            )];
            for node in &flow.nodes {
                lines.push(format!("· {}（{}）", node.label, node.kind_label()));
            }
            lines.push("切到「AI 流程」页可以看到图，并能回放部署顺序。".to_owned());
            notes.push("本地读取 · ucloud_flow".to_owned());
            return (lines.join("\n"), notes);
        }

        if text.contains("预算")
            || text.contains("花")
            || text.contains("账")
            || text.contains("钱")
        {
            let mut lines = vec![format!(
                "余额 {}，本月已花 {}。",
                self.billing
                    .balance_yuan
                    .map_or_else(|| "—".to_owned(), |yuan| format!("¥{yuan:.2}")),
                money(self.billing.month_cents)
            )];
            for (project, cents) in self.billing.by_project.iter().take(5) {
                lines.push(format!("· {project}：{}", money(*cents)));
            }
            notes.push("本地读取 · ucloud_billing".to_owned());
            return (lines.join("\n"), notes);
        }

        if text.contains("部署") || text.contains("上线") {
            let slug = self
                .store
                .data
                .projects
                .keys()
                .next()
                .cloned()
                .unwrap_or_else(|| "blog".to_owned());
            let actions = create_action(ResourceKind::UHost, "2C / 4GiB", "cn-bj2", 1);
            let plan = Plan::new(
                PlanKind::Deploy,
                format!("为 {slug} 在 cn-bj2 部署 1 台主机"),
                actions,
            )
            .with_project(&slug);
            let delta = plan.monthly_delta_text();
            self.store.insert_plan(plan);
            let _ = self.store.save();
            notes.push("本地计划 · ucloud_plan_deploy".to_owned());
            return (
                format!(
                    "可以。方案：1 台 2C/4GiB 云主机（cn-bj2），月成本 {delta}。计划卡片已生成，确认后执行。"
                ),
                notes,
            );
        }

        let mut lines = vec![self.inventory.summary_line()];
        if let Some(project) = self.store.project("shop-api") {
            lines.push(format!(
                "项目 {}：{}（{} 个资源）",
                project.name,
                project.note,
                self.inventory.of_project(Some("shop-api")).len()
            ));
        }
        lines.push(
            "可以问我：资源盘点、账单、预算、这个项目用了哪些服务、有没有浪费的钱。".to_owned(),
        );
        notes.push("本地读取 · ucloud_inventory".to_owned());
        (lines.join("\n"), notes)
    }

    /// Write project metadata (used by settings later).
    pub fn remember_project(&mut self, record: ProjectRecord) {
        self.store.set_project(record);
        let _ = self.store.save();
    }
}

fn welcome(mode: Mode) -> String {
    let mut lines = vec![
        "我是 adash，你的云管家。".to_owned(),
        "可以问我：这个项目用了哪些服务？账户里有什么浪费？这个月花了多少钱？".to_owned(),
    ];
    match mode {
        Mode::Mock => lines.push("现在是 Mock 模式：数据来自内置样例，不会花钱。".to_owned()),
        Mode::Live => lines.push("当前连接真实账户：任何变更都需要你在计划卡片里确认。".to_owned()),
    }
    lines.join("\n")
}

impl Render for Adash {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let modal = self.modal_open();
        for (index, session) in self.sessions.iter().enumerate() {
            let enabled = !modal
                && self.page == Page::Session(index)
                && self.session_tab == 0
                && !session.closed
                && session
                    .terminal
                    .as_ref()
                    .is_some_and(adash_ssh::Terminal::is_connected);
            session
                .terminal_input
                .update(cx, |input, _| input.enabled = enabled);
        }
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(color(ColorRole::SURFACE))
            .text_color(color(ColorRole::ON_SURFACE))
            .child(self.render_titlebar(window, cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_sidebar(cx))
                    .child(self.render_content(window, cx)),
            )
            .child(self.render_overlay(cx))
    }
}

impl Adash {
    pub(crate) fn modal_open(&self) -> bool {
        self.pending_close.is_some()
            || self.pending_exit
            || self.pending_remote_delete.is_some()
            || self.pending_remote_name.is_some()
            || self.pending_discard_file.is_some()
            || self.pending_forget_keys
    }
    /// Modal confirmations that must not proceed silently: closing live
    /// sessions, deleting a remote file, forgetting host keys.
    fn render_overlay(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let open = self.modal_open();
        let close_dirty = self
            .sessions
            .iter()
            .enumerate()
            .any(|(index, session)| !session.closed && self.editor_dirty(index, cx));
        let (title, body, confirm_label, destructive) = if self.pending_exit {
            (
                if close_dirty {
                    "还有未保存的修改"
                } else {
                    "还有活动会话"
                },
                format!(
                    "当前有 {} 个主机会话。关闭窗口会断开连接，未保存的文件修改将被放弃。",
                    self.sessions
                        .iter()
                        .filter(|session| !session.closed)
                        .count()
                ),
                "关闭窗口",
                true,
            )
        } else if let Some((_, path)) = &self.pending_discard_file {
            (
                "未保存的修改",
                format!("放弃当前文件的修改并打开 {}？", path.display()),
                "放弃修改并打开",
                true,
            )
        } else if let Some((_, path)) = &self.pending_remote_name {
            match path {
                Some(path) => (
                    "重命名",
                    format!("{} 的新名称：", path.display()),
                    "重命名",
                    false,
                ),
                None => ("新建远程目录", "目录名称：".into(), "创建目录", false),
            }
        } else if let Some((_, path)) = &self.pending_remote_delete {
            (
                "删除远程文件",
                format!("将永久删除 {}，此操作不可撤销。", path.display()),
                "删除",
                true,
            )
        } else if let Some(index) = self.pending_close {
            let name = self
                .sessions
                .get(index)
                .map_or_else(|| "会话".to_owned(), |session| session.host.name.clone());
            (
                "关闭会话",
                format!("关闭 {name} 的终端与转发连接？未保存的文件修改将被放弃。"),
                "关闭会话",
                true,
            )
        } else if self.pending_forget_keys {
            (
                "清除主机指纹",
                "清除后下次连接每台主机都需要重新核对指纹。".to_owned(),
                "清除",
                true,
            )
        } else {
            self.last_dialog
                .clone()
                .unwrap_or(("", String::new(), "", false))
        };
        if open {
            self.last_dialog = Some((title, body.clone(), confirm_label, destructive));
        }
        let view = cx.entity().downgrade();
        let mut dialog = MaterialDialog::new("ssh-confirmation")
            .modal(open)
            .width(420.)
            .animation(DialogAnimation::Fade)
            .dismiss_on_scrim(false)
            .headline(title)
            .supporting_text(body)
            .on_dismiss(move |_, cx| {
                let _ = view.update(cx, |this, cx| {
                    this.cancel_overlay(cx);
                });
            });
        if let Some((index, _)) = &self.pending_remote_name
            && let Some(session) = self.sessions.get(*index)
        {
            dialog = dialog.content(
                div()
                    .rounded(rem(24.))
                    .border_1()
                    .border_color(color(ColorRole::OUTLINE_VARIANT))
                    .child(widgets::rounded_input(
                        "remote-operation-name",
                        &session.remote_name,
                        Some("名称"),
                    )),
            );
        }
        let _ = destructive;
        dialog
            .action(widgets::text_button(
                "overlay-cancel",
                "取消",
                cx.listener(|this, _, _, cx| this.cancel_overlay(cx)),
            ))
            .action(widgets::filled_button(
                "overlay-confirm",
                confirm_label,
                cx.listener(|this, _, window, cx| {
                    if this.pending_exit {
                        this.pending_exit = false;
                        for session in &mut this.sessions {
                            if let Some(terminal) = session.terminal.take() {
                                terminal.close();
                            }
                            session.forwards.clear();
                            session.forward_rx.clear();
                        }
                        window.remove_window();
                    } else if let Some((index, path)) = this.pending_discard_file.take() {
                        this.request_file(index, path, cx);
                    } else if this.pending_remote_name.is_some() {
                        this.confirm_remote_name(cx);
                    } else if this.pending_remote_delete.is_some() {
                        this.confirm_remote_delete(cx);
                    } else if let Some(index) = this.pending_close.take() {
                        this.finish_close_session(index, cx);
                    } else if this.pending_forget_keys {
                        if let Err(error) = this
                            .profiles
                            .update(|profiles| profiles.known_hosts.clear())
                        {
                            this.notice = Some(format!("清除失败：{error:#}"));
                        } else {
                            this.pending_forget_keys = false;
                            this.notice = Some("已清除主机指纹".into());
                        }
                        cx.notify();
                    }
                }),
            ))
            .into_any_element()
    }

    fn cancel_overlay(&mut self, cx: &mut Context<Self>) {
        self.pending_exit = false;
        self.pending_close = None;
        self.pending_remote_delete = None;
        self.pending_remote_name = None;
        self.pending_discard_file = None;
        self.pending_forget_keys = false;
        cx.notify();
    }

    fn render_titlebar(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        // Kit lays supported min/max/close controls out inside the visible
        // frame. Material's viewport-anchored captions sit in the Linux shadow.
        let mut bar = gpui_kit::component::TitleBar::new()
            .w_full()
            .min_w_0()
            .h(px(32.))
            .pl(px(title_bar_leading(px(32.), window)))
            .border_0()
            .bg(color(ColorRole::SURFACE))
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(rem(10.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(rem(20.))
                            .rounded(rem(8.))
                            .bg(color(ColorRole::PRIMARY))
                            .child(
                                div()
                                    .text_size(rem(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(color(ColorRole::ON_PRIMARY))
                                    .child("ad"),
                            ),
                    )
                    .child(
                        div()
                            .text_size(rem(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(i18n::APP_NAME),
                    )
                    .child(self.mode_badge()),
            )
            .child(div().flex_1());

        bar = bar.child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(rem(6.))
                .pr(px(8.))
                .children((window.viewport_size().width >= px(640.)).then(|| {
                    div()
                        .text_size(TEXT_AUX)
                        .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                        .child(format!(
                            "更新于 {}",
                            if self.last_refresh == 0 {
                                "—".to_owned()
                            } else {
                                format!("{} 秒前", (now_secs() - self.last_refresh).max(0))
                            }
                        ))
                }))
                .child(title_bar_control(
                    MaterialButton::new("refresh-all")
                        .variant(ButtonVariant::Text)
                        .button_size(ButtonSize::ExtraSmall)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.refresh();
                            this.notice = Some("数据已刷新".to_owned());
                            cx.notify();
                        }))
                        .child(i18n::action::REFRESH),
                )),
        );
        bar.into_any_element()
    }

    fn mode_badge(&self) -> AnyElement {
        let (text, bg, fg) = match self.mode {
            Mode::Mock => (
                "MOCK",
                ColorRole::TERTIARY_CONTAINER,
                ColorRole::ON_TERTIARY_CONTAINER,
            ),
            Mode::Live => (
                "LIVE",
                ColorRole::ERROR_CONTAINER,
                ColorRole::ON_ERROR_CONTAINER,
            ),
        };
        div()
            .px(rem(8.))
            .py(px(2.))
            .rounded_full()
            .bg(color(bg))
            .text_color(color(fg))
            .text_size(rem(10.))
            .font_weight(FontWeight::SEMIBOLD)
            .child(text)
            .into_any_element()
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut rail = div()
            .flex()
            .flex_col()
            .w(px(96.))
            .h_full()
            .min_h_0()
            .items_center()
            .gap(px(4.))
            .py(rem(16.))
            .bg(color(ColorRole::SURFACE));

        for page in Page::PRODUCT {
            rail = rail.child(self.nav_item(page.id(), page.label(), page == self.page, page, cx));
        }

        if !self.sessions.is_empty() {
            for (index, session) in self
                .sessions
                .iter()
                .enumerate()
                .filter(|(_, session)| !session.closed)
            {
                rail = rail.child(self.nav_item(
                    format!("session-{index}"),
                    {
                        let limit = if session.host.name.is_ascii() { 8 } else { 5 };
                        if session.host.name.chars().count() > limit {
                            format!(
                                "{}…",
                                session.host.name.chars().take(limit).collect::<String>()
                            )
                        } else {
                            session.host.name.clone()
                        }
                    },
                    self.page == Page::Session(index),
                    Page::Session(index),
                    cx,
                ));
            }
        }

        for page in Page::HOSTS {
            rail = rail.child(self.nav_item(page.id(), page.label(), page == self.page, page, cx));
        }

        rail = rail.child(div().flex_1());
        rail.child(self.nav_item(
            Page::Settings.id(),
            Page::Settings.label(),
            self.page == Page::Settings,
            Page::Settings,
            cx,
        ))
        .into_any_element()
    }

    fn nav_item(
        &self,
        id: String,
        label: impl Into<SharedString> + Clone,
        selected: bool,
        target: Page,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let target_id = id.clone();
        let symbol = match target {
            Page::Home => "home",
            Page::Resources => "cloud",
            Page::Hosts => "dns",
            Page::Flows => "account_tree",
            Page::Session(_) => "terminal",
            Page::Identities => "key",
            Page::Snippets => "code",
            Page::PortForward => "swap_horiz",
            Page::Settings => "tune",
        };
        let item = NavItem::new(
            SharedString::from(id.clone()),
            label.into(),
            NavIcon::symbol(
                gpui_kit_material::icons::Symbol::named(symbol).expect("bundled symbol"),
            ),
        );
        NavigationItem::new(SharedString::from(id), item)
            .layout(NavItemLayout::Vertical)
            .selected(selected)
            .on_click(cx.listener(move |this, _, _, cx| {
                let _ = &target_id;
                this.page = target;
                if let Page::Session(index) = target {
                    this.active_session = Some(index);
                }
                cx.notify();
            }))
            .into_any_element()
    }

    fn render_content(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let body = match self.page {
            Page::Home => self.render_home(window, cx),
            Page::Resources => self.render_resources(window, cx),
            Page::Flows => self.render_flows(window, cx),
            Page::Hosts => self.render_hosts(window, cx),
            Page::Session(index) => self.render_session(index, window, cx),
            Page::Identities => self.render_identities(cx),
            Page::Snippets => self.render_snippets(window, cx),
            Page::PortForward => self.render_port_forward(cx),
            Page::Settings => self.render_settings(window, cx),
        };

        let mut column = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .gap(rem(12.))
            .my(px(12.))
            .mr(px(12.))
            .when(self.page != Page::Home, |panel| {
                panel
                    .p(rem(28.))
                    .rounded(rem(28.))
                    .bg(color(ColorRole::SURFACE_CONTAINER_LOW))
            });

        if let Some(error) = &self.client_error {
            column = column.child(crate::widgets::notice(
                format!("无法连接 UCloud：{error}（当前显示的是空账户）"),
                true,
            ));
        }
        if let Some(notice) = &self.notice {
            column = column.child(crate::widgets::notice(notice.clone(), false));
        }
        column
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(body),
            )
            .into_any_element()
    }
}

/// Theme hook: the app only ever paints with roles, so nothing else is needed.
pub fn theme_ready() -> bool {
    theme::color(ColorRole::SURFACE) != theme::color(ColorRole::SURFACE)
}
