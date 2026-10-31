//! UCloud account form. The private key is masked and only submitted to the
//! keyring after a read-only connection check succeeds.
use gpui::{AppContext, Context, Entity, Window, base::input::InputState};
use ucloud_api::CloudProfile;

pub struct CloudForm {
    pub public_key: Entity<InputState>,
    pub private_key: Entity<InputState>,
    pub project_id: Entity<InputState>,
    pub region: Entity<InputState>,
    pub endpoint: Entity<InputState>,
}
impl CloudForm {
    pub fn new(
        profile: Option<&CloudProfile>,
        window: &mut Window,
        cx: &mut Context<crate::app::Adash>,
    ) -> Self {
        let mut field = |value: &str,
                         placeholder: &'static str,
                         masked: bool,
                         cx: &mut Context<crate::app::Adash>| {
            let state = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .masked(masked)
            });
            state.update(cx, |state, cx| state.set_value(value, window, cx));
            state
        };
        Self {
            public_key: field(
                profile.map_or("", |profile| profile.public_key.as_str()),
                "UCloud PublicKey",
                false,
                cx,
            ),
            private_key: field("", "留空保留已存 PrivateKey", true, cx),
            project_id: field(
                profile.map_or("", |profile| profile.project_id.as_str()),
                "ProjectId（可选）",
                false,
                cx,
            ),
            region: field(
                profile.map_or("cn-bj2", |profile| profile.region.as_str()),
                "地域，例如 cn-bj2",
                false,
                cx,
            ),
            endpoint: field(
                profile.map_or("", |profile| profile.endpoint.as_str()),
                "默认 https://api.ucloud.cn",
                false,
                cx,
            ),
        }
    }
}
