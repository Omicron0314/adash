//! Settings: what this build is, what it is connected to, and where its state is.

use crate::app::Adash;
use crate::widgets;
use gpui::prelude::*;
use gpui::*;
use gpui_kit_material::prelude::*;

impl Adash {
    /// Settings page (read-only facts for now).
    pub fn render_settings(&self, _cx: &mut Context<Self>) -> AnyElement {
        let credentials = std::env::var("UCLOUD_PUBLIC_KEY").ok();
        let mock_dir = ucloud_api::transport::mock_dir();
        let state_path = self.store.path().to_path_buf();
        let agent = match &self.agent.offline_reason() {
            Some(reason) => format!("离线（{reason}）"),
            None => "已连接 pi".to_owned(),
        };

        div()
            .flex()
            .flex_col()
            .gap(rem(12.))
            .max_w(rem(720.))
            .child(
                div()
                    .text_size(TEXT_TITLE)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("设置"),
            )
            .child(
                MaterialCard::new("settings-account", CardVariant::Filled)
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .p(rem(16.))
                    .child(widgets::section_header("账户"))
                    .child(widgets::kv_row(
                        "模式",
                        if self.mode == ucloud_api::Mode::Mock {
                            "Mock（不访问网络，不花钱）"
                        } else {
                            "Live（真实账户）"
                        },
                    ))
                    .child(widgets::kv_row(
                        "PublicKey",
                        credentials
                            .map_or_else(|| "未设置".to_owned(), |key| ucloud_api::redact(&key)),
                    ))
                    .child(widgets::kv_row(
                        "凭证来源",
                        "环境变量 UCLOUD_PUBLIC_KEY / UCLOUD_PRIVATE_KEY（钥匙串在 M2 后续）",
                    ))
                    .child(widgets::kv_row("Mock 数据", mock_dir.display().to_string()))
                    .child(widgets::kv_row(
                        "本地状态",
                        state_path.display().to_string(),
                    )),
            )
            .child(
                MaterialCard::new("settings-agent", CardVariant::Filled)
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .p(rem(16.))
                    .child(widgets::section_header("代理"))
                    .child(widgets::kv_row("pi", agent))
                    .child(widgets::kv_row(
                        "扩展",
                        crate::agent::repo_root()
                            .join("agent/extensions/ucloud")
                            .display()
                            .to_string(),
                    ))
                    .child(widgets::kv_row(
                        "权限",
                        "--no-builtin-tools：没有 shell、没有文件写入",
                    ))
                    .child(widgets::kv_row(
                        "变更",
                        "只能在计划卡片确认后，由 adash-tool apply 执行",
                    )),
            )
            .child(
                MaterialCard::new("settings-build", CardVariant::Filled)
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .p(rem(16.))
                    .child(widgets::section_header("构建"))
                    .child(widgets::kv_row("版本", env!("CARGO_PKG_VERSION")))
                    .child(widgets::kv_row(
                        "组件",
                        "GPUI + gpui-kit-material（M3 Expressive）",
                    ))
                    .child(widgets::kv_row(
                        "参考资料",
                        "docs/design/neko.html · catussh 外壳",
                    )),
            )
            .into_any_element()
    }
}
