//! Home: the agent conversation, with the account overview beside it.

use crate::agent::Role;
use crate::app::{Adash, Page};
use crate::i18n::{self, action, overview};
use crate::widgets;
use adash_core::resource::{ResourceKind, money};
use gpui::prelude::*;
use gpui::*;
use gpui_kit_material::prelude::*;

impl Adash {
    /// Conversation + overview.
    pub fn render_home(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .gap(rem(16.))
            .size_full()
            .child(self.render_chat(window, cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w(px(340.))
                    .flex_shrink_0()
                    .gap(rem(10.))
                    .child(self.render_overview(window, cx)),
            )
            .into_any_element()
    }

    fn render_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let mut messages = div()
            .id("chat-scroll")
            .flex()
            .flex_col()
            .gap(rem(12.))
            .flex_1()
            .min_h_0()
            .p(rem(4.))
            .overflow_y_scroll();

        for (index, message) in self.messages.iter().enumerate() {
            messages = messages.child(match message.role {
                Role::User => div()
                    .flex()
                    .justify_end()
                    .child(
                        div()
                            .max_w(rem(420.))
                            .px(rem(14.))
                            .py(rem(10.))
                            .rounded(rem(18.))
                            .bg(color(ColorRole::PRIMARY_CONTAINER))
                            .text_color(color(ColorRole::ON_PRIMARY_CONTAINER))
                            .text_size(TEXT_BODY)
                            .child(message.text.clone()),
                    )
                    .into_any_element(),
                Role::Agent => {
                    let mut body = div().flex().flex_col().gap(rem(6.)).max_w(rem(560.)).child(
                        div()
                            .text_size(TEXT_BODY)
                            .child(if message.text.is_empty() {
                                "…".to_owned()
                            } else {
                                message.text.clone()
                            }),
                    );
                    if message.offline {
                        body = body.child(
                            div()
                                .text_size(TEXT_AUX)
                                .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                                .child(i18n::OFFLINE_PREFIX),
                        );
                    }
                    if !message.notes.is_empty() {
                        let mut notes = div().flex().flex_col().gap(px(2.)).pl(rem(2.));
                        for note in &message.notes {
                            notes = notes.child(
                                div()
                                    .text_size(TEXT_AUX)
                                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                                    .child(note.clone()),
                            );
                        }
                        body = body.child(notes);
                    }
                    div()
                        .flex()
                        .flex_col()
                        .items_start()
                        .child(body)
                        .into_any_element()
                }
            });
            let _ = index;
        }

        if let Some(plan) = self.pending_plan().cloned() {
            messages = messages.child(self.render_plan_card(&plan, cx));
        }

        let column = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .gap(rem(10.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(rem(8.))
                    .child(
                        div()
                            .text_size(TEXT_TITLE)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(i18n::nav::HOME),
                    )
                    .child(div().flex_1())
                    .children(self.agent.status.as_ref().map(|status| {
                        div()
                            .text_size(TEXT_AUX)
                            .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                            .child(status.clone())
                    })),
            )
            .child(messages)
            .child(self.render_composer(window, cx));

        column.into_any_element()
    }

    fn render_composer(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(rem(8.))
            .p(rem(8.))
            .rounded(rem(20.))
            .bg(color(ColorRole::SURFACE_CONTAINER_HIGH))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(TextField::new("chat-input", &self.input)),
            )
            .child(
                MaterialButton::new("chat-send")
                    .variant(ButtonVariant::Filled)
                    .disabled(self.agent.streaming)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.send_input(window, cx);
                    }))
                    .child(action::SEND),
            )
            .into_any_element()
    }

    fn render_plan_card(&self, plan: &adash_core::Plan, cx: &mut Context<Self>) -> AnyElement {
        let destructive = !plan.reversible;
        let plan_id_cancel = plan.id.clone();
        let plan_id_confirm = plan.id.clone();
        let mut actions = div().flex().flex_col().gap(px(2.));
        for action in &plan.actions {
            actions = actions.child(
                div()
                    .flex()
                    .items_center()
                    .gap(rem(8.))
                    .child(
                        div()
                            .w(rem(52.))
                            .text_size(TEXT_AUX)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(if action.verb.reversible() {
                                color(ColorRole::ON_SURFACE_VARIANT)
                            } else {
                                color(ColorRole::ERROR)
                            })
                            .child(action.verb.label()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(TEXT_AUX)
                            .child(action.summary()),
                    )
                    .child(
                        div()
                            .text_size(TEXT_AUX)
                            .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                            .child(money(action.monthly_cents)),
                    ),
            );
        }

        let mut risks = div().flex().flex_col().gap(px(1.));
        for risk in &plan.risks {
            risks = risks.child(
                div()
                    .text_size(TEXT_AUX)
                    .text_color(color(if destructive {
                        ColorRole::ERROR
                    } else {
                        ColorRole::ON_SURFACE_VARIANT
                    }))
                    .child(format!("· {risk}")),
            );
        }

        MaterialCard::new("plan-card", CardVariant::Elevated)
            .flex()
            .flex_col()
            .gap(rem(8.))
            .p(rem(16.))
            .max_w(rem(560.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(rem(8.))
                    .child(
                        div()
                            .text_size(TEXT_BODY)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(plan.kind.label()),
                    )
                    .child(
                        div()
                            .text_size(TEXT_BODY)
                            .text_color(color(if plan.monthly_delta_cents > 0 {
                                ColorRole::ON_SURFACE
                            } else {
                                ColorRole::TERTIARY
                            }))
                            .child(plan.monthly_delta_text()),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(TEXT_AUX)
                            .text_color(color(if plan.reversible {
                                ColorRole::ON_SURFACE_VARIANT
                            } else {
                                ColorRole::ERROR
                            }))
                            .child(if plan.reversible {
                                "可逆"
                            } else {
                                "不可逆"
                            }),
                    ),
            )
            .child(
                div()
                    .text_size(TEXT_AUX)
                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                    .child(plan.summary.clone()),
            )
            .child(actions)
            .children((!plan.risks.is_empty()).then_some(risks))
            .child(
                div()
                    .flex()
                    .gap(rem(8.))
                    .child(widgets::text_button(
                        format!("plan-cancel-{}", plan.id),
                        action::CANCEL,
                        cx.listener(move |this, _, _, cx| {
                            this.decide_plan(plan_id_cancel.clone(), false, cx);
                        }),
                    ))
                    .child(widgets::filled_button(
                        format!("plan-confirm-{}", plan.id),
                        action::CONFIRM,
                        cx.listener(move |this, _, _, cx| {
                            this.decide_plan(plan_id_confirm.clone(), true, cx);
                        }),
                    )),
            )
            .into_any_element()
    }

    fn render_overview(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let balance = self
            .billing
            .balance_yuan
            .map_or_else(|| "—".to_owned(), |yuan| format!("¥{yuan:.2}"));

        let counts = self.inventory.counts();
        let monthly = money(self.inventory.monthly_total_cents());

        let mut count_rows = div().flex().flex_col();
        for (kind, count) in &counts {
            let kind = *kind;
            count_rows = count_rows.child(
                div()
                    .id(SharedString::from(format!("count-{}", kind.as_str())))
                    .flex()
                    .items_center()
                    .gap(rem(8.))
                    .px(rem(8.))
                    .py(px(6.))
                    .rounded(rem(10.))
                    .cursor_pointer()
                    .hover(|this| this.bg(color(ColorRole::SURFACE_CONTAINER_HIGH)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.filters = crate::app::Filters::default();
                        this.filters.kinds.insert(kind);
                        this.page = Page::Resources;
                        cx.notify();
                    }))
                    .child(div().flex_1().text_size(TEXT_AUX).child(kind.label()))
                    .child(
                        div()
                            .text_size(TEXT_AUX)
                            .font_weight(FontWeight::MEDIUM)
                            .child(count.to_string()),
                    ),
            );
        }

        let budget_limit: i64 = self
            .projects
            .iter()
            .map(|project| project.budget.limit_cents)
            .sum();
        let budget_spent: i64 = self
            .projects
            .iter()
            .map(|project| project.budget.spent_cents)
            .sum();
        let ratio = if budget_limit > 0 {
            (budget_spent as f32 / budget_limit as f32).clamp(0.0, 1.5)
        } else {
            0.0
        };

        let findings_savings: i64 = self
            .findings
            .iter()
            .map(|finding| finding.monthly_savings_cents)
            .sum();

        div()
            .flex()
            .flex_col()
            .gap(rem(10.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(rem(8.))
                    .child(
                        div()
                            .text_size(TEXT_TITLE)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("账户概览"),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(TEXT_AUX)
                            .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                            .child(format!("{} 个资源", self.inventory.resources.len())),
                    ),
            )
            .child(widgets::stat_card(
                "ov-balance",
                overview::BALANCE,
                balance,
                Some(monthly),
            ))
            .child(widgets::stat_card(
                "ov-spend",
                overview::SPEND,
                money(self.billing.month_cents),
                Some(format!(
                    "{} · {} 个项目",
                    if self.billing.by_product.is_empty() {
                        "无账单".to_owned()
                    } else {
                        self.billing
                            .by_product
                            .iter()
                            .take(2)
                            .map(|(product, cents)| format!("{product} {}", money(*cents)))
                            .collect::<Vec<_>>()
                            .join(" / ")
                    },
                    self.projects.len()
                )),
            ))
            .child(
                MaterialCard::new("ov-budget", CardVariant::Filled)
                    .flex()
                    .flex_col()
                    .gap(rem(8.))
                    .p(rem(16.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(TEXT_AUX)
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                                    .child(overview::BUDGET),
                            )
                            .child(
                                div()
                                    .text_size(TEXT_AUX)
                                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                                    .child(if budget_limit > 0 {
                                        format!("{} / {}", money(budget_spent), money(budget_limit))
                                    } else {
                                        "未设置预算".to_owned()
                                    }),
                            ),
                    )
                    .child(widgets::progress_bar(
                        "ov-budget-bar",
                        "预算使用",
                        ratio.min(1.0),
                        window,
                        cx,
                    )),
            )
            .child(
                MaterialCard::new("ov-counts", CardVariant::Filled)
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .p(rem(12.))
                    .child(
                        div()
                            .px(rem(8.))
                            .pb(rem(4.))
                            .text_size(TEXT_AUX)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                            .child(overview::RESOURCES),
                    )
                    .child(count_rows),
            )
            .child(
                MaterialCard::new("ov-findings", CardVariant::Filled)
                    .flex()
                    .flex_col()
                    .gap(rem(6.))
                    .p(rem(16.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(TEXT_AUX)
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                                    .child(overview::FINDINGS),
                            )
                            .child(
                                div()
                                    .text_size(TEXT_AUX)
                                    .text_color(color(if self.findings.is_empty() {
                                        ColorRole::ON_SURFACE_VARIANT
                                    } else {
                                        ColorRole::TERTIARY
                                    }))
                                    .child(if self.findings.is_empty() {
                                        "没有浪费".to_owned()
                                    } else {
                                        format!("可省 {} / 月", money(findings_savings))
                                    }),
                            ),
                    )
                    .children((!self.findings.is_empty()).then(|| {
                        widgets::tonal_button(
                            "ov-optimize",
                            action::OPTIMIZE,
                            cx.listener(|this, _, _, cx| {
                                this.ask("有浪费的钱吗？".to_owned(), cx);
                            }),
                        )
                    })),
            )
            .into_any_element()
    }
}

/// Kinds shown in the overview when the account is empty.
#[allow(dead_code)]
fn all_kinds() -> [ResourceKind; 8] {
    ResourceKind::ALL
}
