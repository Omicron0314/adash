//! Settings: the catussh sections that map to real capabilities here.
//!
//! General / Terminal / Connection / AI. Each row controls a real capability.

use crate::app::Adash;
use crate::widgets;
use adash_core::state::Settings;
use gpui::prelude::*;
use gpui::*;
use gpui_kit_material::prelude::*;

const CATEGORIES: [&str; 4] = ["通用", "终端", "连接", "AI"];

impl Adash {
    /// Settings page.
    pub fn render_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let category = self.settings_category.min(CATEGORIES.len() - 1);
        let mut category_rail = div().flex().flex_col().gap(px(2.)).w(px(180.));
        for (index, name) in CATEGORIES.iter().enumerate() {
            let selected = index == category;
            category_rail = category_rail.child(
                div()
                    .id(("settings-category", index))
                    .flex()
                    .items_center()
                    .h(px(44.))
                    .px(rem(14.))
                    .rounded(rem(22.))
                    .cursor_pointer()
                    .when(selected, |this| {
                        this.bg(color(ColorRole::SECONDARY_CONTAINER))
                            .text_color(color(ColorRole::ON_SECONDARY_CONTAINER))
                    })
                    .when(!selected, |this| {
                        this.hover(|this| this.bg(color(ColorRole::SURFACE_CONTAINER_HIGH)))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.settings_category = index;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_size(TEXT_BODY)
                            .font_weight(FontWeight::MEDIUM)
                            .child(*name),
                    ),
            );
        }

        let body = match category {
            0 => self.settings_general(cx),
            1 => self.settings_terminal(window, cx),
            2 => self.settings_connection(cx),
            _ => self.settings_ai(cx),
        };

        div()
            .flex()
            .gap(rem(24.))
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(rem(12.))
                    .child(
                        div()
                            .text_size(rem(40.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("设置"),
                    )
                    .child(category_rail),
            )
            .child(
                div()
                    .id("settings-scroll")
                    .flex()
                    .flex_col()
                    .gap(rem(18.))
                    .flex_1()
                    .min_w_0()
                    .max_w(rem(720.))
                    .overflow_y_scroll()
                    .child(
                        div()
                            .text_size(rem(24.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(CATEGORIES[category]),
                    )
                    .child(body),
            )
            .into_any_element()
    }

    fn set_settings(&mut self, cx: &mut Context<Self>, update: impl FnOnce(&mut Settings)) {
        let previous = self.store.data.settings.clone();
        update(&mut self.store.data.settings);
        if let Err(error) = self.store.save() {
            self.store.data.settings = previous;
            self.notice = Some(format!("设置保存失败：{error}"));
        } else {
            let limit = self.store.data.settings.recent_limit;
            if self.profiles.data.recent.len() > limit
                && let Err(error) = self
                    .profiles
                    .update(|profiles| profiles.recent.truncate(limit))
            {
                self.notice = Some(format!("最近主机排序保存失败：{error:#}"));
            }
        }
        cx.notify();
    }

    fn settings_general(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.store.data.settings.clone();
        let mut appearance = div().flex().gap(rem(8.));
        for (value, name) in [("light", "浅色"), ("dark", "深色"), ("system", "跟随系统")] {
            let selected = settings.appearance == value;
            let target = value.to_owned();
            appearance = appearance.child(widgets::chip(
                format!("appearance-{value}"),
                name,
                selected,
                cx.listener(move |this, next: &bool, _, cx| {
                    if *next {
                        let value = target.clone();
                        this.set_settings(cx, |settings| settings.appearance = value);
                    }
                }),
            ));
        }
        CardSurface::new(CardVariant::Filled)
            .bg(color(ColorRole::SURFACE_CONTAINER_LOWEST))
            .rounded(rem(24.))
            .p(rem(20.))
            .gap(rem(16.))
            .child(widgets::section_header("语言"))
            .child(settings_row(
                "界面语言",
                "当前版本只提供简体中文",
                div().text_size(TEXT_BODY).child("简体中文"),
            ))
            .child(widgets::section_header("外观"))
            .child(settings_row(
                "浅色或深色",
                "切换语义色板；终端配色随之调整",
                appearance,
            ))
            .child(settings_row(
                "确认后关闭会话",
                "关闭正在运行的主机会话前先询问",
                widgets::tonal_button(
                    "toggle-confirm-close",
                    if settings.confirm_close {
                        "已开启"
                    } else {
                        "已关闭"
                    },
                    cx.listener(|this, _, _, cx| {
                        let value = !this.store.data.settings.confirm_close;
                        this.set_settings(cx, |settings| settings.confirm_close = value);
                    }),
                ),
            ))
            .child(settings_row(
                "连接后自动打开终端",
                "关闭时可从会话页手动连接终端",
                widgets::tonal_button(
                    "toggle-auto-shell",
                    if settings.auto_shell {
                        "已开启"
                    } else {
                        "已关闭"
                    },
                    cx.listener(|this, _, _, cx| {
                        let value = !this.store.data.settings.auto_shell;
                        this.set_settings(cx, |settings| settings.auto_shell = value);
                    }),
                ),
            ))
            .child(widgets::section_header("最近主机"))
            .child(settings_row(
                "保留数量",
                "只影响主机页的最近使用排序",
                {
                    let mut row = div().flex().gap(rem(6.));
                    for value in [5usize, 10, 20] {
                        row = row.child(widgets::chip(
                            format!("recent-{value}"),
                            value.to_string(),
                            settings.recent_limit == value,
                            cx.listener(move |this, next: &bool, _, cx| {
                                if *next {
                                    this.set_settings(cx, |settings| settings.recent_limit = value);
                                }
                            }),
                        ));
                    }
                    row
                },
            ))
            .into_any_element()
    }

    fn settings_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.store.data.settings.clone();
        let mut sizes = div().flex().gap(rem(6.));
        for size in [11u16, 12, 13, 15, 17] {
            let selected = settings.terminal_font_size == size;
            sizes = sizes.child(widgets::chip(
                format!("font-size-{size}"),
                format!("{size} pt"),
                selected,
                cx.listener(move |this, next: &bool, _, cx| {
                    if *next {
                        this.set_settings(cx, |settings| settings.terminal_font_size = size);
                    }
                }),
            ));
        }
        let installed_fonts = window.text_system().all_font_names();
        let mut fonts = div().flex().gap(rem(6.)).flex_wrap();
        for font in [
            "",
            "JetBrains Mono",
            "Fira Code",
            "Noto Sans Mono",
            "Source Code Pro",
        ] {
            if !font.is_empty()
                && !installed_fonts
                    .iter()
                    .any(|installed| installed.eq_ignore_ascii_case(font))
            {
                continue;
            }
            let name = if font.is_empty() {
                "等宽默认"
            } else {
                font
            };
            let selected = settings.terminal_font == font;
            let target = font.to_owned();
            fonts = fonts.child(widgets::chip(
                format!("font-{name}"),
                name,
                selected,
                cx.listener(move |this, next: &bool, _, cx| {
                    if *next {
                        let value = target.clone();
                        this.set_settings(cx, |settings| settings.terminal_font = value);
                    }
                }),
            ));
        }
        let preview = div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .p(rem(16.))
            .rounded(rem(16.))
            .bg(color(ColorRole::INVERSE_SURFACE))
            .font_family(if settings.terminal_font.is_empty() {
                "monospace".to_owned()
            } else {
                settings.terminal_font.clone()
            })
            .text_size(rem(f32::from(settings.terminal_font_size)))
            .child(
                div()
                    .text_color(color(ColorRole::INVERSE_ON_SURFACE))
                    .child("root@web-01:~$ ls --color"),
            )
            .child(
                div()
                    .flex()
                    .gap(rem(12.))
                    .child(div().text_color(color(ColorRole::PRIMARY)).child("bin"))
                    .child(div().text_color(color(ColorRole::PRIMARY)).child("etc"))
                    .child(
                        div()
                            .text_color(color(ColorRole::SUCCESS))
                            .child("deploy.sh"),
                    ),
            )
            .child(
                div()
                    .text_color(color(ColorRole::INVERSE_ON_SURFACE))
                    .child("root@web-01:~$ _"),
            );
        CardSurface::new(CardVariant::Filled)
            .bg(color(ColorRole::SURFACE_CONTAINER_LOWEST))
            .rounded(rem(24.))
            .p(rem(20.))
            .gap(rem(16.))
            .child(widgets::section_header("预览"))
            .child(preview)
            .child(widgets::section_header("文本"))
            .child(settings_row("字号", "终端与文件编辑器的等宽字号", sizes))
            .child(settings_row("字体", "使用系统已安装的等宽字体", fonts))
            .child(widgets::section_header("快捷键"))
            .child(shortcut("复制终端内容", "Ctrl+Shift+C"))
            .child(shortcut("粘贴终端内容", "Ctrl+V / Ctrl+Shift+V"))
            .child(shortcut("关闭当前会话", "会话页「关闭会话」"))
            .into_any_element()
    }

    fn settings_connection(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.store.data.settings.clone();
        let known = self.profiles.data.known_hosts.len();
        CardSurface::new(CardVariant::Filled)
            .bg(color(ColorRole::SURFACE_CONTAINER_LOWEST))
            .rounded(rem(24.))
            .p(rem(20.))
            .gap(rem(16.))
            .child(widgets::section_header("SSH"))
            .child(settings_row(
                "连接超时",
                "TCP 与 SSH 握手的最长等待时间",
                {
                    let mut row = div().flex().gap(rem(6.));
                    for value in [5u64, 10, 20, 30] {
                        row = row.child(widgets::chip(
                            format!("timeout-{value}"),
                            format!("{value} s"),
                            settings.connect_timeout == value,
                            cx.listener(move |this, next: &bool, _, cx| {
                                if *next {
                                    this.set_settings(cx, |settings| {
                                        settings.connect_timeout = value
                                    });
                                }
                            }),
                        ));
                    }
                    row
                },
            ))
            .child(settings_row(
                "已保存的主机指纹",
                "首次连接需要核对，变更后拒绝连接",
                div().text_size(TEXT_BODY).child(format!("{known} 个")),
            ))
            .child(div().flex().justify_end().child(widgets::text_button(
                "forget-host-keys",
                "清除全部指纹",
                cx.listener(|this, _, _, cx| {
                    this.pending_forget_keys = true;
                    cx.notify();
                }),
            )))
            .child(widgets::section_header("监控"))
            .child(settings_row(
                "刷新间隔",
                "读取远程 /proc 与磁盘统计的频率",
                {
                    let mut row = div().flex().gap(rem(6.));
                    for value in [5u64, 10, 30, 60] {
                        row = row.child(widgets::chip(
                            format!("monitor-{value}"),
                            format!("{value} s"),
                            settings.monitor_interval == value,
                            cx.listener(move |this, next: &bool, _, cx| {
                                if *next {
                                    this.set_settings(cx, |settings| {
                                        settings.monitor_interval = value
                                    });
                                }
                            }),
                        ));
                    }
                    row
                },
            ))
            .child(widgets::section_header("存储"))
            .child(settings_row(
                "主机配置",
                "密码与口令只存系统钥匙串",
                div()
                    .text_size(TEXT_AUX)
                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                    .child(self.profiles.path().display().to_string()),
            ))
            .into_any_element()
    }

    fn settings_ai(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut protocols = div().flex().flex_wrap().gap(rem(8.));
        for (id, label) in [
            ("openai-completions", "Chat Completions"),
            ("openai-responses", "Responses"),
            ("anthropic-messages", "Anthropic Messages"),
        ] {
            protocols = protocols.child(widgets::chip(
                format!("ai-api-{id}"),
                label,
                self.ai_form.api == id,
                cx.listener(move |this, selected: &bool, _, cx| {
                    if *selected && !this.ai_saving {
                        this.ai_form.api = id.into();
                        cx.notify();
                    }
                }),
            ));
        }
        let status = self
            .agent
            .offline_reason()
            .map(ToOwned::to_owned)
            .or_else(|| self.agent.status.clone())
            .unwrap_or_else(|| {
                self.agent
                    .model
                    .as_ref()
                    .map_or("正在连接 AI".into(), |model| {
                        format!("已配置 · {model}")
                    })
            });
        CardSurface::new(CardVariant::Filled)
            .bg(color(ColorRole::SURFACE_CONTAINER_LOWEST))
            .rounded(rem(24.))
            .p(rem(20.))
            .gap(rem(16.))
            .child(widgets::section_header("模型服务"))
            .child(
                div().text_size(TEXT_AUX).child(
                    "填写服务的 Base URL、API key 和模型 ID。保存后重连，并开启新的 AI 会话。",
                ),
            )
            .child(protocols)
            .child(widgets::labelled_input(
                "ai-endpoint",
                "Endpoint（Base URL）",
                &self.ai_form.endpoint,
            ))
            .child(widgets::labelled_input(
                "ai-model",
                "模型 ID",
                &self.ai_form.model,
            ))
            .child(widgets::labelled_input(
                "ai-key",
                "API key",
                &self.ai_form.key,
            ))
            .child(
                div()
                    .text_size(TEXT_AUX)
                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                    .child(if self.ai_settings.key_id.is_some() {
                        "密钥已存入系统钥匙串。留空保留当前密钥；更换 Endpoint 需重新填写。"
                    } else {
                        "API key 仅保存在系统钥匙串，不写入配置文件或对话。"
                    }),
            )
            .child(
                div()
                    .flex()
                    .gap(rem(12.))
                    .child(widgets::labelled_input(
                        "ai-context",
                        "上下文长度（tokens）",
                        &self.ai_form.context_window,
                    ))
                    .child(widgets::labelled_input(
                        "ai-output",
                        "最大输出（tokens）",
                        &self.ai_form.max_tokens,
                    )),
            )
            .children(
                self.ai_error
                    .clone()
                    .map(|error| widgets::notice(error, true)),
            )
            .child(div().text_size(TEXT_AUX).child(status))
            .child(
                div()
                    .flex()
                    .gap(rem(8.))
                    .child(
                        MaterialButton::new("ai-save")
                            .variant(ButtonVariant::Filled)
                            .disabled(self.ai_saving || self.agent.streaming)
                            .on_click(cx.listener(|this, _, window, cx| this.save_ai(window, cx)))
                            .child(if self.ai_saving {
                                "正在保存…"
                            } else {
                                "保存并连接"
                            }),
                    )
                    .child(
                        MaterialButton::new("ai-remove")
                            .variant(ButtonVariant::Text)
                            .disabled(self.ai_saving || self.agent.streaming)
                            .on_click(cx.listener(|this, _, window, cx| this.remove_ai(window, cx)))
                            .child("移除配置与密钥"),
                    ),
            )
            .into_any_element()
    }

    fn save_ai(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ai_saving || self.agent.streaming {
            return;
        }
        let mut settings = crate::ai::AiSettings {
            endpoint: self.ai_form.endpoint.read(cx).value().to_string(),
            model: self.ai_form.model.read(cx).value().to_string(),
            api: self.ai_form.api.clone(),
            key_id: None,
            context_window: self
                .ai_form
                .context_window
                .read(cx)
                .value()
                .trim()
                .parse()
                .unwrap_or(0),
            max_tokens: self
                .ai_form
                .max_tokens
                .read(cx)
                .value()
                .trim()
                .parse()
                .unwrap_or(0),
        };
        if let Err(error) = settings.validate() {
            self.ai_error = Some(error);
            cx.notify();
            return;
        }
        let key = zeroize::Zeroizing::new(self.ai_form.key.read(cx).value().to_string());
        self.ai_saving = true;
        self.ai_error = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move { settings.persist(&key) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.ai_saving = false;
                match result {
                    Ok((settings, warning)) => {
                        this.ai_settings = settings;
                        this.ai_form = crate::ai::AiForm::new(&this.ai_settings, window, cx);
                        this.ai_error = warning;
                        this.agent = crate::agent::Agent::connect(this.mode);
                        this.notice = Some("AI 配置已保存，正在连接新会话".into());
                    }
                    Err(error) => this.ai_error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn remove_ai(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ai_saving || self.agent.streaming {
            return;
        }
        self.ai_saving = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async { crate::ai::AiSettings::remove() })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.ai_saving = false;
                this.ai_settings = crate::ai::AiSettings::load().unwrap_or_default();
                this.ai_form = crate::ai::AiForm::new(&this.ai_settings, window, cx);
                this.ai_error = result.err();
                this.agent = crate::agent::Agent::connect(this.mode);
                cx.notify();
            });
        })
        .detach();
    }
}

fn settings_row(
    title: &'static str,
    description: &'static str,
    trailing: impl IntoElement,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(rem(8.))
        .child(
            div()
                .text_size(TEXT_BODY)
                .font_weight(FontWeight::MEDIUM)
                .child(title),
        )
        .child(
            div()
                .text_size(TEXT_AUX)
                .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                .child(description),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(rem(6.))
                .child(trailing.into_any_element()),
        )
        .into_any_element()
}

fn shortcut(title: &'static str, value: impl Into<SharedString>) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(rem(12.))
        .child(
            div()
                .w(rem(120.))
                .text_size(TEXT_AUX)
                .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                .child(title),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(TEXT_AUX)
                .child(value.into()),
        )
        .into_any_element()
}
