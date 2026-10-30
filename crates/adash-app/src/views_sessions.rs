//! SSH pages: saved hosts, identities, snippets, port forwarding and the
//! per-host terminal/files/monitor surfaces. All transport lives in `adash-ssh`.

use crate::app::{Adash, Page};
use crate::widgets;
use adash_ssh::{ForwardDirection, IdentityKind, RemoteFile};
use gpui::prelude::*;
use gpui::*;
use gpui_kit_material::prelude::*;

const TABS: [&str; 3] = ["终端", "文件", "监控"];

fn field(
    id: &'static str,
    label: &'static str,
    state: &Entity<gpui::base::input::InputState>,
) -> AnyElement {
    div()
        .flex_1()
        .min_w(rem(160.))
        .child(widgets::labelled_input(id, label, state))
        .into_any_element()
}

impl Adash {
    /// Saved hosts plus the add form.
    pub fn render_hosts(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let mut hosts: Vec<_> = self.profiles.data.hosts.values().cloned().collect();
        hosts.sort_by_key(|host| {
            (
                self.profiles
                    .data
                    .recent
                    .iter()
                    .position(|id| id == &host.id)
                    .unwrap_or(usize::MAX),
                host.name.clone(),
            )
        });
        let mut list = div().flex().flex_col().gap(rem(12.));
        if hosts.is_empty() {
            list = list.child(widgets::empty_state(
                "还没有保存的主机",
                "在下面添加一台，或从「云资源」打开一台主机的终端。",
            ));
        }
        for host in hosts {
            let id = host.id.clone();
            let id_open = id.clone();
            let id_delete = id.clone();
            let select_id = id.clone();
            let selected = self.selected_hosts.contains(&id);
            let identity = host
                .identity
                .as_ref()
                .and_then(|identity| self.profiles.data.identities.get(identity))
                .map_or_else(|| "SSH agent".to_owned(), |identity| identity.name.clone());
            let open = self
                .sessions
                .iter()
                .position(|session| session.host.id == id);
            list = list.child(
                CardSurface::new(CardVariant::Filled)
                    .bg(color(ColorRole::SURFACE_CONTAINER_LOWEST))
                    .rounded(rem(24.))
                    .p(rem(20.))
                    .gap(rem(12.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(rem(10.))
                            .child(widgets::resource_icon(adash_core::ResourceKind::UHost))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .text_size(rem(16.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(host.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_size(TEXT_AUX)
                                            .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                                            .child(format!(
                                                "{}@{}:{} · {}",
                                                host.username, host.address, host.port, identity
                                            )),
                                    ),
                            )
                            .child(widgets::chip(
                                format!("select-{id}"),
                                "批量",
                                selected,
                                cx.listener(move |this, next: &bool, _, cx| {
                                    if *next {
                                        this.selected_hosts.insert(select_id.clone());
                                    } else {
                                        this.selected_hosts.remove(&select_id);
                                    }
                                    cx.notify();
                                }),
                            ))
                            .child(widgets::text_button(
                                format!("open-{id}"),
                                if open.is_some() {
                                    "查看会话"
                                } else {
                                    "连接"
                                },
                                cx.listener(move |this, _, window, cx| {
                                    this.open_host(&id_open, window, cx);
                                }),
                            ))
                            .child(widgets::text_button(
                                format!("delete-{id}"),
                                "删除",
                                cx.listener(move |this, _, _, cx| this.delete_host(&id_delete, cx)),
                            )),
                    ),
            );
        }

        let form = div()
            .flex()
            .flex_col()
            .gap(rem(12.))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(rem(12.))
                    .child(field("host-name", "名称", &self.host_form.name))
                    .child(field("host-address", "地址", &self.host_form.address)),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(rem(12.))
                    .child(field("host-port", "端口", &self.host_form.port))
                    .child(field("host-user", "用户", &self.host_form.user))
                    .child(field(
                        "host-key",
                        "私钥路径（可选）",
                        &self.host_form.key_path,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_end()
                    .gap(rem(12.))
                    .child(field(
                        "host-secret",
                        "密码或口令（可选）",
                        &self.host_form.secret,
                    ))
                    .child(widgets::filled_button(
                        "add-host",
                        "保存主机",
                        cx.listener(|this, _, window, cx| this.add_host(window, cx)),
                    )),
            );

        div()
            .id("hosts-scroll")
            .flex()
            .flex_col()
            .gap(rem(18.))
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(
                div()
                    .text_size(rem(40.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("主机"),
            )
            .children(
                self.profile_error
                    .clone()
                    .map(|error| widgets::notice(format!("主机配置无法读取：{error}"), true)),
            )
            .child(
                CardSurface::new(CardVariant::Filled)
                    .bg(color(ColorRole::SURFACE_CONTAINER_LOWEST))
                    .rounded(rem(24.))
                    .p(rem(20.))
                    .gap(rem(12.))
                    .child(
                        div()
                            .text_size(rem(20.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("添加主机"),
                    )
                    .child(form),
            )
            .child(
                div()
                    .text_size(rem(20.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("已保存"),
            )
            .child(list)
            .into_any_element()
    }

    /// Identities with their keyring-backed secrets.
    pub fn render_identities(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let mut list = div().flex().flex_col().gap(rem(8.));
        if self.profiles.data.identities.is_empty() {
            list = list.child(widgets::empty_state(
                "还没有身份",
                "添加密码或私钥身份；密钥只写入系统钥匙串。",
            ));
        }
        for identity in self.profiles.data.identities.values().cloned() {
            let id = identity.id.clone();
            let delete_id = id.clone();
            let kind = match identity.kind {
                IdentityKind::Agent => "SSH agent",
                IdentityKind::Password => "密码",
                IdentityKind::PrivateKey => "私钥",
            };
            let path = identity
                .key_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default();
            list = list.child(
                CardSurface::new(CardVariant::Filled)
                    .bg(color(ColorRole::SURFACE_CONTAINER_LOWEST))
                    .rounded(rem(16.))
                    .px(rem(16.))
                    .py(rem(12.))
                    .flex()
                    .items_center()
                    .gap(rem(10.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(TEXT_BODY)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(identity.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(TEXT_AUX)
                                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                                    .child(if path.is_empty() {
                                        kind.to_owned()
                                    } else {
                                        format!("{kind} · {path}")
                                    }),
                            ),
                    )
                    .child(widgets::text_button(
                        format!("delete-identity-{id}"),
                        "删除",
                        cx.listener(move |this, _, _, cx| this.delete_identity(&delete_id, cx)),
                    )),
            );
        }
        let mut kinds = div().flex().gap(rem(8.));
        for (kind, name) in [
            (IdentityKind::Agent, "SSH agent"),
            (IdentityKind::Password, "密码"),
            (IdentityKind::PrivateKey, "私钥"),
        ] {
            let selected = self.identity_kind == kind;
            let target = kind.clone();
            kinds = kinds.child(widgets::chip(
                format!("identity-kind-{name}"),
                name,
                selected,
                cx.listener(move |this, next: &bool, _, cx| {
                    if *next {
                        this.identity_kind = target.clone();
                    }
                    cx.notify();
                }),
            ));
        }
        div()
            .id("identities-scroll")
            .flex()
            .flex_col()
            .gap(rem(18.))
            .max_w(rem(760.))
            .overflow_y_scroll()
            .child(
                div()
                    .text_size(rem(40.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("身份"),
            )
            .child(kinds)
            .child(
                CardSurface::new(CardVariant::Filled)
                    .bg(color(ColorRole::SURFACE_CONTAINER_LOWEST))
                    .rounded(rem(24.))
                    .p(rem(20.))
                    .gap(rem(10.))
                    .child(
                        div()
                            .text_size(rem(20.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("添加身份"),
                    )
                    .child(field("identity-name", "名称", &self.identity_form.name))
                    .child(field(
                        "identity-key",
                        "私钥文件（私钥身份必填）",
                        &self.identity_form.key_path,
                    ))
                    .child(field(
                        "identity-secret",
                        "密码或口令（写入系统钥匙串）",
                        &self.identity_form.secret,
                    ))
                    .child(div().flex().justify_end().child(widgets::filled_button(
                        "add-identity",
                        "保存身份",
                        cx.listener(|this, _, window, cx| this.add_identity(window, cx)),
                    ))),
            )
            .child(list)
            .into_any_element()
    }

    /// Snippets and their output on the selected host.
    pub fn render_snippets(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let mut list = div().flex().flex_col().gap(rem(8.));
        if self.profiles.data.snippets.is_empty() {
            list = list.child(widgets::empty_state(
                "还没有片段",
                "把常用的运维命令保存下来，在任意主机上一次执行。",
            ));
        }
        for snippet in self.profiles.data.snippets.values().cloned() {
            let id = snippet.id.clone();
            let delete_id = id.clone();
            let command = snippet.command.clone();
            list = list.child(
                CardSurface::new(CardVariant::Filled)
                    .bg(color(ColorRole::SURFACE_CONTAINER_LOWEST))
                    .rounded(rem(16.))
                    .px(rem(16.))
                    .py(rem(12.))
                    .flex()
                    .items_center()
                    .gap(rem(10.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(TEXT_BODY)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(snippet.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(TEXT_AUX)
                                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                                    .child(command.clone()),
                            ),
                    )
                    .child(widgets::text_button(
                        format!("run-{id}"),
                        "当前主机",
                        cx.listener({
                            let command = command.clone();
                            move |this, _, _, cx| {
                                if let Some(index) = this.active_session {
                                    this.run_on_host(index, command.clone(), cx);
                                } else {
                                    this.notice = Some("先打开一台主机的会话".to_owned());
                                    cx.notify();
                                }
                            }
                        }),
                    ))
                    .child(widgets::text_button(
                        format!("batch-{id}"),
                        "批量执行",
                        cx.listener(move |this, _, _, cx| {
                            this.run_snippet_batch(command.clone(), cx);
                        }),
                    ))
                    .child(widgets::text_button(
                        format!("delete-snippet-{id}"),
                        "删除",
                        cx.listener(move |this, _, _, cx| this.delete_snippet(&delete_id, cx)),
                    )),
            );
        }
        let output = self
            .sessions
            .iter()
            .find_map(|session| session.exec_output.clone());
        div()
            .id("snippets-scroll")
            .flex()
            .flex_col()
            .gap(rem(18.))
            .max_w(rem(820.))
            .overflow_y_scroll()
            .child(
                div()
                    .text_size(rem(40.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("片段"),
            )
            .child(
                CardSurface::new(CardVariant::Filled)
                    .bg(color(ColorRole::SURFACE_CONTAINER_LOWEST))
                    .rounded(rem(24.))
                    .p(rem(20.))
                    .gap(rem(10.))
                    .child(
                        div()
                            .text_size(rem(20.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("添加片段"),
                    )
                    .child(field("snippet-name", "名称", &self.snippet_form.name))
                    .child(field("snippet-command", "命令", &self.snippet_form.command))
                    .child(div().flex().justify_end().child(widgets::filled_button(
                        "add-snippet",
                        "保存片段",
                        cx.listener(|this, _, window, cx| {
                            this.add_snippet(window, cx);
                        }),
                    ))),
            )
            .child(list)
            .children((!self.batch_output.is_empty()).then(|| {
                CardSurface::new(CardVariant::Filled)
                    .bg(color(ColorRole::INVERSE_SURFACE))
                    .rounded(rem(16.))
                    .p(rem(16.))
                    .gap(rem(6.))
                    .child(
                        div()
                            .text_size(TEXT_AUX)
                            .text_color(color(ColorRole::INVERSE_ON_SURFACE))
                            .child(if self.batch_running > 0 {
                                format!("批量执行中：剩余 {}", self.batch_running)
                            } else {
                                "批量结果".to_owned()
                            }),
                    )
                    .children(self.batch_output.iter().map(|text| {
                        div()
                            .font_family("monospace")
                            .text_size(TEXT_AUX)
                            .text_color(color(ColorRole::INVERSE_ON_SURFACE))
                            .child(text.clone())
                    }))
            }))
            .children(output.map(|output| {
                CardSurface::new(CardVariant::Filled)
                    .bg(color(ColorRole::INVERSE_SURFACE))
                    .rounded(rem(16.))
                    .p(rem(16.))
                    .child(
                        div()
                            .font_family("monospace")
                            .text_size(TEXT_AUX)
                            .text_color(color(ColorRole::INVERSE_ON_SURFACE))
                            .child(output),
                    )
            }))
            .into_any_element()
    }

    /// Port-forward rules; they bind to the open session's host.
    pub fn render_port_forward(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let mut list = div().flex().flex_col().gap(rem(8.));
        if self.profiles.data.forwards.is_empty() {
            list = list.child(widgets::empty_state(
                "还没有端口转发",
                "规则只绑定 127.0.0.1，不会暴露到局域网。",
            ));
        }
        let active = if let Some(index) = self.active_session {
            self.sessions
                .get(index)
                .map(|session| session.host.name.clone())
        } else {
            None
        };
        for rule in self.profiles.data.forwards.values().cloned() {
            let id = rule.id.clone();
            let delete_id = id.clone();
            let toggle_id = id.clone();
            let running = if let Some(index) = self.active_session {
                self.sessions.get(index).is_some_and(|session| {
                    session.forward.is_some() && session.forward_rule.as_deref() == Some(&rule.id)
                })
            } else {
                false
            };
            list = list.child(
                CardSurface::new(CardVariant::Filled)
                    .bg(color(ColorRole::SURFACE_CONTAINER_LOWEST))
                    .rounded(rem(16.))
                    .px(rem(16.))
                    .py(rem(12.))
                    .flex()
                    .items_center()
                    .gap(rem(10.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .child(
                                div()
                                    .text_size(TEXT_BODY)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(rule.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(TEXT_AUX)
                                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                                    .child(format!(
                                        "127.0.0.1:{} → {}:{}",
                                        rule.bind_port, rule.target_host, rule.target_port
                                    )),
                            ),
                    )
                    .child(widgets::tonal_button(
                        format!("toggle-{id}"),
                        if running { "停止" } else { "启动" },
                        cx.listener(move |this, _, _, cx| {
                            if let Some(index) = this.active_session {
                                this.toggle_forward(index, &toggle_id, cx);
                            } else {
                                this.notice = Some("先打开一台主机的会话".to_owned());
                                cx.notify();
                            }
                        }),
                    ))
                    .child(widgets::text_button(
                        format!("delete-forward-{id}"),
                        "删除",
                        cx.listener(move |this, _, _, cx| this.delete_forward(&delete_id, cx)),
                    )),
            );
        }
        let mut directions = div().flex().gap(rem(8.));
        for (direction, name) in [
            (ForwardDirection::Local, "本地转发"),
            (ForwardDirection::Remote, "远程转发"),
        ] {
            let selected = self.forward_direction == direction;
            let target = direction;
            directions = directions.child(widgets::chip(
                format!("forward-direction-{name}"),
                name,
                selected,
                cx.listener(move |this, next: &bool, _, cx| {
                    if *next {
                        this.forward_direction = target;
                    }
                    cx.notify();
                }),
            ));
        }
        div()
            .id("forward-scroll")
            .flex()
            .flex_col()
            .gap(rem(18.))
            .max_w(rem(820.))
            .overflow_y_scroll()
            .child(
                div()
                    .text_size(rem(40.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("端口转发"),
            )
            .children(active.map(|name| {
                div()
                    .text_size(TEXT_AUX)
                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                    .child(format!("当前会话：{name}"))
            }))
            .child(directions)
            .child(
                CardSurface::new(CardVariant::Filled)
                    .bg(color(ColorRole::SURFACE_CONTAINER_LOWEST))
                    .rounded(rem(24.))
                    .p(rem(20.))
                    .gap(rem(10.))
                    .child(
                        div()
                            .text_size(rem(20.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("添加规则"),
                    )
                    .child(field("forward-name", "名称", &self.forward_form.name))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(rem(12.))
                            .child(field(
                                "forward-bind",
                                "本地端口",
                                &self.forward_form.bind_port,
                            ))
                            .child(field(
                                "forward-host",
                                "目标主机",
                                &self.forward_form.target_host,
                            ))
                            .child(field(
                                "forward-target",
                                "目标端口",
                                &self.forward_form.target_port,
                            )),
                    )
                    .child(div().flex().justify_end().child(widgets::filled_button(
                        "add-forward",
                        "保存规则",
                        cx.listener(|this, _, window, cx| this.add_forward(window, cx)),
                    ))),
            )
            .child(list)
            .into_any_element()
    }

    /// One host session: header, tabs and the selected surface.
    pub fn render_session(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session) = self.sessions.get(index) else {
            return widgets::empty_state("会话已关闭", "从「主机」里重新打开一台主机。");
        };
        let host = session.host.clone();
        let status = session.status.clone();
        let pending = session.pending_key.clone();
        let can_terminal = session.worker.is_some();

        let mut tabs = div().flex().items_center().gap(rem(6.));
        for (tab, name) in TABS.iter().enumerate() {
            tabs = tabs.child(widgets::chip(
                format!("session-tab-{tab}"),
                *name,
                self.session_tab == tab,
                cx.listener(move |this, next: &bool, _, cx| {
                    if *next {
                        this.session_tab = tab;
                    }
                    cx.notify();
                }),
            ));
        }

        let surface = match self.session_tab {
            0 => self.render_terminal(index, window, cx),
            1 => self.render_files(index, window, cx),
            _ => self.render_monitor(index, cx),
        };
        let mut header = div()
            .flex()
            .items_center()
            .gap(rem(10.))
            .child(
                div()
                    .text_size(rem(32.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(host.name.clone()),
            )
            .child(
                div()
                    .text_size(TEXT_AUX)
                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                    .child(format!("{}@{}:{}", host.username, host.address, host.port)),
            )
            .children(status.map(|status| {
                div()
                    .text_size(TEXT_AUX)
                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                    .child(status)
            }))
            .child(div().flex_1());
        if pending.is_some() {
            header = header.child(widgets::filled_button(
                "trust-host-key",
                "信任并连接",
                cx.listener(move |this, _, _, cx| this.trust_host_key(index, cx)),
            ));
        }
        if can_terminal {
            header = header.child(widgets::text_button(
                "sample-host",
                "刷新",
                cx.listener(move |this, _, _, cx| {
                    this.refresh_files(index, cx);
                    this.sample_monitor(index, cx);
                }),
            ));
        } else {
            header = header.child(widgets::text_button(
                "reconnect-host",
                "连接",
                cx.listener(move |this, _, _, cx| this.connect(index, cx)),
            ));
        }
        header = header.child(widgets::text_button(
            "close-session",
            "关闭会话",
            cx.listener(move |this, _, _, cx| this.close_session(index, cx)),
        ));
        header = header.child(widgets::text_button(
            "back-to-hosts",
            "返回主机",
            cx.listener(|this, _, _, cx| {
                this.page = Page::Hosts;
                cx.notify();
            }),
        ));

        div()
            .flex()
            .flex_col()
            .gap(rem(12.))
            .flex_1()
            .min_h_0()
            .child(header)
            .child(tabs)
            .child(surface)
            .into_any_element()
    }

    fn render_terminal(
        &mut self,
        index: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session) = self.sessions.get(index) else {
            return div().into_any_element();
        };
        if session.terminal.is_none() {
            return div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .gap(rem(12.))
                .child(widgets::empty_state(
                    "终端未连接",
                    "连接后会打开一个真正的交互式 shell。",
                ))
                .child(div().flex().justify_center().child(widgets::filled_button(
                    "connect-terminal",
                    "连接终端",
                    cx.listener(move |this, _, _, cx| this.connect_terminal(index, cx)),
                )))
                .into_any_element();
        }
        let terminal = session.terminal.as_ref().expect("checked");
        let focus = session.focus.clone();
        let key_focus = session.focus.clone();
        let click_focus = session.focus.clone();
        let lines = terminal_rows(terminal);
        let font = self.store.data.settings.terminal_font_size.clamp(10, 24) as f32;
        let mut screen = div()
            .id("terminal-screen")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .p(rem(14.))
            .rounded(rem(16.))
            .bg(color(ColorRole::INVERSE_SURFACE))
            .overflow_y_scroll()
            .track_focus(&focus)
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if let Some(bytes) = key_bytes(event) {
                    if let Some(session) = this.sessions.get(index)
                        && let Some(terminal) = &session.terminal
                    {
                        terminal.input(bytes);
                    }
                    window.focus(&key_focus, cx);
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_, _, window, cx| {
                    click_focus.focus(window, cx);
                    cx.notify();
                }),
            );
        for (text, role) in lines {
            screen = screen.child(
                div()
                    .font_family("monospace")
                    .text_size(rem(font))
                    .text_color(color(role))
                    .child(text),
            );
        }
        let terminal_ref = session.terminal.as_ref().expect("checked");
        let mut keys = div().flex().gap(rem(6.)).flex_wrap();
        for (name, bytes) in [
            ("Ctrl+C", "\u{3}"),
            ("Ctrl+D", "\u{4}"),
            ("Tab", "\t"),
            ("Esc", "\u{1b}"),
            ("↑", "\u{1b}[A"),
            ("↓", "\u{1b}[B"),
            ("←", "\u{1b}[D"),
            ("→", "\u{1b}[C"),
        ] {
            let bytes = bytes.to_owned();
            keys = keys.child(widgets::tonal_button(
                format!("key-{name}"),
                name,
                cx.listener(move |this, _, _, cx| {
                    if let Some(session) = this.sessions.get(index)
                        && let Some(terminal) = &session.terminal
                    {
                        terminal.input(bytes.clone().into_bytes());
                    }
                    cx.notify();
                }),
            ));
        }
        let _ = terminal_ref;
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(rem(8.))
            .child(screen)
            .child(keys)
            .into_any_element()
    }

    fn render_files(
        &mut self,
        index: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session) = self.sessions.get(index) else {
            return div().into_any_element();
        };
        let directory = session.directory.clone();
        let entries = session.files.clone().unwrap_or_default();
        let mut rows = div().flex().flex_col();
        if entries.is_empty() {
            rows = rows.child(widgets::empty_state(
                "目录为空或尚未读取",
                "连接后可刷新目录。",
            ));
        }
        for entry in &entries {
            rows = rows.child(file_row(index, entry, cx));
        }
        if let Some(error) = session.file_error.clone() {
            rows = rows.child(widgets::notice(error, true));
        }
        let editor_path = session.editor_path.clone();
        let editor = session.editor.clone();
        let mut editor_panel = div().flex().flex_col().gap(rem(8.)).flex_1().min_w_0();
        if let Some(path) = editor_path {
            editor_panel =
                editor_panel
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(rem(8.))
                            .child(
                                div()
                                    .text_size(TEXT_AUX)
                                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                                    .child(path.display().to_string()),
                            )
                            .child(div().flex_1())
                            .child(widgets::filled_button(
                                "save-file",
                                "保存",
                                cx.listener(move |this, _, _, cx| this.save_file(index, cx)),
                            )),
                    )
                    .child(div().flex_1().min_h_0().overflow_hidden().child(
                        widgets::rounded_area("file-editor", &editor, Some("文件内容")),
                    ));
        }
        let upload = div()
            .flex()
            .items_center()
            .gap(rem(8.))
            .child(widgets::tonal_button(
                "upload-file",
                "上传本地文件",
                cx.listener(move |this, _, _, cx| this.upload_pick(index, cx)),
            ));
        let _ = &upload;
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(rem(10.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(rem(8.))
                    .child(
                        div()
                            .text_size(TEXT_BODY)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(directory.display().to_string()),
                    )
                    .child(div().flex_1())
                    .child(widgets::text_button(
                        "refresh-files",
                        "刷新",
                        cx.listener(move |this, _, _, cx| this.refresh_files(index, cx)),
                    )),
            )
            .child(
                div()
                    .flex()
                    .gap(rem(12.))
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .id("file-list")
                            .flex()
                            .flex_col()
                            .w(rem(300.))
                            .overflow_y_scroll()
                            .child(rows),
                    )
                    .child(editor_panel),
            )
            .into_any_element()
    }

    fn render_monitor(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(session) = self.sessions.get(index) else {
            return div().into_any_element();
        };
        let sample = session.monitor.clone();
        let previous = session.previous_monitor.clone();
        let error = session.monitor_error.clone();
        let interval = self.store.data.settings.monitor_interval;
        let mut grid = div().flex().gap(rem(12.)).flex_wrap();
        if let Some(sample) = sample {
            let cpu = previous
                .as_ref()
                .and_then(|previous| sample.cpu_percent(previous));
            grid = grid
                .child(widgets::stat_card(
                    "mon-cpu",
                    "CPU",
                    cpu.map_or_else(|| "—".to_owned(), |value| format!("{value:.0} %")),
                    Some(format!("负载 {}", sample.load)),
                ))
                .child(widgets::stat_card(
                    "mon-mem",
                    "内存",
                    format!("{:.0} %", sample.memory_percent()),
                    Some(format!(
                        "{} / {} GiB",
                        sample.memory_total.saturating_sub(sample.memory_available) / 1024 / 1024,
                        sample.memory_total / 1024 / 1024
                    )),
                ))
                .child(widgets::stat_card(
                    "mon-disk",
                    "磁盘",
                    format!(
                        "{:.0} %",
                        100. * sample.disk_used_kib as f64 / sample.disk_total_kib.max(1) as f64
                    ),
                    Some(format!("{} GiB", sample.disk_total_kib / 1024)),
                ))
                .child(widgets::stat_card(
                    "mon-net",
                    "网络",
                    format!("↓ {:.1} MiB", sample.network_received as f64 / 1048576.),
                    Some(format!(
                        "↑ {:.1} MiB",
                        sample.network_sent as f64 / 1048576.
                    )),
                ))
                .child(widgets::stat_card(
                    "mon-up",
                    "运行时间",
                    format!("{:.1} h", sample.uptime_seconds / 3600.),
                    Some(format!("每 {interval} 秒刷新")),
                ));
        } else {
            grid = grid.child(widgets::empty_state(
                "还没有监控数据",
                "连接后每 10 秒读取一次 /proc。",
            ));
        }
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(rem(12.))
            .children(error.map(|error| widgets::notice(error, true)))
            .child(div().flex().justify_end().child(widgets::text_button(
                "sample-now",
                "立即采样",
                cx.listener(move |this, _, _, cx| this.sample_monitor(index, cx)),
            )))
            .child(grid)
            .into_any_element()
    }
}

fn file_row(index: usize, entry: &RemoteFile, cx: &mut Context<Adash>) -> AnyElement {
    let path = entry.path.clone();
    let directory = entry.directory;
    let name = entry.name.clone();
    let size = entry.size;
    let open_path = path.clone();
    let download_path = path.clone();
    let delete_path = path.clone();
    div()
        .id(SharedString::from(format!("file-{}", path.display())))
        .flex()
        .items_center()
        .gap(rem(6.))
        .px(rem(10.))
        .py(px(4.))
        .rounded(rem(8.))
        .hover(|this| this.bg(color(ColorRole::SURFACE_CONTAINER_HIGH)))
        .child(
            div()
                .id(SharedString::from(format!("file-open-{}", path.display())))
                .flex()
                .items_center()
                .gap(rem(8.))
                .flex_1()
                .min_w_0()
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    if directory {
                        if let Some(session) = this.sessions.get_mut(index) {
                            session.directory = open_path.clone();
                        }
                        this.refresh_files(index, cx);
                    } else {
                        this.open_file(index, open_path.clone(), cx);
                    }
                }))
                .child(
                    div()
                        .text_size(TEXT_AUX)
                        .child(if directory { "▸" } else { "·" }),
                )
                .child(div().flex_1().min_w_0().text_size(TEXT_BODY).child(name))
                .child(
                    div()
                        .text_size(TEXT_AUX)
                        .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                        .child(if directory {
                            String::new()
                        } else {
                            human_size(size)
                        }),
                ),
        )
        .children((!directory).then(|| {
            widgets::text_button(
                format!("file-download-{}", download_path.display()),
                "↓",
                cx.listener(move |this, _, _, cx| {
                    this.download_pick(index, download_path.clone(), cx);
                }),
            )
        }))
        .children((!directory).then(|| {
            widgets::text_button(
                format!("file-delete-{}", delete_path.display()),
                "×",
                cx.listener(move |this, _, _, cx| {
                    this.pending_remote_delete = Some((index, delete_path.clone()));
                    cx.notify();
                }),
            )
        }))
        .into_any_element()
}

fn human_size(size: u64) -> String {
    if size >= 1024 * 1024 {
        format!("{:.1} MiB", size as f64 / 1048576.)
    } else if size >= 1024 {
        format!("{:.1} KiB", size as f64 / 1024.)
    } else {
        format!("{size} B")
    }
}

/// Terminal rows with a semantic colour per run. The renderer only uses roles.
fn terminal_rows(terminal: &adash_ssh::Terminal) -> Vec<(String, ColorRole)> {
    let screen = terminal.screen.lock().expect("terminal parser poisoned");
    let (rows, cols) = screen.screen().size();
    let mut lines = Vec::new();
    for row in 0..rows {
        let mut run = String::new();
        let mut role = ColorRole::INVERSE_ON_SURFACE;
        for column in 0..cols {
            let Some(cell) = screen.screen().cell(row, column) else {
                break;
            };
            let current = match cell.fgcolor() {
                vt100::Color::Default => ColorRole::INVERSE_ON_SURFACE,
                vt100::Color::Idx(1 | 9) => ColorRole::ERROR,
                vt100::Color::Idx(2 | 10) => ColorRole::SUCCESS,
                vt100::Color::Idx(3 | 11) => ColorRole::WARNING,
                vt100::Color::Idx(4 | 12 | 6 | 14) => ColorRole::PRIMARY,
                vt100::Color::Idx(5 | 13) => ColorRole::TERTIARY,
                _ => ColorRole::INVERSE_ON_SURFACE,
            };
            if current != role && !run.is_empty() {
                lines.push((std::mem::take(&mut run), role));
            }
            role = current;
            run.push_str(cell.contents());
        }
        if run.is_empty() {
            lines.push((String::new(), ColorRole::INVERSE_ON_SURFACE));
        } else {
            lines.push((run, role));
        }
    }
    lines
}

/// Translate a key press into the bytes a terminal expects.
fn key_bytes(event: &KeyDownEvent) -> Option<Vec<u8>> {
    let modifiers = event.keystroke.modifiers;
    let key = event.keystroke.key.as_str();
    if modifiers.control {
        return match key {
            "c" => Some(vec![0x03]),
            "d" => Some(vec![0x04]),
            "l" => Some(vec![0x0c]),
            "u" => Some(vec![0x15]),
            "z" => Some(vec![0x1a]),
            _ => None,
        };
    }
    Some(match key {
        "enter" => vec![b'\r'],
        "backspace" => vec![0x7f],
        "tab" => vec![b'\t'],
        "escape" => vec![0x1b],
        "up" => b"\x1b[A".to_vec(),
        "down" => b"\x1b[B".to_vec(),
        "right" => b"\x1b[C".to_vec(),
        "left" => b"\x1b[D".to_vec(),
        "home" => b"\x1b[H".to_vec(),
        "end" => b"\x1b[F".to_vec(),
        "delete" => b"\x1b[3~".to_vec(),
        _ => event.keystroke.key_char.as_ref()?.as_bytes().to_vec(),
    })
}
