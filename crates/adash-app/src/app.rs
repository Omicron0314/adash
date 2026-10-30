//! Application shell: state, routing, and the three product pages plus the
//! host-connection pages.
//!
//! Ordering of work follows AGENTS.md: data is loaded off the UI path where it
//! can fail (UCloud calls are synchronous today and bounded by a 20s timeout),
//! and the agent is pumped from a short foreground timer instead of blocking.

use crate::agent::{Agent, ChatMessage, ConfirmRequest};
use crate::i18n::{self, nav};
use adash_core::executor;
use adash_core::flow::{Flow, FlowRun};
use adash_core::inventory::Inventory;
use adash_core::optimizer::Finding;
use adash_core::plan::{Plan, create_action};
use adash_core::project::{ProjectView, project_views};
use adash_core::resource::{ResourceId, ResourceKind, money};
use adash_core::state::{ProjectRecord, Store, now_secs};
use adash_core::{Billing, PlanKind, Resource};
use gpui::base::input::InputState;
use gpui::prelude::*;
use gpui::*;
use gpui_kit_material::prelude::*;
use gpui_kit_material::theme;
use std::collections::BTreeSet;
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
    /// SSH identities (transport lands in M6).
    Identities,
    /// Reusable command snippets (M6).
    Snippets,
    /// Port forwarding rules (M6).
    PortForward,
    /// Credentials, theme, language.
    Settings,
    /// One open host session, by index into [`Adash::sessions`].
    Session(usize),
}

impl Page {
    /// Product pages, in sidebar order.
    pub const PRODUCT: [Self; 3] = [Self::Home, Self::Resources, Self::Flows];
    /// Host-connection pages, in sidebar order.
    pub const HOSTS: [Self; 3] = [Self::Identities, Self::Snippets, Self::PortForward];

    /// Stable id for element ids and tests.
    pub fn id(self) -> String {
        match self {
            Self::Home => "home".to_owned(),
            Self::Resources => "resources".to_owned(),
            Self::Flows => "flows".to_owned(),
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
            Self::Identities => nav::IDENTITIES,
            Self::Snippets => nav::SNIPPETS,
            Self::PortForward => nav::PORT_FORWARD,
            Self::Settings => nav::SETTINGS,
            Self::Session(_) => nav::HOSTS,
        }
    }
}

/// An open host session: one row in the sidebar, one window on the host.
#[derive(Debug, Clone)]
pub struct HostSession {
    /// The cloud host.
    pub resource: ResourceId,
    /// Host name, shown in the sidebar.
    pub name: String,
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
    /// Open host sessions.
    pub sessions: Vec<HostSession>,
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

        let input = cx.new(|cx| InputState::new(window, cx).placeholder(i18n::INPUT_PLACEHOLDER));
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("搜索名称或 id"));

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
            agent: Agent::connect(),
            input,
            search,
            filters: Filters::default(),
            selected: None,
            flow_project: None,
            replay: Replay::default(),
            session_tab: 0,
            flow_compact: false,
            flow_selected: None,
            sessions: Vec::new(),
            notice: None,
            loading: false,
            last_refresh: 0,
            pump: None,
        };

        if let Some(reason) = app.agent.offline_reason() {
            app.messages.push(ChatMessage::agent(
                format!("（{}：{reason}）", i18n::OFFLINE_PREFIX),
                true,
            ));
        }
        app.refresh();
        app.flow_project = app.default_flow_project();
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
        self.messages.push(ChatMessage::user(text.clone()));
        if self.agent.prompt(&text).is_some() {
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
    fn pump_once(&mut self, _cx: &mut Context<Self>) -> Option<bool> {
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
        Some(settled)
    }

    /// Answer the agent's confirm request, minting the approval when the user
    /// said yes. Without a live agent (offline), the plan is applied locally.
    pub fn decide_plan(&mut self, plan_id: String, approve: bool, cx: &mut Context<Self>) {
        let pending = self.agent.pending.clone();
        if !approve {
            if let Some(request) = pending {
                self.agent.answer_confirm(&request.request_id, false);
            }
            self.store.take_plan(&plan_id);
            let _ = self.store.save();
            self.notice = Some("计划已取消，云上没有任何变更".to_owned());
            cx.notify();
            return;
        }

        let Some(token) = self.store.approve(&plan_id, adash_core::plan::TTL_SECS) else {
            self.notice = Some("计划已不存在，请重新生成".to_owned());
            cx.notify();
            return;
        };
        let _ = self.store.save();

        match pending {
            Some(request) => {
                self.agent.answer_confirm(&request.request_id, true);
                self.notice = Some(format!(
                    "已确认计划，批准令牌 {}…",
                    &token[..token.len().min(12)]
                ));
                self.start_pump(cx);
            }
            None => {
                // Offline: nothing will call `apply`, so the app records it the
                // same way the tool would, and says what that means.
                match executor::apply_mock(&mut self.store, &self.inventory, &plan_id) {
                    Ok(outcome) => {
                        let _ = self.store.save();
                        self.messages.push(ChatMessage::agent(
                            format!(
                                "已批准并记录执行（离线模式，未真实创建资源）。项目 {} 的流程图已更新。",
                                outcome.project
                            ),
                            true,
                        ));
                        self.replay = Replay {
                            run: Some(outcome.run.id.clone()),
                            step: outcome.run.steps.len(),
                            playing: false,
                        };
                        self.refresh();
                        self.page = Page::Flows;
                        self.flow_project = Some(outcome.project);
                    }
                    Err(error) => self.notice = Some(format!("执行失败：{error}")),
                }
            }
        }
        cx.notify();
    }

    /// Open (or focus) a host session from the resources page.
    pub fn open_session(&mut self, resource: &Resource, cx: &mut Context<Self>) {
        if let Some(index) = self
            .sessions
            .iter()
            .position(|session| session.resource == resource.id)
        {
            self.page = Page::Session(index);
        } else {
            self.sessions.push(HostSession {
                resource: resource.id.clone(),
                name: resource.name.clone(),
            });
            self.page = Page::Session(self.sessions.len() - 1);
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
            notes.push("ucloud_plan_optimize".to_owned());
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
            notes.push("ucloud_flow".to_owned());
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
            notes.push("ucloud_billing".to_owned());
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
            notes.push("ucloud_plan_deploy".to_owned());
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
        notes.push("ucloud_inventory".to_owned());
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
        Mode::Mock => lines.push("现在是 Mock 模式：数据来自 fixtures，不会花钱。".to_owned()),
        Mode::Live => lines.push("当前连接真实账户：任何变更都需要你在计划卡片里确认。".to_owned()),
    }
    lines.join("\n")
}

impl Render for Adash {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(color(ColorRole::SURFACE))
            .text_color(color(ColorRole::ON_SURFACE))
            .child(self.render_titlebar(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_sidebar(cx))
                    .child(self.render_content(window, cx)),
            )
    }
}

impl Adash {
    fn render_titlebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let labels = CaptionLabels {
            minimize: "最小化".into(),
            maximize: "最大化".into(),
            restore: "还原".into(),
            close: "关闭".into(),
        };
        let mut bar = TitleBar::new("adash-titlebar", px(56.), labels)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(rem(10.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(rem(28.))
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
                            .flex()
                            .flex_col()
                            .gap(px(1.))
                            .child(
                                div()
                                    .text_size(TEXT_BODY)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(i18n::APP_NAME),
                            )
                            .child(
                                div()
                                    .text_size(rem(10.))
                                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                                    .child(i18n::TAGLINE),
                            ),
                    )
                    .child(self.mode_badge()),
            )
            .child(div().flex_1());

        bar = bar.child(
            div()
                .flex()
                .items_center()
                .gap(rem(6.))
                .pr(px(8.))
                .child(
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
                        )),
                )
                .child(crate::widgets::text_button(
                    "refresh-all",
                    i18n::action::REFRESH,
                    cx.listener(|this, _, _, cx| {
                        this.refresh();
                        this.notice = Some("数据已刷新".to_owned());
                        cx.notify();
                    }),
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
            .w(px(232.))
            .h_full()
            .min_h_0()
            .gap(px(2.))
            .p(rem(12.))
            .border_r_1()
            .border_color(color(ColorRole::OUTLINE_VARIANT))
            .bg(color(ColorRole::SURFACE_CONTAINER_LOW));

        for page in Page::PRODUCT {
            rail = rail.child(self.nav_item(page.id(), page.label(), page == self.page, page, cx));
        }

        rail = rail.child(
            div()
                .pt(rem(12.))
                .pb(rem(4.))
                .child(crate::widgets::section_header(nav::HOSTS)),
        );

        if self.sessions.is_empty() {
            rail = rail.child(
                div()
                    .px(rem(14.))
                    .py(rem(6.))
                    .text_size(TEXT_AUX)
                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                    .child(i18n::empty::NO_SESSION),
            );
        } else {
            for (index, session) in self.sessions.iter().enumerate() {
                rail = rail.child(self.nav_item(
                    format!("session-{index}"),
                    session.name.clone(),
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
        div()
            .id(SharedString::from(id))
            .flex()
            .items_center()
            .h(px(42.))
            .px(rem(14.))
            .rounded(rem(21.))
            .cursor_pointer()
            .when(selected, |this| {
                this.bg(color(ColorRole::SECONDARY_CONTAINER))
                    .text_color(color(ColorRole::ON_SECONDARY_CONTAINER))
            })
            .when(!selected, |this| {
                this.hover(|this| this.bg(color(ColorRole::SURFACE_CONTAINER_HIGH)))
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                let _ = &target_id;
                this.page = target;
                cx.notify();
            }))
            .child(
                div()
                    .text_size(TEXT_BODY)
                    .font_weight(FontWeight::MEDIUM)
                    .child(label.into()),
            )
            .into_any_element()
    }

    fn render_content(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let body = match self.page {
            Page::Home => self.render_home(window, cx),
            Page::Resources => self.render_resources(window, cx),
            Page::Flows => self.render_flows(window, cx),
            Page::Session(index) => self.render_session(index, window, cx),
            Page::Identities | Page::Snippets | Page::PortForward => {
                self.render_host_placeholder(self.page)
            }
            Page::Settings => self.render_settings(cx),
        };

        let mut column = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .gap(rem(10.))
            .p(rem(20.));

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
            .child(div().flex_1().min_h_0().child(body))
            .into_any_element()
    }

    fn render_host_placeholder(&self, page: Page) -> AnyElement {
        let (title, description) = match page {
            Page::Identities => (
                nav::IDENTITIES,
                "身份会在连接主机时使用：密码或私钥，只存在系统钥匙串里，不进入模型上下文。",
            ),
            Page::Snippets => (
                nav::SNIPPETS,
                "片段是可以一次在多个主机上执行的命令，等 SSH 传输落地后启用。",
            ),
            _ => (
                nav::PORT_FORWARD,
                "端口转发规则（本地/远程），等 SSH 传输落地后启用。",
            ),
        };
        div()
            .flex()
            .flex_col()
            .gap(rem(10.))
            .child(
                div()
                    .text_size(TEXT_TITLE)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(
                div()
                    .text_size(TEXT_BODY)
                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                    .child(description),
            )
            .child(crate::widgets::notice(
                "SSH 传输在 M6 落地（见 docs/ROADMAP.md）；现在点开主机会显示它的真实云资源信息。",
                false,
            ))
            .into_any_element()
    }
}

/// Theme hook: the app only ever paints with roles, so nothing else is needed.
pub fn theme_ready() -> bool {
    theme::color(ColorRole::SURFACE) != theme::color(ColorRole::SURFACE)
}
