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
    CardSurface::new(CardVariant::Filled)
        .id(id)
        .bg(color(ColorRole::SURFACE_CONTAINER_LOWEST))
        .rounded(rem(24.))
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

/// Product emblem with the same rounded tile geometry as neko's server cards.
pub fn resource_icon(kind: adash_core::resource::ResourceKind) -> AnyElement {
    use adash_core::resource::ResourceKind;
    let symbol = match kind {
        ResourceKind::UHost => "dns",
        ResourceKind::Eip => "language",
        ResourceKind::Disk => "hard_drive",
        ResourceKind::Udb => "database",
        ResourceKind::Redis => "memory",
        ResourceKind::Ulb => "hub",
        _ => "cloud",
    };
    div()
        .size(rem(48.))
        .rounded(rem(16.))
        .bg(color(ColorRole::PRIMARY_CONTAINER))
        .text_color(color(ColorRole::ON_PRIMARY_CONTAINER))
        .flex()
        .items_center()
        .justify_center()
        .child(
            gpui_kit_material::icons::Icon::new(
                gpui_kit_material::icons::Symbol::named(symbol)
                    .expect("bundled product symbol")
                    .icon()
                    .asset_path(),
            )
            .size(rem(24.)),
        )
        .into_any_element()
}

/// An M3 rounded single-line field. Kit's `Input` owns editing, selection,
/// clipboard, IME and the caret; this only supplies the chrome, so every field
/// in the app shares one shape instead of the kit's 4dp text-field corners.
pub fn rounded_input(
    id: impl Into<ElementId>,
    state: &Entity<gpui::base::input::InputState>,
    label: Option<&'static str>,
) -> AnyElement {
    let mut input = gpui_kit::component::input::Input::new(state)
        .appearance(false)
        .p_0()
        .text_size(rem(15.));
    if let Some(label) = label {
        input = input.aria_label(label);
    }
    div()
        .id(id)
        .flex()
        .items_center()
        .h(px(48.))
        .px(rem(16.))
        .rounded(rem(24.))
        .bg(color(ColorRole::SURFACE_CONTAINER_HIGH))
        .border_1()
        .border_color(color(ColorRole::SURFACE_CONTAINER_HIGH))
        .child(div().flex_1().min_w_0().child(input))
        .into_any_element()
}

/// The multi-line counterpart: same surface, a 20dp radius, and the kit
/// textarea for editing.
pub fn rounded_area(
    id: impl Into<ElementId>,
    state: &Entity<gpui::base::input::TextareaState>,
    label: Option<&'static str>,
) -> AnyElement {
    let mut area = gpui_kit::component::input::Textarea::new(state)
        .appearance(false)
        .p_0()
        .text_size(rem(14.));
    if let Some(label) = label {
        area = area.aria_label(label);
    }
    div()
        .id(id)
        .flex()
        .p(rem(14.))
        .rounded(rem(20.))
        .bg(color(ColorRole::SURFACE_CONTAINER_HIGH))
        .child(div().flex_1().min_w_0().child(area))
        .into_any_element()
}

/// A rounded editor using the same font settings as the terminal.
pub fn rounded_area_with_font(
    id: impl Into<ElementId>,
    state: &Entity<gpui::base::input::TextareaState>,
    family: SharedString,
    size: f32,
) -> AnyElement {
    div()
        .id(id)
        .flex()
        .p(rem(14.))
        .rounded(rem(20.))
        .bg(color(ColorRole::SURFACE_CONTAINER_HIGH))
        .child(
            div().flex_1().min_w_0().child(
                gpui_kit::component::input::Textarea::new(state)
                    .appearance(false)
                    .p_0()
                    .font_family(family)
                    .text_size(rem(size))
                    .aria_label("文件内容"),
            ),
        )
        .into_any_element()
}

/// A labelled rounded field for forms: the label sits above, the field itself
/// stays a clean pill so it reads the same as the composer.
pub fn labelled_input(
    id: &'static str,
    label: &'static str,
    state: &Entity<gpui::base::input::InputState>,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(rem(6.))
        .child(
            div()
                .text_size(TEXT_AUX)
                .font_weight(FontWeight::MEDIUM)
                .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                .child(label),
        )
        .child(rounded_input(id, state, Some(label)))
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
        ResourceState::Running => ColorRole::SUCCESS,
        ResourceState::Stopped | ResourceState::Available => ColorRole::OUTLINE,
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
