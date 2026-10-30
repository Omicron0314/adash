//! Thin Material compositions this app needs and the kit does not ship:
//! statistic cards, key/value rows, status dots, chips and section headers.
//!
//! Behaviour stays with the kit (`MaterialCard`, `ListItem`, `FilterChip`); this
//! module only arranges them with semantic roles and rem sizing.

use gpui::prelude::*;
use gpui::*;
use gpui_kit_material::prelude::*;
use gpui_kit_material::theme;

/// A card with a title and body, used for every overview tile.
pub fn stat_card(
    id: impl Into<ElementId>,
    title: &'static str,
    value: String,
    caption: Option<String>,
) -> AnyElement {
    MaterialCard::new(id, CardVariant::Filled)
        .flex()
        .flex_col()
        .gap(rem(6.))
        .p(rem(16.))
        .child(
            div()
                .text_size(TEXT_AUX)
                .font_weight(FontWeight::MEDIUM)
                .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                .child(title),
        )
        .child(
            div()
                .text_size(TEXT_HEADLINE)
                .font_weight(FontWeight::SEMIBOLD)
                .child(value),
        )
        .children(caption.map(|caption| {
            div()
                .text_size(TEXT_AUX)
                .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                .child(caption)
        }))
        .into_any_element()
}

/// A small uppercase section header above a block.
pub fn section_header(title: impl Into<SharedString>) -> AnyElement {
    div()
        .text_size(TEXT_AUX)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color(ColorRole::ON_SURFACE_VARIANT))
        .child(title.into())
        .into_any_element()
}

/// `label   value` row used in detail panels.
pub fn kv_row(label: impl Into<SharedString>, value: impl Into<SharedString>) -> AnyElement {
    div()
        .flex()
        .items_start()
        .gap(rem(12.))
        .py(rem(3.))
        .child(
            div()
                .w(rem(96.))
                .flex_shrink_0()
                .text_size(TEXT_AUX)
                .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                .child(label.into()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(TEXT_BODY)
                .child(value.into()),
        )
        .into_any_element()
}

/// A 8px round dot in the state's colour.
pub fn state_dot(state: adash_core::ResourceState) -> AnyElement {
    use adash_core::ResourceState;
    let role = match state {
        ResourceState::Running => ColorRole::TERTIARY,
        ResourceState::Stopped => ColorRole::OUTLINE,
        ResourceState::Pending => ColorRole::SECONDARY,
        ResourceState::Error => ColorRole::ERROR,
        ResourceState::Released => ColorRole::OUTLINE_VARIANT,
    };
    div()
        .size(px(8.))
        .flex_shrink_0()
        .rounded_full()
        .bg(color(role))
        .into_any_element()
}

/// A state pill: dot + label, never colour alone.
pub fn state_pill(state: adash_core::ResourceState) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(rem(6.))
        .px(rem(8.))
        .py(px(2.))
        .rounded_full()
        .bg(color(ColorRole::SURFACE_CONTAINER_HIGH))
        .child(state_dot(state))
        .child(
            div()
                .text_size(TEXT_AUX)
                .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                .child(state.label()),
        )
        .into_any_element()
}

/// A selected/unselected filter chip.
pub fn chip(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
    on_toggle: impl Fn(&bool, &mut Window, &mut App) + 'static,
) -> AnyElement {
    FilterChip::new(id, label)
        .selected(selected)
        .on_toggle(on_toggle)
        .into_any_element()
}

/// A one-line notice at the top of the content area.
pub fn notice(text: impl Into<SharedString>, error: bool) -> AnyElement {
    let (bg, fg) = if error {
        (ColorRole::ERROR_CONTAINER, ColorRole::ON_ERROR_CONTAINER)
    } else {
        (
            ColorRole::SECONDARY_CONTAINER,
            ColorRole::ON_SECONDARY_CONTAINER,
        )
    };
    div()
        .flex()
        .items_center()
        .gap(rem(8.))
        .px(rem(14.))
        .py(rem(8.))
        .rounded(rem(12.))
        .bg(color(bg))
        .text_color(color(fg))
        .text_size(TEXT_AUX)
        .child(text.into())
        .into_any_element()
}

/// An empty state with an optional action button.
pub fn empty_state(title: impl Into<SharedString>, hint: impl Into<SharedString>) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(rem(6.))
        .p(rem(32.))
        .rounded(rem(16.))
        .border_1()
        .border_color(color(ColorRole::OUTLINE_VARIANT))
        .child(
            div()
                .text_size(TEXT_BODY)
                .font_weight(FontWeight::MEDIUM)
                .child(title.into()),
        )
        .child(
            div()
                .text_size(TEXT_AUX)
                .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                .child(hint.into()),
        )
        .into_any_element()
}

/// A Material text button.
pub fn text_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    MaterialButton::new(id)
        .variant(ButtonVariant::Text)
        .on_click(on_click)
        .child(label.into())
        .into_any_element()
}

/// A filled (primary) button.
pub fn filled_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    MaterialButton::new(id)
        .variant(ButtonVariant::Filled)
        .on_click(on_click)
        .child(label.into())
        .into_any_element()
}

/// A tonal button, for secondary actions inside cards.
pub fn tonal_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    MaterialButton::new(id)
        .variant(ButtonVariant::Tonal)
        .on_click(on_click)
        .child(label.into())
        .into_any_element()
}

/// A measured progress bar with a label, 4dp.
pub fn progress_bar(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    value: f32,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    gpui_kit_material::prelude::progress_with_label(id, label, value, window, cx)
}

/// A vertical hairline between two columns.
pub fn vertical_divider() -> AnyElement {
    div()
        .w(px(1.))
        .h_full()
        .bg(color(ColorRole::OUTLINE_VARIANT))
        .into_any_element()
}

/// A horizontally scrolling row of filter chips.
pub fn chip_row(children: Vec<AnyElement>) -> AnyElement {
    let mut row = div().flex().items_center().gap(rem(6.)).flex_wrap();
    for child in children {
        row = row.child(child);
    }
    row.into_any_element()
}

/// The semantic colour role for the theme's surface, exported for views that
/// need to match the shell (e.g. the flow canvas).
pub fn canvas_background() -> Rgba {
    theme::color(ColorRole::SURFACE_CONTAINER_LOW)
}
