//! Host sessions: the 「连接主机」 pages reachable from the resources page.
//!
//! Transport (SSH) is M6; what lands now is the session shell around the real
//! cloud host: which machine it is, how to reach it, and the three surfaces
//! (terminal, files, monitor) it will host.

use crate::app::{Adash, Page};
use crate::i18n::action;
use crate::widgets;
use adash_core::resource::{Resource, money};
use gpui::prelude::*;
use gpui::*;
use gpui_kit_material::prelude::*;

const TABS: [&str; 3] = ["终端", "文件", "监控"];

impl Adash {
    /// One host session, by index.
    pub fn render_session(
        &mut self,
        index: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session) = self.sessions.get(index).cloned() else {
            return widgets::empty_state("会话已关闭", "从「云资源」里重新打开一台主机。");
        };
        let Some(host) = self.inventory.get(&session.resource).cloned() else {
            return widgets::empty_state("主机已不存在", "账户里找不到这台主机了。");
        };

        let mut tabs = div().flex().items_center().gap(rem(6.));
        for (tab, label) in TABS.iter().enumerate() {
            let selected = self.session_tab == tab;
            tabs = tabs.child(widgets::chip(
                SharedString::from(format!("session-tab-{tab}")),
                *label,
                selected,
                cx.listener(move |this, next: &bool, _, cx| {
                    if *next {
                        this.session_tab = tab;
                    }
                    cx.notify();
                }),
            ));
        }

        let ssh = host.public_ip.as_ref().map(|ip| format!("ssh root@{ip}"));

        let surface = match self.session_tab {
            0 => self.render_terminal_surface(&host, ssh.clone(), cx),
            1 => self.render_files_surface(&host),
            _ => self.render_monitor_surface(&host),
        };

        div()
            .flex()
            .flex_col()
            .gap(rem(10.))
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(rem(10.))
                    .child(
                        div()
                            .text_size(TEXT_TITLE)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(host.name.clone()),
                    )
                    .child(widgets::state_pill(host.state))
                    .child(
                        div()
                            .text_size(TEXT_AUX)
                            .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                            .child(format!(
                                "{} · {} · {} · {}",
                                host.region,
                                host.spec,
                                host.project_or_unassigned(),
                                money(host.monthly_cost_cents)
                            )),
                    )
                    .child(div().flex_1())
                    .children(ssh.clone().map(|ssh| {
                        widgets::text_button("copy-ssh", action::COPY, cx.listener(
                            move |this, _, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(ssh.clone()));
                                this.notice = Some("登录命令已复制".to_owned());
                                cx.notify();
                            },
                        ))
                    }))
                    .child(widgets::text_button(
                        "back-to-resources",
                        "返回云资源",
                        cx.listener(|this, _, _, cx| {
                            this.page = Page::Resources;
                            cx.notify();
                        }),
                    )),
            )
            .child(tabs)
            .child(surface)
            .child(
                div()
                    .text_size(TEXT_AUX)
                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                    .child(
                        "SSH 传输（终端、SFTP、监控采集）在 M6 落地；这里的 IP 与规格来自 UCloud 实时数据。",
                    ),
            )
            .into_any_element()
    }

    fn render_terminal_surface(
        &self,
        host: &Resource,
        ssh: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut lines = vec![
            format!("$ # {} @ {}", host.name, host.region),
            format!(
                "$ # {} · {} · {}",
                host.spec,
                host.state.label(),
                host.public_ip
                    .clone()
                    .unwrap_or_else(|| "无公网 IP".to_owned())
            ),
        ];
        if let Some(ssh) = &ssh {
            lines.push(format!("$ {ssh}"));
        } else {
            lines.push("$ # 这台主机没有公网 IP，需要先绑定 EIP 或走跳板机".to_owned());
        }
        lines.push("$ ".to_owned());

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(px(1.))
            .p(rem(14.))
            .rounded(rem(14.))
            .bg(color(ColorRole::SURFACE_CONTAINER_HIGHEST))
            .font_family("monospace")
            .children(lines.into_iter().map(|line| {
                div()
                    .text_size(TEXT_AUX)
                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                    .child(line)
            }))
            .child(div().pt(rem(8.)).child(widgets::tonal_button(
                "terminal-connect",
                "连接（M6）",
                cx.listener(|this, _, _, cx| {
                    this.notice = Some("SSH 传输尚未接入：见 docs/ROADMAP.md M6".to_owned());
                    cx.notify();
                }),
            )))
            .into_any_element()
    }

    fn render_files_surface(&self, host: &Resource) -> AnyElement {
        let rows = [("/", "目录"), ("/var/log", "目录"), ("/etc/nginx", "目录")];
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(px(1.))
            .p(rem(14.))
            .rounded(rem(14.))
            .border_1()
            .border_color(color(ColorRole::OUTLINE_VARIANT))
            .child(
                div()
                    .text_size(TEXT_AUX)
                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                    .child(format!("{} 的文件（SFTP 在 M6 接入）", host.name)),
            )
            .children(rows.into_iter().map(|(path, kind)| {
                div()
                    .flex()
                    .items_center()
                    .gap(rem(10.))
                    .px(rem(8.))
                    .py(px(6.))
                    .child(div().flex_1().text_size(TEXT_BODY).child(path))
                    .child(
                        div()
                            .text_size(TEXT_AUX)
                            .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                            .child(kind),
                    )
            }))
            .into_any_element()
    }

    fn render_monitor_surface(&self, host: &Resource) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(rem(8.))
            .p(rem(14.))
            .rounded(rem(14.))
            .border_1()
            .border_color(color(ColorRole::OUTLINE_VARIANT))
            .child(
                div()
                    .text_size(TEXT_AUX)
                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                    .child("CPU / 内存 / 磁盘 / 网络（监控采集在 M6 接入）"),
            )
            .child(widgets::kv_row(
                "公网 IP",
                host.public_ip.clone().unwrap_or_else(|| "—".to_owned()),
            ))
            .child(widgets::kv_row(
                "内网 IP",
                host.private_ip.clone().unwrap_or_else(|| "—".to_owned()),
            ))
            .child(widgets::kv_row("规格", host.spec.clone()))
            .child(widgets::kv_row("月成本", money(host.monthly_cost_cents)))
            .into_any_element()
    }
}
