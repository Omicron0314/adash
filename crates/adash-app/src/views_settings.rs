//! Settings: the catussh sections that map to real capabilities here.
//!
//! General / Terminal / Connection / About, each row backed by a persisted
//! setting or a live fact. Rows without a corresponding capability are omitted
//! rather than shown as dead switches.

use crate::app::Adash;
use crate::widgets;
use adash_core::state::Settings;
use gpui::prelude::*;
use gpui::*;
use gpui_kit_material::prelude::*;

const CATEGORIES: [&str; 4] = ["通用", "终端", "连接", "关于"];

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
            _ => self.settings_about(cx),
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

    fn settings_about(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let credentials = std::env::var("UCLOUD_PUBLIC_KEY").ok();
        let agent = match &self.agent.offline_reason() {
            Some(reason) => format!("本地应答 · {reason}"),
            None => "pi SDK 已嵌入".to_owned(),
        };
        let model = self
            .agent
            .model
            .clone()
            .unwrap_or_else(|| "启动中".to_owned());
        let tools = if self.agent.tools.is_empty() {
            "—".to_owned()
        } else {
            self.agent.tools.join(" · ")
        };
        div()
            .flex()
            .flex_col()
            .gap(rem(18.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(rem(16.))
                    .child(
                        div()
                            .size(rem(72.))
                            .rounded(rem(24.))
                            .bg(color(ColorRole::PRIMARY))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(color(ColorRole::ON_PRIMARY))
                            .text_size(rem(28.))
                            .font_weight(FontWeight::BOLD)
                            .child("ad"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(rem(24.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("adash"),
                            )
                            .child(
                                div()
                                    .text_size(TEXT_AUX)
                                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                                    .child("UCloud 对话控制台"),
                            ),
                    ),
            )
            .child(
                CardSurface::new(CardVariant::Filled)
                    .bg(color(ColorRole::SURFACE_CONTAINER_LOWEST))
                    .rounded(rem(24.))
                    .p(rem(20.))
                    .gap(rem(12.))
                    .child(widgets::section_header("版本"))
                    .child(shortcut("版本", env!("CARGO_PKG_VERSION")))
                    .child(shortcut("组件", "GPUI · gpui-kit-material · pi SDK"))
                    .child(shortcut("界面参考", "docs/design/neko.html · catussh"))
                    .child(widgets::section_header("云账户"))
                    .child(shortcut(
                        "模式",
                        if self.mode == ucloud_api::Mode::Mock {
                            "Mock（不花钱）"
                        } else {
                            "Live"
                        },
                    ))
                    .child(shortcut(
                        "PublicKey",
                        credentials
                            .as_deref()
                            .map_or_else(|| "未设置".to_owned(), ucloud_api::redact),
                    ))
                    .child(shortcut(
                        "本地状态",
                        self.store.path().display().to_string(),
                    ))
                    .child(widgets::section_header("代理"))
                    .child(shortcut("运行方式", agent))
                    .child(shortcut("模型", model))
                    .child(shortcut("工具", tools))
                    .child(shortcut("变更", "只能在计划卡片确认后由 adash-tool 执行")),
            )
            .child(
                div()
                    .flex()
                    .gap(rem(8.))
                    .child(widgets::text_button(
                        "open-repo",
                        "GitHub",
                        cx.listener(|this, _, _, cx| {
                            this.notice = Some("https://github.com/Omicron0314/adash".to_owned());
                            cx.notify();
                        }),
                    ))
                    .child(widgets::text_button(
                        "open-license",
                        "MIT 许可",
                        cx.listener(|this, _, _, cx| {
                            this.notice = Some("许可证：MIT；详见仓库 LICENSE".to_owned());
                            cx.notify();
                        }),
                    )),
            )
            .into_any_element()
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
