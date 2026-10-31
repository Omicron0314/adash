//! SSH pages: saved hosts, identities, snippets, port forwarding and the
//! per-host terminal/files/monitor surfaces. All transport lives in `adash-ssh`.

use crate::app::{Adash, Page};
use crate::widgets;
use adash_ssh::{ForwardDirection, IdentityKind};
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
            let id_edit = id.clone();
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
                .position(|session| !session.closed && session.host.id == id);
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
                                format!("edit-{id}"),
                                "编辑",
                                cx.listener(move |this, _, window, cx| {
                                    this.edit_host(&id_edit, window, cx)
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

        let mut identities = div().flex().gap(rem(8.)).flex_wrap();
        for (identity, title) in std::iter::once((None, "SSH agent".to_owned())).chain(
            self.profiles
                .data
                .identities
                .values()
                .map(|identity| (Some(identity.id.clone()), identity.name.clone())),
        ) {
            let selected = self.host_form.identity == identity;
            identities = identities.child(widgets::chip(
                SharedString::from(format!(
                    "host-identity-{}",
                    identity.as_deref().unwrap_or("agent")
                )),
                title,
                selected,
                cx.listener(move |this, next: &bool, window, cx| {
                    if *next {
                        this.host_form.identity = identity.clone();
                        this.host_form
                            .key_path
                            .update(cx, |state, cx| state.set_value("", window, cx));
                        this.host_form
                            .secret
                            .update(cx, |state, cx| state.set_value("", window, cx));
                    }
                    cx.notify();
                }),
            ));
        }
        let form = div()
            .flex()
            .flex_col()
            .gap(rem(12.))
            .child(
                div()
                    .text_size(TEXT_AUX)
                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                    .child("登录身份"),
            )
            .child(identities)
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
                        if self.host_form.editing.is_some() {
                            "保存修改"
                        } else {
                            "保存主机"
                        },
                        cx.listener(|this, _, window, cx| this.add_host(window, cx)),
                    )),
            )
            .children(self.host_form.editing.as_ref().map(|_| {
                widgets::text_button(
                    "cancel-host-edit",
                    "取消编辑",
                    cx.listener(|this, _, window, cx| this.cancel_host_edit(window, cx)),
                )
            }));

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
                            .child(if self.host_form.editing.is_some() {
                                "编辑主机"
                            } else {
                                "添加主机"
                            }),
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
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(rem(6.))
                            .child(div().text_size(TEXT_AUX).child("命令"))
                            .child(widgets::rounded_area_with_font(
                                "snippet-command",
                                &self.snippet_form.command,
                                if self.store.data.settings.terminal_font.is_empty() {
                                    "monospace".into()
                                } else {
                                    self.store.data.settings.terminal_font.clone().into()
                                },
                                f32::from(self.store.data.settings.terminal_font_size),
                            )),
                    )
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
            let target_host = rule.host_id.clone();
            let target_session = if target_host.is_empty() {
                self.active_session
            } else {
                self.sessions
                    .iter()
                    .position(|session| !session.closed && session.host.id == target_host)
            };
            let running = target_session
                .and_then(|index| self.sessions.get(index))
                .is_some_and(|session| session.forwards.contains_key(&id));
            let starting = target_session
                .and_then(|index| self.sessions.get(index))
                .is_some_and(|session| session.forward_rx.contains_key(&id));
            let error = target_session
                .and_then(|index| self.sessions.get(index))
                .and_then(|session| session.forward_errors.get(&id).cloned());
            let host_name = self
                .profiles
                .data
                .hosts
                .get(&target_host)
                .map_or("当前主机", |host| host.name.as_str())
                .to_owned();
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
                                        "{} · {} · 127.0.0.1:{} → {}:{}",
                                        host_name,
                                        if rule.direction == ForwardDirection::Local {
                                            "本地"
                                        } else {
                                            "远程"
                                        },
                                        rule.bind_port,
                                        rule.target_host,
                                        rule.target_port
                                    )),
                            )
                            .children(error.map(|error| {
                                div()
                                    .text_size(TEXT_AUX)
                                    .text_color(color(ColorRole::ERROR))
                                    .child(error)
                            })),
                    )
                    .child(widgets::tonal_button(
                        format!("toggle-{id}"),
                        if running {
                            "停止"
                        } else if starting {
                            "取消启动"
                        } else {
                            "启动"
                        },
                        cx.listener(move |this, _, window, cx| {
                            let index = if target_host.is_empty() {
                                this.active_session
                            } else {
                                this.sessions.iter().position(|session| {
                                    !session.closed && session.host.id == target_host
                                })
                            };
                            if let Some(index) = index {
                                this.toggle_forward(index, &toggle_id, cx);
                            } else if !target_host.is_empty() {
                                this.open_host(&target_host, window, cx);
                            } else {
                                this.notice = Some("请选择转发连接的主机".into());
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
        let mut host_choices = div().flex().flex_wrap().gap(rem(8.));
        for host in self.profiles.data.hosts.values() {
            let id = host.id.clone();
            let selected = self.forward_form.host_id.as_ref() == Some(&id);
            host_choices = host_choices.child(widgets::chip(
                SharedString::from(format!("forward-target-{}", host.id)),
                host.name.clone(),
                selected,
                cx.listener(move |this, next: &bool, _, cx| {
                    if *next {
                        this.forward_form.host_id = Some(id.clone());
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
            .child(div().text_size(TEXT_AUX).child("转发主机"))
            .child(host_choices)
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
        let Some(session) = self.sessions.get(index).filter(|session| !session.closed) else {
            return widgets::empty_state("会话已关闭", "从「主机」里重新打开一台主机。");
        };
        let host = session.host.clone();
        let status = session.status.clone();
        let pending = session.pending_key.clone();
        let can_terminal = session.connected;

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
            1 => self.render_files_page(index, cx),
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
                    .max_w(rem(440.))
                    .truncate()
                    .child(host.name.clone()),
            )
            .child(
                div()
                    .text_size(TEXT_AUX)
                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                    .child(format!("{}@{}:{}", host.username, host.address, host.port)),
            )
            .child(div().flex_1());
        if can_terminal {
            header = header.child(widgets::text_button(
                "reconnect-ssh",
                "重新连接",
                cx.listener(move |this, _, _, cx| this.connect(index, cx)),
            ));
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
            .children(status.map(|status| {
                let blocked = status.contains("指纹已变更");
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(rem(12.))
                    .p(rem(12.))
                    .rounded(rem(16.))
                    .bg(color(if blocked {
                        ColorRole::ERROR_CONTAINER
                    } else {
                        ColorRole::SURFACE_CONTAINER
                    }))
                    .child(div().flex_1().min_w_0().text_size(TEXT_AUX).child(status))
                    .children(pending.map(|_| {
                        widgets::filled_button(
                            "trust-host-key",
                            "信任并连接",
                            cx.listener(move |this, _, _, cx| this.trust_host_key(index, cx)),
                        )
                    }))
            }))
            .child(tabs)
            .child(surface)
            .into_any_element()
    }

    fn render_terminal(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session) = self.sessions.get(index) else {
            return div().into_any_element();
        };
        let Some(terminal) = session
            .terminal
            .as_ref()
            .filter(|terminal| terminal.is_connected())
        else {
            return div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .gap(rem(12.))
                .child(widgets::empty_state(
                    "终端未连接",
                    "连接后打开交互式 shell。",
                ))
                .child(div().flex().justify_center().child(widgets::filled_button(
                    "connect-terminal",
                    "连接终端",
                    cx.listener(move |this, _, _, cx| this.connect_terminal(index, cx)),
                )))
                .into_any_element();
        };
        let focus = session.focus.clone();
        let key_focus = focus.clone();
        let click_focus = focus.clone();
        let font = f32::from(self.store.data.settings.terminal_font_size.clamp(10, 24));
        let family: SharedString = if self.store.data.settings.terminal_font.is_empty() {
            "monospace".into()
        } else {
            self.store.data.settings.terminal_font.clone().into()
        };
        let font_size = px(font * f32::from(window.rem_size()) / 14.);
        let line_height = font_size * 1.35;
        let font_id = window
            .text_system()
            .resolve_font(&gpui::font(family.clone()));
        let cell_width = window
            .text_system()
            .advance(font_id, font_size, 'M')
            .map_or(font_size * 0.6, |advance| advance.width);
        let parser = terminal.screen.lock().unwrap();
        let cursor = parser.screen().cursor_position();
        let rows = crate::terminal_view::terminal_rows(parser.screen(), focus.is_focused(window));
        drop(parser);
        let native_input = session.terminal_input.clone();
        native_input.update(cx, |input, _| input.sender = Some(terminal.input_sender()));
        let preedit = native_input.read(cx).preedit.clone();
        let ime_focus = focus.clone();
        let inset = window.rem_size();
        let mut contents = div()
            .flex()
            .flex_col()
            .w_full()
            .font_family(family.clone())
            .text_size(font_size)
            .line_height(line_height);
        for row in rows {
            contents = contents.child(
                div()
                    .h(line_height)
                    .flex_shrink_0()
                    .whitespace_nowrap()
                    .child(row.element()),
            );
        }
        let view = cx.entity().downgrade();
        let screen = div()
            .id("terminal-screen")
            .relative()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .p(rem(14.))
            .rounded(rem(20.))
            .bg(color(ColorRole::INVERSE_SURFACE))
            .overflow_hidden()
            .track_focus(&focus)
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if this.modal_open() {
                    return;
                }
                let Some(terminal) = this
                    .sessions
                    .get(index)
                    .and_then(|session| session.terminal.as_ref())
                else {
                    return;
                };
                let key = event.keystroke.key.as_str();
                // Printable input is committed by the platform text handler,
                // including Chinese IME; handling it here would duplicate text.
                let printable = event
                    .keystroke
                    .key_char
                    .as_ref()
                    .is_some_and(|text| !text.chars().any(char::is_control));
                if printable
                    && (event.prefer_character_input
                        || (!event.keystroke.modifiers.control && !event.keystroke.modifiers.alt))
                    || !event.keystroke.modifiers.control
                        && !event.keystroke.modifiers.alt
                        && !this.sessions[index]
                            .terminal_input
                            .read(cx)
                            .preedit
                            .is_empty()
                {
                    return;
                }
                if event.keystroke.modifiers.control
                    && event.keystroke.modifiers.shift
                    && key == "c"
                {
                    cx.write_to_clipboard(ClipboardItem::new_string(terminal.contents()));
                } else if event.keystroke.modifiers.control && key == "v" {
                    if let Some(text) = cx
                        .read_from_clipboard()
                        .and_then(|clipboard| clipboard.text())
                    {
                        let bracketed = terminal.screen.lock().unwrap().screen().bracketed_paste();
                        let text = if bracketed {
                            format!("\u{1b}[200~{text}\u{1b}[201~")
                        } else {
                            text
                        };
                        terminal.input(text.into_bytes());
                    }
                } else {
                    let application = terminal
                        .screen
                        .lock()
                        .unwrap()
                        .screen()
                        .application_cursor();
                    if let Some(bytes) = crate::terminal_view::key_bytes(event, application) {
                        terminal.input(bytes);
                    } else {
                        return;
                    }
                }
                window.focus(&key_focus, cx);
                cx.stop_propagation();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_, _, window, cx| {
                    click_focus.focus(window, cx);
                }),
            )
            .on_scroll(cx.listener(move |this, event: &ScrollEvent, _, cx| {
                if let Some(terminal) = this
                    .sessions
                    .get(index)
                    .and_then(|session| session.terminal.as_ref())
                {
                    let delta =
                        f32::from(event.delta.pixel_delta(line_height).y) / f32::from(line_height);
                    terminal.scroll(delta.round() as i32);
                    cx.stop_propagation();
                }
            }))
            .child(contents)
            .children((!preedit.is_empty()).then(|| {
                div()
                    .absolute()
                    .left(inset + cell_width * f32::from(cursor.1))
                    .top(inset + line_height * f32::from(cursor.0))
                    .font_family(family)
                    .text_size(font_size)
                    .line_height(line_height)
                    .text_color(color(ColorRole::INVERSE_ON_SURFACE))
                    .bg(color(ColorRole::INVERSE_SURFACE))
                    .border_b_1()
                    .border_color(color(ColorRole::PRIMARY))
                    .child(preedit)
            }))
            .child(
                canvas(
                    |bounds, _, _| bounds,
                    move |bounds, _, window, cx| {
                        native_input.update(cx, |input, _| {
                            input.bounds = Bounds::new(
                                point(
                                    bounds.origin.x + cell_width * f32::from(cursor.1),
                                    bounds.origin.y + line_height * f32::from(cursor.0),
                                ),
                                size(cell_width, line_height),
                            )
                        });
                        window.handle_input(
                            &ime_focus,
                            ElementInputHandler::new(bounds, native_input.clone()),
                            cx,
                        );
                        let rows = (f32::from(bounds.size.height) / f32::from(line_height))
                            .floor()
                            .max(2.) as u16;
                        let columns = (f32::from(bounds.size.width) / f32::from(cell_width))
                            .floor()
                            .max(2.) as u16;
                        let _ = view.update(cx, |this, _| {
                            if let Some(terminal) = this
                                .sessions
                                .get(index)
                                .and_then(|session| session.terminal.as_ref())
                            {
                                terminal.resize(rows, columns);
                            }
                        });
                    },
                )
                .absolute()
                .top(rem(14.))
                .bottom(rem(14.))
                .left(rem(14.))
                .right(rem(14.)),
            );
        let mut keys = div().flex().gap(rem(6.)).flex_wrap();
        for (name, bytes) in [
            ("Ctrl+C", "\u{3}"),
            ("Ctrl+D", "\u{4}"),
            ("Tab", "\t"),
            ("Esc", "\u{1b}"),
        ] {
            let bytes = bytes.to_owned();
            keys = keys.child(widgets::tonal_button(
                format!("key-{name}"),
                name,
                cx.listener(move |this, _, _, _| {
                    if let Some(terminal) = this
                        .sessions
                        .get(index)
                        .and_then(|session| session.terminal.as_ref())
                    {
                        terminal.input(bytes.clone().into_bytes());
                    }
                }),
            ));
        }
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
                        "{:.1} / {:.1} GiB",
                        sample.memory_total.saturating_sub(sample.memory_available) as f64
                            / 1073741824.,
                        sample.memory_total as f64 / 1073741824.
                    )),
                ))
                .child(widgets::stat_card(
                    "mon-disk",
                    "磁盘",
                    format!(
                        "{:.0} %",
                        100. * sample.disk_used_kib as f64 / sample.disk_total_kib.max(1) as f64
                    ),
                    Some(format!(
                        "{:.1} GiB",
                        sample.disk_total_kib as f64 / 1048576.
                    )),
                ))
                .child(widgets::stat_card(
                    "mon-net",
                    "网络累计",
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
