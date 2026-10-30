//! Application shell: navigation, routing and shared state.
//!
//! Pages: home (agent conversation + account overview), resources (every cloud
//! object the account owns) and flows (n8n-style service graphs per project).

use crate::i18n::Text;
use gpui::prelude::*;
use gpui::*;
use gpui_kit_material::prelude::*;
use gpui_kit_material::theme;

/// A top-level destination in the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// Agent conversation plus the account overview.
    Home,
    /// Cloud resources: everything already created in this account.
    Resources,
    /// AI flows: which services a project uses, drawn as a graph.
    Flows,
    /// Credentials, theme, language; pinned to the bottom of the sidebar.
    Settings,
}

impl Page {
    /// Sidebar order; `Settings` is rendered separately, below the spacer.
    pub const ALL: [Self; 4] = [Self::Home, Self::Resources, Self::Flows, Self::Settings];

    /// Product pages, i.e. everything except `Settings`.
    pub const PAGES: [Self; 3] = [Self::Home, Self::Resources, Self::Flows];

    /// Stable id for element ids and tests.
    pub fn id(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Resources => "resources",
            Self::Flows => "flows",
            Self::Settings => "settings",
        }
    }

    /// Sidebar label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Home => Text::NavHome.zh(),
            Self::Resources => Text::NavResources.zh(),
            Self::Flows => Text::NavFlows.zh(),
            Self::Settings => Text::NavSettings.zh(),
        }
    }

    /// Material symbol name.
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Home => "forum",
            Self::Resources => "dns",
            Self::Flows => "account_tree",
            Self::Settings => "settings",
        }
    }
}

/// Root view. Holds the current page until the real state lands in M2+.
pub struct Adash {
    /// Currently visible page.
    pub page: Page,
}

impl Adash {
    /// Build the shell on top of an already themed window.
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self { page: Page::Home }
    }
}

impl Render for Adash {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .p(rem(24.))
                            .child(page_placeholder(self.page)),
                    ),
            )
    }
}

impl Adash {
    fn render_titlebar(&self, _cx: &mut Context<Self>) -> AnyElement {
        let labels = CaptionLabels {
            minimize: "最小化".into(),
            maximize: "最大化".into(),
            restore: "还原".into(),
            close: "关闭".into(),
        };
        TitleBar::new("adash-titlebar", px(56.), labels)
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
                            .text_size(TEXT_TITLE)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(Text::AppName.zh()),
                    ),
            )
            .child(div().flex_1())
            .into_any_element()
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut rail = div()
            .flex()
            .flex_col()
            .w(px(220.))
            .h_full()
            .gap(rem(4.))
            .p(rem(12.))
            .border_r_1()
            .border_color(color(ColorRole::OUTLINE_VARIANT))
            .bg(color(ColorRole::SURFACE_CONTAINER_LOW));

        for page in Page::PAGES {
            rail = rail.child(self.nav_item(page, cx));
        }

        rail = rail.child(div().flex_1()).child(
            div()
                .pt(rem(8.))
                .border_t_1()
                .border_color(color(ColorRole::OUTLINE_VARIANT))
                .child(self.nav_item(Page::Settings, cx)),
        );

        rail.into_any_element()
    }

    fn nav_item(&self, page: Page, cx: &mut Context<Self>) -> AnyElement {
        let selected = page == self.page;
        div()
            .id(SharedString::from(page.id()))
            .flex()
            .items_center()
            .gap(rem(12.))
            .h(px(44.))
            .px(rem(14.))
            .rounded(rem(22.))
            .cursor_pointer()
            .when(selected, |this| {
                this.bg(color(ColorRole::SECONDARY_CONTAINER))
                    .text_color(color(ColorRole::ON_SECONDARY_CONTAINER))
            })
            .hover(|this| this.bg(color(ColorRole::SURFACE_CONTAINER_HIGH)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.page = page;
                cx.notify();
            }))
            .child(
                div()
                    .text_size(TEXT_BODY)
                    .font_weight(FontWeight::MEDIUM)
                    .child(page.label()),
            )
            .into_any_element()
    }
}

fn page_placeholder(page: Page) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(rem(8.))
        .child(
            div()
                .text_size(TEXT_HEADLINE)
                .font_weight(FontWeight::SEMIBOLD)
                .child(page.label()),
        )
        .child(
            div()
                .text_size(TEXT_BODY)
                .text_color(theme::color(ColorRole::ON_SURFACE_VARIANT))
                .child(format!("{} — 页面内容见 docs/ROADMAP.md", page.id())),
        )
        .into_any_element()
}
