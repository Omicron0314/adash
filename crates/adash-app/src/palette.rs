use gpui_kit_material::prelude::*;

// Exact light roles from neko.html's .m3 template; views only consume roles.
pub fn light_scheme() -> gpui_kit_material::color::Scheme {
    use gpui_kit_material::color::Color;
    let mut scheme = ThemeId::default().scheme(ThemeMode::Light, Contrast::Standard);
    for (role, hex) in [
        (ColorRole::SURFACE, 0xf4f1fa),
        (ColorRole::SURFACE_CONTAINER_LOWEST, 0xffffff),
        (ColorRole::SURFACE_CONTAINER_LOW, 0xf9f6fe),
        (ColorRole::SURFACE_CONTAINER, 0xf0ecf6),
        (ColorRole::SURFACE_CONTAINER_HIGH, 0xeae6f0),
        (ColorRole::SURFACE_CONTAINER_HIGHEST, 0xe4e1ea),
        (ColorRole::PRIMARY, 0x4b49c8),
        (ColorRole::ON_PRIMARY, 0xffffff),
        (ColorRole::PRIMARY_CONTAINER, 0xe2dfff),
        (ColorRole::ON_PRIMARY_CONTAINER, 0x2c2891),
        (ColorRole::SECONDARY, 0x5d5c72),
        (ColorRole::SECONDARY_CONTAINER, 0xe2e0f9),
        (ColorRole::ON_SECONDARY_CONTAINER, 0x1a1a2c),
        (ColorRole::TERTIARY, 0x8a4a60),
        (ColorRole::TERTIARY_CONTAINER, 0xffd8e4),
        (ColorRole::ON_TERTIARY_CONTAINER, 0x3e1020),
        (ColorRole::ON_SURFACE, 0x1c1b22),
        (ColorRole::ON_SURFACE_VARIANT, 0x47464f),
        (ColorRole::OUTLINE, 0x787680),
        (ColorRole::OUTLINE_VARIANT, 0xc8c5d0),
        (ColorRole::INVERSE_SURFACE, 0x16161d),
        (ColorRole::INVERSE_ON_SURFACE, 0xe6e7ea),
        (ColorRole::SUCCESS, 0x246b3a),
        (ColorRole::SUCCESS_BG, 0xb4f1c0),
    ] {
        scheme = scheme.with_role(role, Color::hex(hex));
    }
    scheme
}

pub fn resolve(appearance: &str, cx: &gpui_kit::App) -> gpui_kit_material::color::Scheme {
    let dark =
        appearance == "dark" || appearance == "system" && gpui_kit_material::theme::system_dark(cx);
    if !dark {
        return light_scheme();
    }
    use gpui_kit_material::color::Color;
    let mut scheme = ThemeId::default().scheme(ThemeMode::Dark, Contrast::Standard);
    for (role, hex) in [
        (ColorRole::SURFACE, 0x121218),
        (ColorRole::SURFACE_CONTAINER_LOWEST, 0x0e0e14),
        (ColorRole::SURFACE_CONTAINER_LOW, 0x1c1b22),
        (ColorRole::SURFACE_CONTAINER, 0x201f27),
        (ColorRole::SURFACE_CONTAINER_HIGH, 0x2a2931),
        (ColorRole::SURFACE_CONTAINER_HIGHEST, 0x35343c),
        (ColorRole::PRIMARY, 0xc3c0ff),
        (ColorRole::ON_PRIMARY, 0x1f1a8a),
        (ColorRole::PRIMARY_CONTAINER, 0x3a36ab),
        (ColorRole::ON_PRIMARY_CONTAINER, 0xe2dfff),
        (ColorRole::SECONDARY_CONTAINER, 0x45455a),
        (ColorRole::ON_SECONDARY_CONTAINER, 0xe2e0f9),
        (ColorRole::TERTIARY_CONTAINER, 0x6b3a4b),
        (ColorRole::ON_TERTIARY_CONTAINER, 0xffd8e4),
        (ColorRole::INVERSE_SURFACE, 0x0b0b10),
        (ColorRole::INVERSE_ON_SURFACE, 0xe6e7ea),
    ] {
        scheme = scheme.with_role(role, Color::hex(hex));
    }
    scheme
}
