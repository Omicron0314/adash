//! Cloud resources: everything already created in this account.

use crate::app::{Adash, Filters, Page};
use crate::i18n::{self, action};
use crate::widgets;
use adash_core::resource::{Resource, ResourceKind, money};
use gpui::prelude::*;
use gpui::*;
use gpui_kit_material::prelude::*;

impl Adash {
    /// Filtered resource list with a detail panel.
    pub fn render_resources(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let query = self.search.read(cx).value().to_lowercase();
        let visible: Vec<Resource> = self
            .inventory
            .resources
            .iter()
            .filter(|resource| self.matches(resource, &query))
            .cloned()
            .collect();

        let mut chips = Vec::new();
        chips.push(widgets::chip(
            "filter-all",
            "全部",
            self.filters.kinds.is_empty(),
            cx.listener(|this, next: &bool, _, cx| {
                if *next {
                    this.filters.kinds.clear();
                }
                cx.notify();
            }),
        ));
        for kind in ResourceKind::ALL {
            if self.inventory.of_kind(kind).count() == 0 {
                continue;
            }
            let selected = self.filters.kinds.contains(&kind);
            chips.push(widgets::chip(
                SharedString::from(format!("filter-{}", kind.as_str())),
                kind.label(),
                selected,
                cx.listener(move |this, next: &bool, _, cx| {
                    if *next {
                        this.filters.kinds.insert(kind);
                    } else {
                        this.filters.kinds.remove(&kind);
                    }
                    cx.notify();
                }),
            ));
        }

        let mut region_chips = Vec::new();
        for region in self.inventory.regions() {
            let selected = self.filters.region.as_deref() == Some(region.as_str());
            let target = region.clone();
            region_chips.push(widgets::chip(
                SharedString::from(format!("region-{region}")),
                region.clone(),
                selected,
                cx.listener(move |this, next: &bool, _, cx| {
                    this.filters.region = next.then(|| target.clone());
                    cx.notify();
                }),
            ));
        }
        for project in self.inventory.projects() {
            let selected = self.filters.project.as_deref() == Some(project.as_str());
            let target = project.clone();
            region_chips.push(widgets::chip(
                SharedString::from(format!("project-{project}")),
                format!("adash:{project}"),
                selected,
                cx.listener(move |this, next: &bool, _, cx| {
                    this.filters.project = next.then(|| target.clone());
                    cx.notify();
                }),
            ));
        }

        let mut list = div()
            .id("resource-scroll")
            .flex()
            .flex_row()
            .flex_wrap()
            .items_start()
            .gap(rem(16.))
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();

        if visible.is_empty() {
            list = list.child(widgets::empty_state(
                i18n::empty::NO_RESOURCES,
                "试试清除筛选，或者让代理帮你建一个。",
            ));
        }
        for resource in &visible {
            let selected = self.selected.as_ref() == Some(&resource.id);
            let target = resource.id.clone();
            list = list.child(
                div()
                    .id(SharedString::from(format!("row-{}", resource.id.key())))
                    .flex()
                    .flex_col()
                    .items_start()
                    .w(relative(0.48))
                    .gap(rem(16.))
                    .p(rem(20.))
                    .rounded(rem(24.))
                    .bg(color(ColorRole::SURFACE_CONTAINER_LOWEST))
                    .border_1()
                    .border_color(color(if selected {
                        ColorRole::PRIMARY
                    } else {
                        ColorRole::SURFACE_CONTAINER_LOWEST
                    }))
                    .cursor_pointer()
                    .when(selected, |this| {
                        this.bg(color(ColorRole::SECONDARY_CONTAINER))
                    })
                    .when(!selected, |this| {
                        this.hover(|this| this.bg(color(ColorRole::SURFACE_CONTAINER_HIGH)))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected = Some(target.clone());
                        cx.notify();
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(rem(10.))
                            .child(widgets::resource_icon(resource.id.kind))
                            .child(widgets::state_pill(resource.state)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .gap(px(1.))
                            .child(
                                div()
                                    .text_size(TEXT_BODY)
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(resource.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(TEXT_AUX)
                                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                                    .child(format!(
                                        "{} · {} · {}",
                                        resource.id.kind.label(),
                                        resource.region,
                                        resource.spec
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .text_size(TEXT_AUX)
                            .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                            .child(resource.project.clone().map_or_else(
                                || "未分组".to_owned(),
                                |slug| format!("adash:{slug}"),
                            )),
                    )
                    .child(
                        div()
                            .text_size(rem(18.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(money(resource.monthly_cost_cents) + " / 月"),
                    ),
            );
        }

        let mut column = div()
            .flex()
            .flex_col()
            .gap(rem(16.))
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(rem(12.))
                    .child(
                        div()
                            .text_size(rem(40.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(i18n::nav::RESOURCES),
                    )
                    .child(
                        div()
                            .text_size(TEXT_AUX)
                            .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                            .child(format!(
                                "{} / {} 个 · 月成本 {}",
                                visible.len(),
                                self.inventory.resources.len(),
                                money(self.inventory.monthly_total_cents())
                            )),
                    )
                    .child(div().flex_1())
                    .child(div().w(px(260.)).child(widgets::rounded_input(
                        "resource-search",
                        &self.search,
                        Some("搜索"),
                    ))),
            )
            .child(widgets::chip_row(chips))
            .child(widgets::chip_row(region_chips));

        let filtered = !self.filters.kinds.is_empty()
            || self.filters.region.is_some()
            || self.filters.project.is_some()
            || !query.is_empty();
        if filtered {
            column = column.child(div().flex().child(widgets::text_button(
                "clear-filters",
                action::CLEAR_FILTERS,
                cx.listener(|this, _, _, cx| {
                    this.filters = Filters::default();
                    cx.notify();
                }),
            )));
        }

        let detail = self
            .selected_resource()
            .map(|resource| self.render_detail(&resource, cx));

        column
            .child(
                div()
                    .flex()
                    .gap(rem(16.))
                    .flex_1()
                    .min_h_0()
                    .child(list)
                    .children(detail),
            )
            .into_any_element()
    }

    fn matches(&self, resource: &Resource, query: &str) -> bool {
        let kind_ok =
            self.filters.kinds.is_empty() || self.filters.kinds.contains(&resource.id.kind);
        let region_ok = self
            .filters
            .region
            .as_deref()
            .is_none_or(|region| resource.region == region);
        let project_ok = match self.filters.project.as_deref() {
            Some(project) => resource.project.as_deref() == Some(project),
            None => true,
        };
        let query_ok = query.is_empty()
            || resource.name.to_lowercase().contains(query)
            || resource.id.id.to_lowercase().contains(query);
        kind_ok && region_ok && project_ok && query_ok
    }

    fn selected_resource(&self) -> Option<Resource> {
        let id = self.selected.clone()?;
        self.inventory.get(&id).cloned()
    }

    fn render_detail(&self, resource: &Resource, cx: &mut Context<Self>) -> AnyElement {
        let mut rows = div().flex().flex_col();
        rows = rows
            .child(widgets::kv_row("ID", resource.id.id.clone()))
            .child(widgets::kv_row("状态", resource.state.label()))
            .child(widgets::kv_row("地域", resource.region.clone()))
            .child(widgets::kv_row("规格", resource.spec.clone()))
            .child(widgets::kv_row(
                "项目",
                resource.project_or_unassigned().to_owned(),
            ))
            .child(widgets::kv_row(
                "月成本",
                money(resource.monthly_cost_cents),
            ));
        if let Some(ip) = &resource.public_ip {
            rows = rows.child(widgets::kv_row("公网 IP", ip.clone()));
        }
        if let Some(ip) = &resource.private_ip {
            rows = rows.child(widgets::kv_row("内网 IP", ip.clone()));
        }
        // IPs are rendered above; the attribute map repeats them for the agent.
        for (key, value) in &resource.attributes {
            if matches!(key.as_str(), "公网 IP" | "内网 IP") {
                continue;
            }
            rows = rows.child(widgets::kv_row(key.clone(), value.clone()));
        }
        if !resource.attached_to.is_empty() {
            rows = rows.child(widgets::kv_row(
                "关联",
                resource
                    .attached_to
                    .iter()
                    .map(|id| id.id.clone())
                    .collect::<Vec<_>>()
                    .join(" · "),
            ));
        }

        let host = resource.id.kind == ResourceKind::UHost;
        let target = resource.clone();

        CardSurface::new(CardVariant::Filled)
            .id("resource-detail")
            .bg(color(ColorRole::SURFACE_CONTAINER_LOWEST))
            .rounded(rem(24.))
            .flex()
            .flex_col()
            .gap(rem(8.))
            .p(rem(16.))
            .w(px(340.))
            .flex_shrink_0()
            .self_start()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(rem(8.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(TEXT_BODY)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(resource.name.clone()),
                    )
                    .child(widgets::state_pill(resource.state)),
            )
            .child(rows)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(rem(6.))
                    .child(widgets::tonal_button(
                        "detail-flow",
                        "查看流程图",
                        cx.listener(move |this, _, _, cx| {
                            this.flow_project = target.project.clone();
                            this.page = Page::Flows;
                            this.replay = crate::app::Replay::default();
                            cx.notify();
                        }),
                    ))
                    .child(widgets::text_button(
                        "detail-copy",
                        action::COPY,
                        cx.listener({
                            let key = resource.id.key();
                            move |this, _, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(key.clone()));
                                this.notice = Some("资源 id 已复制".to_owned());
                                cx.notify();
                            }
                        }),
                    ))
                    .children(host.then(|| {
                        let resource = resource.clone();
                        widgets::filled_button(
                            "detail-terminal",
                            action::OPEN_TERMINAL,
                            cx.listener(move |this, _, window, cx| {
                                this.open_session(&resource, window, cx);
                            }),
                        )
                    })),
            )
            .into_any_element()
    }
}
