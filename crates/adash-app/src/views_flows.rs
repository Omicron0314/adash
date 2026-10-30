//! AI flows: a project's services drawn as an n8n-style graph, with replay.
//!
//! Layout is computed here (layered DAG); the stored data is topology only, so
//! the same flow renders sensibly at any window size. Edges are drawn as thin
//! orthogonal segments rather than a paint path: it keeps the canvas testable
//! with divs and avoids a second rendering path.

use crate::app::{Adash, Page};
use crate::i18n::{self, action};
use crate::widgets;
use adash_core::flow::{Flow, FlowEdge, FlowNode};
use adash_core::resource::money;
use gpui::prelude::*;
use gpui::*;
use gpui_kit_material::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

/// Geometry for one layout density.
#[derive(Debug, Clone, Copy)]
struct Metrics {
    node_w: f32,
    node_h: f32,
    col_gap: f32,
    row_gap: f32,
    pad: f32,
}

impl Metrics {
    fn for_compact(compact: bool) -> Self {
        if compact {
            Self {
                node_w: 168.,
                node_h: 70.,
                col_gap: 48.,
                row_gap: 14.,
                pad: 16.,
            }
        } else {
            Self {
                node_w: 196.,
                node_h: 76.,
                col_gap: 64.,
                row_gap: 16.,
                pad: 24.,
            }
        }
    }
}

impl Adash {
    /// Flow page: project switcher, canvas and replay timeline.
    pub fn render_flows(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let projects = self.inventory.flow_projects();
        let Some(flow) = self.current_flow() else {
            return div()
                .flex()
                .flex_col()
                .gap(rem(10.))
                .child(
                    div()
                        .text_size(rem(32.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(i18n::nav::FLOWS),
                )
                .child(widgets::empty_state(
                    i18n::empty::NO_FLOW,
                    "先让代理部署一个项目，这里就会出现它的服务图。",
                ))
                .into_any_element();
        };

        let mut chips = Vec::new();
        for slug in &projects {
            let selected = self.flow_project.as_deref() == Some(slug.as_str());
            let target = slug.clone();
            chips.push(widgets::chip(
                SharedString::from(format!("flow-{slug}")),
                slug.clone(),
                selected,
                cx.listener(move |this, next: &bool, _, cx| {
                    if *next {
                        this.flow_project = Some(target.clone());
                        this.replay = crate::app::Replay::default();
                        this.flow_selected = None;
                    }
                    cx.notify();
                }),
            ));
        }

        let canvas = self.render_canvas(&flow, window, cx);
        let timeline = self.render_timeline(&flow, cx);

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
                            .text_size(rem(32.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("{} · 服务图", flow.name)),
                    )
                    .child(
                        div()
                            .text_size(TEXT_AUX)
                            .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                            .child(flow.summary()),
                    )
                    .child(div().flex_1())
                    .child(widgets::text_button(
                        "flow-fit",
                        action::FIT,
                        cx.listener(|this, _, _, cx| {
                            this.flow_compact = !this.flow_compact;
                            cx.notify();
                        }),
                    )),
            )
            .child(widgets::chip_row(chips))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .gap(rem(16.))
                    .child(canvas)
                    .child(self.render_flow_inspector(&flow, cx)),
            )
            .child(timeline)
            .into_any_element()
    }

    fn render_canvas(&self, flow: &Flow, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let metrics = Metrics::for_compact(self.flow_compact);
        let positions = layout(flow, metrics);

        let width = positions
            .values()
            .map(|(x, _)| *x + metrics.node_w)
            .fold(0.0_f32, f32::max)
            + metrics.pad;
        let width = width.max(f32::from(window.bounds().size.width) - 420.);
        let height = positions
            .values()
            .map(|(_, y)| *y + metrics.node_h)
            .fold(0.0_f32, f32::max)
            + metrics.pad;

        let highlighted = self.highlighted_nodes(flow);

        let mut canvas = div()
            .relative()
            .w(px(width))
            .h(px(height))
            .rounded(rem(16.))
            .bg(widgets::canvas_background())
            .border_1()
            .border_color(color(ColorRole::SURFACE_CONTAINER_HIGH));

        // The reference editor uses a quiet dotted workspace behind its nodes.
        for col in 0..(width / 24.) as usize {
            for row in 0..(height / 24.) as usize {
                canvas = canvas.child(
                    div()
                        .absolute()
                        .left(px(col as f32 * 24. + 12.))
                        .top(px(row as f32 * 24. + 12.))
                        .size(px(2.))
                        .rounded_full()
                        .bg(color(ColorRole::OUTLINE_VARIANT)),
                );
            }
        }

        for (index, edge) in flow.edges.iter().enumerate() {
            let Some(from) = positions.get(&edge.from) else {
                continue;
            };
            let Some(to) = positions.get(&edge.to) else {
                continue;
            };
            for (segment, (x, y, w, h)) in edge_segments(from, to, metrics).into_iter().enumerate()
            {
                let lit = highlighted.contains(&edge.from) && highlighted.contains(&edge.to);
                if !lit && edge.evidence == adash_core::Evidence::Inferred {
                    let horizontal = w > h;
                    let length = if horizontal { w } else { h };
                    for dash in 0..(length / 10.).ceil() as usize {
                        let offset = dash as f32 * 10.;
                        let span = (length - offset).min(5.);
                        canvas = canvas.child(
                            div()
                                .absolute()
                                .left(px(x + if horizontal { offset } else { 0. }))
                                .top(px(y + if horizontal { 0. } else { offset }))
                                .w(px(if horizontal { span } else { w }))
                                .h(px(if horizontal { h } else { span }))
                                .bg(color(ColorRole::OUTLINE)),
                        );
                    }
                    continue;
                }
                canvas = canvas.child(
                    div()
                        .id(SharedString::from(format!("edge-{index}-{segment}")))
                        .absolute()
                        .left(px(x))
                        .top(px(y))
                        .w(px(w))
                        .h(px(h))
                        .rounded(px(1.))
                        .when(lit, |this| this.bg(color(ColorRole::PRIMARY)))
                        .when(!lit, |this| {
                            this.bg(color(if edge.evidence == adash_core::Evidence::Inferred {
                                ColorRole::OUTLINE
                            } else {
                                ColorRole::OUTLINE_VARIANT
                            }))
                        }),
                );
            }
        }

        for node in &flow.nodes {
            let Some((x, y)) = positions.get(&node.id).copied() else {
                continue;
            };
            canvas = canvas.child(self.render_node(node, x, y, metrics, &highlighted, cx));
        }

        div()
            .id("flow-canvas-scroll")
            .flex_1()
            .min_h_0()
            .overflow_scroll()
            .p(rem(4.))
            .child(canvas)
            .into_any_element()
    }

    fn render_node(
        &self,
        node: &FlowNode,
        x: f32,
        y: f32,
        metrics: Metrics,
        highlighted: &BTreeSet<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let lit = highlighted.contains(&node.id);
        let selected = self.flow_selected.as_deref() == Some(node.id.as_str());
        let resource = node.resource.clone();
        let node_id = node.id.clone();

        let mut card = div()
            .id(SharedString::from(format!("node-{}", node.id)))
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(metrics.node_w))
            .h(px(metrics.node_h))
            .flex()
            .flex_col()
            .justify_center()
            .gap(px(2.))
            .px(rem(12.))
            .rounded(rem(14.))
            .bg(color(if lit || selected {
                ColorRole::SECONDARY_CONTAINER
            } else {
                ColorRole::SURFACE_CONTAINER_LOWEST
            }))
            .border_1()
            .border_color(color(if selected {
                ColorRole::PRIMARY
            } else {
                ColorRole::OUTLINE_VARIANT
            }))
            .cursor_pointer();

        for _ in resource.iter() {
            card = card.child(
                div()
                    .absolute()
                    .left_0()
                    .top(rem(14.))
                    .bottom(rem(14.))
                    .w(px(3.))
                    .rounded_full()
                    .bg(color(ColorRole::PRIMARY)),
            );
        }

        card = card
            .on_click(cx.listener(move |this, _, _, cx| {
                this.flow_selected = Some(node_id.clone());
                if let Some(resource) = &resource {
                    this.selected = Some(resource.clone());
                }
                cx.notify();
            }))
            .child(
                div()
                    .text_size(TEXT_AUX)
                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                    .child(node.kind_label().to_owned()),
            )
            .child(
                div()
                    .text_size(TEXT_BODY)
                    .font_weight(FontWeight::MEDIUM)
                    .child(node.label.clone()),
            )
            .child(
                div()
                    .text_size(rem(10.))
                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                    .child(node.detail.clone()),
            );

        card.into_any_element()
    }

    fn render_flow_inspector(&self, flow: &Flow, cx: &mut Context<Self>) -> AnyElement {
        let selected = self
            .flow_selected
            .as_ref()
            .and_then(|id| flow.nodes.iter().find(|node| &node.id == id));
        let mut panel = CardSurface::new(CardVariant::Filled)
            .bg(color(ColorRole::SURFACE_CONTAINER_LOWEST))
            .w(rem(224.))
            .flex_shrink_0()
            .rounded(rem(24.))
            .p(rem(20.))
            .gap(rem(16.))
            .child(
                div()
                    .text_size(rem(20.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(if selected.is_some() {
                        i18n::flow::NODE_DETAILS
                    } else {
                        i18n::flow::PROJECT_OVERVIEW
                    }),
            );
        if let Some(node) = selected {
            panel = panel
                .child(
                    div()
                        .text_size(rem(16.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(node.label.clone()),
                )
                .child(
                    div()
                        .text_size(TEXT_AUX)
                        .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                        .child(node.detail.clone()),
                )
                .child(widgets::state_pill(node.state));
            if let Some(resource) = node.resource.as_ref() {
                let target = resource.clone();
                panel = panel.child(widgets::tonal_button(
                    "inspect-flow-resource",
                    i18n::flow::VIEW_RESOURCE,
                    cx.listener(move |this, _, _, cx| {
                        this.selected = Some(target.clone());
                        this.page = Page::Resources;
                        cx.notify();
                    }),
                ));
            }
        } else {
            panel = panel
                .child(div().text_size(rem(16.)).child(flow.name.clone()))
                .child(widgets::kv_row(
                    i18n::flow::NODES,
                    flow.nodes.len().to_string(),
                ))
                .child(widgets::kv_row(
                    i18n::flow::CONNECTIONS,
                    flow.edges.len().to_string(),
                ))
                .child(widgets::kv_row(
                    i18n::flow::RUNS,
                    flow.runs.len().to_string(),
                ))
                .child(
                    div()
                        .text_size(TEXT_AUX)
                        .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                        .child(i18n::flow::SELECT_HINT),
                );
        }
        panel
            .child(
                div()
                    .text_size(TEXT_AUX)
                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                    .child(i18n::flow::LEGEND),
            )
            .into_any_element()
    }

    fn render_timeline(&self, flow: &Flow, cx: &mut Context<Self>) -> AnyElement {
        let run = self.replay_run();
        let mut row = div()
            .flex()
            .items_center()
            .gap(rem(10.))
            .p(rem(12.))
            .rounded(rem(14.))
            .bg(color(ColorRole::SURFACE_CONTAINER_LOW));

        match run {
            Some(run) => {
                let total = run.steps.len();
                let step = self.replay.step.min(total);
                let label = run
                    .steps
                    .get(step.saturating_sub(1))
                    .map_or_else(|| "开始回放".to_owned(), |step| step.label.clone());
                let run_id = run.id.clone();
                row = row
                    .child(
                        div()
                            .text_size(TEXT_AUX)
                            .font_weight(FontWeight::MEDIUM)
                            .child(format!("{} · {}", run.label, format_time(run.started_at))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(TEXT_AUX)
                            .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                            .child(format!("{step}/{total} · {label}")),
                    )
                    .child(widgets::text_button(
                        "replay-stop",
                        action::STOP,
                        cx.listener(|this, _, _, cx| {
                            this.replay.playing = false;
                            cx.notify();
                        }),
                    ))
                    .child(widgets::filled_button(
                        "replay-start",
                        action::REPLAY,
                        cx.listener(move |this, _, _, cx| {
                            this.start_replay(run_id.clone(), cx);
                        }),
                    ));
            }
            None if flow.runs.is_empty() => {
                row = row.child(
                    div()
                        .flex_1()
                        .text_size(TEXT_AUX)
                        .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                        .child("还没有执行记录；代理部署后这里可以回放。"),
                );
            }
            None => {
                let latest = flow.runs.last().cloned();
                row = row
                    .child(
                        div()
                            .flex_1()
                            .text_size(TEXT_AUX)
                            .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                            .child(format!(
                                "{} 条执行记录 · 最近一次 {}",
                                flow.runs.len(),
                                latest.as_ref().map_or_else(
                                    || "—".to_owned(),
                                    |run| format_time(run.started_at)
                                )
                            )),
                    )
                    .children(latest.map(|run| {
                        let run_id = run.id.clone();
                        widgets::tonal_button(
                            "replay-latest",
                            action::REPLAY,
                            cx.listener(move |this, _, _, cx| {
                                this.start_replay(run_id.clone(), cx);
                            }),
                        )
                    }));
            }
        }

        row.into_any_element()
    }

    fn highlighted_nodes(&self, _flow: &Flow) -> BTreeSet<String> {
        let mut set = BTreeSet::new();
        if let Some(run) = self.replay_run() {
            for step in run.steps.iter().take(self.replay.step) {
                set.insert(step.node.clone());
            }
        }
        // Without playback the canvas stays neutral: "all nodes lit" would read
        // as a selection rather than a history.
        set
    }

    /// Jump from the flow page to the resource behind a node.
    pub fn open_flow_node(&mut self, node_id: &str, cx: &mut Context<Self>) {
        let Some(flow) = self.current_flow() else {
            return;
        };
        let Some(node) = flow.node(node_id) else {
            return;
        };
        if let Some(resource) = &node.resource {
            self.selected = Some(resource.clone());
            self.page = Page::Resources;
        }
        cx.notify();
    }
}

fn layout(flow: &Flow, metrics: Metrics) -> BTreeMap<String, (f32, f32)> {
    let layers = flow.layers();
    let tallest = layers.iter().map(Vec::len).max().unwrap_or(1) as f32;
    let column_height = tallest * metrics.node_h + (tallest - 1.).max(0.) * metrics.row_gap;

    let mut positions = BTreeMap::new();
    for (column, nodes) in layers.iter().enumerate() {
        let height = nodes.len() as f32 * metrics.node_h
            + (nodes.len() as f32 - 1.).max(0.) * metrics.row_gap;
        let offset = metrics.pad + (column_height - height) / 2.;
        for (row, node) in nodes.iter().enumerate() {
            let x = metrics.pad + column as f32 * (metrics.node_w + metrics.col_gap);
            let y = offset + row as f32 * (metrics.node_h + metrics.row_gap);
            positions.insert(node.clone(), (x, y));
        }
    }
    positions
}

/// Orthogonal segments from a source node's right edge to a target's left edge.
fn edge_segments(
    from: &(f32, f32),
    to: &(f32, f32),
    metrics: Metrics,
) -> [(f32, f32, f32, f32); 3] {
    let x1 = from.0 + metrics.node_w;
    let y1 = from.1 + metrics.node_h / 2.;
    let x2 = to.0;
    let y2 = to.1 + metrics.node_h / 2.;
    let mid = (x1 + metrics.col_gap / 2.).min(x2 - 4.);
    let top = y1.min(y2);
    let height = (y1 - y2).abs().max(2.);
    [
        (x1, y1 - 1., (mid - x1).max(2.), 2.),
        (mid - 1., top, 2., height),
        (mid, y2 - 1., (x2 - mid).max(2.), 2.),
    ]
}

fn format_time(unix: i64) -> String {
    let seconds = adash_core::state::now_secs() - unix;
    match seconds {
        i64::MIN..=0 => "刚刚".to_owned(),
        1..=59 => format!("{seconds} 秒前"),
        60..=3599 => format!("{} 分钟前", seconds / 60),
        3600..=86399 => format!("{} 小时前", seconds / 3600),
        _ => format!("{} 天前", seconds / 86400),
    }
}

/// Exposed for the resource detail's cost line, keeping the format in one place.
pub fn cost(cents: i64) -> String {
    money(cents)
}

/// Unused placeholder to keep the module's public surface stable.
pub type Edge = FlowEdge;

// `gpui` exports a `test` macro through its glob prelude, so the built-in
// attribute is spelled out here.
#[cfg(test)]
mod tests {
    use super::*;
    use adash_core::Inventory;
    use ucloud_api::UcloudClient;

    fn flow() -> Flow {
        let inventory = Inventory::load(&UcloudClient::mock());
        let record = adash_core::ProjectRecord {
            slug: "shop-api".into(),
            name: "Shop API".into(),
            repo: Some("github.com/me/shop-api".into()),
            note: String::new(),
            budget_cents: 0,
        };
        Flow::build(&inventory, "shop-api", Some(&record), Vec::new())
    }

    #[::core::prelude::v1::test]
    fn layout_places_every_node_once_and_starts_with_the_repository() {
        let flow = flow();
        let metrics = Metrics::for_compact(false);
        let positions = layout(&flow, metrics);

        assert_eq!(positions.len(), flow.nodes.len());
        let (x, _) = positions["ext:repo"];
        assert!((x - metrics.pad).abs() < f32::EPSILON);
        assert!(positions.values().all(|(x, y)| *x >= 0.0 && *y >= 0.0));
    }

    #[::core::prelude::v1::test]
    fn columns_never_overlap() {
        let flow = flow();
        let metrics = Metrics::for_compact(false);
        let positions = layout(&flow, metrics);
        let layers = flow.layers();

        for (column, nodes) in layers.iter().enumerate() {
            let expected = metrics.pad + column as f32 * (metrics.node_w + metrics.col_gap);
            for node in nodes {
                assert!((positions[node].0 - expected).abs() < f32::EPSILON);
            }
        }
    }

    #[::core::prelude::v1::test]
    fn edges_are_three_orthogonal_segments() {
        let metrics = Metrics::for_compact(false);
        let segments = edge_segments(&(0., 0.), &(300., 120.), metrics);
        assert_eq!(segments.len(), 3);
        // horizontal, vertical, horizontal
        assert!(segments[0].3 <= 2.5);
        assert!(segments[1].2 <= 2.5);
        assert!(segments[2].3 <= 2.5);
        // and they end where the target starts
        let last = segments[2];
        assert!((last.0 + last.2 - 300.).abs() < 0.5);
    }

    #[::core::prelude::v1::test]
    fn replay_time_formatting_is_relative() {
        let now = adash_core::state::now_secs();
        assert_eq!(format_time(now), "刚刚");
        assert_eq!(format_time(now - 120), "2 分钟前");
        assert_eq!(format_time(now - 7200), "2 小时前");
    }
}
