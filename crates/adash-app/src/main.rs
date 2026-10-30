//! adash desktop app. Startup order follows gpui-kit-material:
//! application → gpui_kit::init → gpui_kit_material::init →
//! theme::init_component_theme → gpui_kit::open_window.
extern crate gpui_kit as gpui;

use gpui::{prelude::*, *};
use gpui_kit_material::icons::Assets;
use gpui_kit_material::prelude::*;
use gpui_kit_material::theme;

fn main() {
    gpui_kit::application()
        .with_assets(Assets::new())
        .run(|cx: &mut App| {
            gpui_kit::init(cx);
            gpui_kit_material::init(cx);
            let scheme = adash_app::palette::light_scheme();
            theme::init_component_theme(scheme, 1.0, cx);
            let mut options =
                title_bar_window_options("adash", px(32.), size(px(1440.), px(900.)), cx);
            // Lets the compositor match the window to adash.desktop for its icon.
            options.app_id = Some("adash".into());
            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| adash_app::app::Adash::new(window, cx))
            })
            .expect("failed to open window");
        });
}
