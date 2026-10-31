//! Account page: UCloud credentials and the embedded AI service.
use crate::{app::Adash, widgets};
use gpui::{prelude::*, *};
use gpui_kit_material::prelude::*;
use ucloud_api::{CloudProfile, Credentials, Mode, UcloudClient};

impl Adash {
    pub fn render_account(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let cloud = self.account_cloud_card(cx);
        let ai = self.account_ai_card(cx);
        div()
            .id("account-scroll")
            .flex()
            .flex_col()
            .gap(rem(18.))
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(rem(6.))
                    .child(
                        div()
                            .text_size(rem(40.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("账户"),
                    )
                    .child(
                        div()
                            .text_size(TEXT_AUX)
                            .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                            .child("UCloud 凭据和 AI 服务都只保存在本机；密钥写入系统钥匙串。"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(rem(18.))
                    .flex_wrap()
                    .items_start()
                    .child(div().flex_1().min_w(rem(420.)).child(cloud))
                    .child(div().flex_1().min_w(rem(420.)).child(ai)),
            )
            .into_any_element()
    }

    fn account_cloud_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let live = self.mode == Mode::Live;
        let profile = self.cloud_profile.as_ref();
        let status = match (live, profile) {
            (true, Some(profile)) => format!(
                "LIVE · {} · {}",
                if profile.region.is_empty() {
                    "未设置地域"
                } else {
                    &profile.region
                },
                profile.public_key_redacted()
            ),
            (true, None) => "LIVE · 已通过环境变量连接".to_owned(),
            (false, Some(profile)) if !profile.enabled => format!(
                "MOCK · 已保存 {} 的凭据，当前未启用",
                profile.public_key_redacted()
            ),
            _ => "MOCK · 使用内置样例，不会产生费用".to_owned(),
        };
        let balance = self
            .inventory
            .balance_yuan
            .map(|value| format!("¥{value:.2}"))
            .unwrap_or_else(|| "—".to_owned());
        CardSurface::new(CardVariant::Filled)
            .bg(color(ColorRole::SURFACE_CONTAINER_LOWEST))
            .rounded(rem(24.))
            .p(rem(20.))
            .gap(rem(16.))
            .child(widgets::section_header("UCloud 云账户"))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(rem(4.))
                    .child(div().text_size(TEXT_BODY).child(status))
                    .child(
                        div()
                            .text_size(TEXT_AUX)
                            .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                            .child(format!("余额 {balance} · 只读连接，变更仍需在计划卡片确认")),
                    ),
            )
            .children(
                self.client_error
                    .clone()
                    .or_else(|| self.cloud_error.clone())
                    .map(|error| widgets::notice(error, true)),
            )
            .child(widgets::labelled_input(
                "cloud-public",
                "PublicKey",
                &self.cloud_form.public_key,
            ))
            .child(widgets::labelled_input(
                "cloud-private",
                "PrivateKey",
                &self.cloud_form.private_key,
            ))
            .child(
                div()
                    .text_size(TEXT_AUX)
                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                    .child("PrivateKey 只写入系统钥匙串；已有配置留空可保留，换 PublicKey 必须重新填写。"),
            )
            .child(
                div()
                    .flex()
                    .gap(rem(12.))
                    .child(div().flex_1().min_w(rem(160.)).child(widgets::labelled_input(
                        "cloud-project",
                        "ProjectId（可选）",
                        &self.cloud_form.project_id,
                    )))
                    .child(div().flex_1().min_w(rem(140.)).child(widgets::labelled_input(
                        "cloud-region",
                        "地域",
                        &self.cloud_form.region,
                    ))),
            )
            .child(widgets::labelled_input(
                "cloud-endpoint",
                "Endpoint（可选）",
                &self.cloud_form.endpoint,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(rem(8.))
                    .child(
                        MaterialButton::new("cloud-connect")
                            .variant(ButtonVariant::Filled)
                            .disabled(self.cloud_saving || self.agent.streaming)
                            .on_click(cx.listener(|this, _, window, cx| this.save_cloud(window, cx)))
                            .child(if self.cloud_saving {
                                "正在验证…"
                            } else {
                                "保存并连接"
                            }),
                    )
                    .child(
                        MaterialButton::new("cloud-mock")
                            .variant(ButtonVariant::Tonal)
                            .disabled(self.cloud_saving || self.agent.streaming || !live)
                            .on_click(cx.listener(|this, _, _, cx| this.use_mock_cloud(cx)))
                            .child("切换到 Mock"),
                    )
                    .children(profile.is_some().then(|| {
                        widgets::text_button(
                            "cloud-remove",
                            "移除云账户",
                            cx.listener(|this, _, _, cx| {
                                this.pending_remove_cloud = true;
                                cx.notify();
                            }),
                        )
                    })),
            )
            .into_any_element()
    }

    fn account_ai_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut protocols = div().flex().flex_wrap().gap(rem(8.));
        for (id, label) in [
            ("openai-completions", "Chat Completions"),
            ("openai-responses", "Responses"),
            ("anthropic-messages", "Anthropic Messages"),
        ] {
            protocols = protocols.child(widgets::chip(
                format!("ai-api-{id}"),
                label,
                self.ai_form.api == id,
                cx.listener(move |this, selected: &bool, _, cx| {
                    if *selected && !this.ai_saving {
                        this.ai_form.api = id.into();
                        cx.notify();
                    }
                }),
            ));
        }
        let status = self
            .agent
            .offline_reason()
            .map(ToOwned::to_owned)
            .or_else(|| self.agent.status.clone())
            .unwrap_or_else(|| {
                self.agent
                    .model
                    .as_ref()
                    .map_or("正在连接 AI".into(), |model| {
                        format!("已配置 · {model}")
                    })
            });
        CardSurface::new(CardVariant::Filled)
            .bg(color(ColorRole::SURFACE_CONTAINER_LOWEST))
            .rounded(rem(24.))
            .p(rem(20.))
            .gap(rem(16.))
            .child(widgets::section_header("AI 服务"))
            .child(
                div()
                    .text_size(TEXT_AUX)
                    .child("填写 OpenAI 兼容或 Anthropic 服务的 Base URL、API key 和模型 ID。"),
            )
            .child(protocols)
            .child(widgets::labelled_input(
                "ai-endpoint",
                "Endpoint（Base URL）",
                &self.ai_form.endpoint,
            ))
            .child(widgets::labelled_input(
                "ai-model",
                "模型 ID",
                &self.ai_form.model,
            ))
            .child(widgets::labelled_input(
                "ai-key",
                "API key",
                &self.ai_form.key,
            ))
            .child(
                div()
                    .text_size(TEXT_AUX)
                    .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                    .child(if self.ai_settings.key_id.is_some() {
                        "密钥已存入系统钥匙串。留空保留当前密钥；更换 Endpoint 需重新填写。"
                    } else {
                        "API key 仅保存在系统钥匙串，不写入配置文件或对话。"
                    }),
            )
            .child(
                div()
                    .flex()
                    .gap(rem(12.))
                    .child(div().flex_1().child(widgets::labelled_input(
                        "ai-context",
                        "上下文长度（tokens）",
                        &self.ai_form.context_window,
                    )))
                    .child(div().flex_1().child(widgets::labelled_input(
                        "ai-output",
                        "最大输出（tokens）",
                        &self.ai_form.max_tokens,
                    ))),
            )
            .children(
                self.ai_error
                    .clone()
                    .map(|error| widgets::notice(error, true)),
            )
            .child(div().text_size(TEXT_AUX).child(status))
            .child(
                div()
                    .flex()
                    .gap(rem(8.))
                    .child(
                        MaterialButton::new("ai-save")
                            .variant(ButtonVariant::Filled)
                            .disabled(self.ai_saving || self.agent.streaming)
                            .on_click(cx.listener(|this, _, window, cx| this.save_ai(window, cx)))
                            .child(if self.ai_saving {
                                "正在保存…"
                            } else {
                                "保存并连接"
                            }),
                    )
                    .child(
                        MaterialButton::new("ai-remove")
                            .variant(ButtonVariant::Text)
                            .disabled(self.ai_saving || self.agent.streaming)
                            .on_click(cx.listener(|this, _, window, cx| this.remove_ai(window, cx)))
                            .child("移除配置与密钥"),
                    ),
            )
            .into_any_element()
    }

    fn save_cloud(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.cloud_saving || self.agent.streaming {
            return;
        }
        let mut profile = CloudProfile {
            public_key: self.cloud_form.public_key.read(cx).value().to_string(),
            project_id: self.cloud_form.project_id.read(cx).value().to_string(),
            region: self.cloud_form.region.read(cx).value().to_string(),
            endpoint: self.cloud_form.endpoint.read(cx).value().to_string(),
            enabled: true,
        };
        if let Err(error) = profile.validate() {
            self.cloud_error = Some(error);
            cx.notify();
            return;
        }
        let private = self
            .cloud_form
            .private_key
            .read(cx)
            .value()
            .trim()
            .to_owned();
        let existing = self.cloud_profile.as_ref();
        let credentials = if private.is_empty() {
            let Some(existing) = existing.filter(|saved| saved.public_key == profile.public_key)
            else {
                self.cloud_error =
                    Some("请填写 PrivateKey；更换 PublicKey 时也必须重新填写".into());
                cx.notify();
                return;
            };
            match existing.credentials_with(
                &profile.public_key,
                &profile.project_id,
                &profile.region,
            ) {
                Ok(credentials) => credentials,
                Err(error) => {
                    self.cloud_error = Some(format!("无法读取已保存的 PrivateKey：{error}"));
                    cx.notify();
                    return;
                }
            }
        } else {
            match Credentials::new(
                profile.public_key.clone(),
                private.clone(),
                Some(profile.project_id.clone()).filter(|value| !value.is_empty()),
                Some(profile.region.clone()).filter(|value| !value.is_empty()),
            ) {
                Ok(credentials) => credentials,
                Err(error) => {
                    self.cloud_error = Some(error.to_string());
                    cx.notify();
                    return;
                }
            }
        };
        let client = match UcloudClient::live_with(
            credentials,
            Some(profile.endpoint.clone()).filter(|value| !value.is_empty()),
            Some(profile.region.clone()).filter(|value| !value.is_empty()),
        ) {
            Ok(client) => client,
            Err(error) => {
                self.cloud_error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        self.cloud_saving = true;
        self.cloud_error = None;
        cx.notify();
        // A single cheap read verifies the key, signature, endpoint and region
        // before saving anything. The UI blocks for this one call by design.
        if let Err(error) = client.balance() {
            self.cloud_saving = false;
            self.cloud_error = Some(format!("连接失败，未保存：{error}"));
            cx.notify();
            return;
        }
        let key = (!private.is_empty()).then_some(private.as_str());
        match profile.clone().save(key) {
            Ok(saved) => {
                self.cloud_profile = Some(saved.clone());
                self.cloud_form = crate::cloud::CloudForm::new(Some(&saved), window, cx);
                self.client = Some(client);
                self.mode = Mode::Live;
                self.client_error = None;
                self.cloud_error = None;
                self.refresh();
                self.agent = crate::agent::Agent::connect(Mode::Live);
                self.notice = Some("已连接 UCloud，账户数据刷新中".into());
            }
            Err(error) => self.cloud_error = Some(error),
        }
        self.cloud_saving = false;
        cx.notify();
    }

    fn use_mock_cloud(&mut self, cx: &mut Context<Self>) {
        if self.cloud_saving || self.agent.streaming {
            return;
        }
        if let Some(profile) = self.cloud_profile.clone() {
            if let Err(error) = profile.set_enabled(false) {
                self.cloud_error = Some(error);
                cx.notify();
                return;
            }
            let mut profile = profile;
            profile.enabled = false;
            self.cloud_profile = Some(profile);
        }
        self.client = Some(UcloudClient::mock());
        self.mode = Mode::Mock;
        self.client_error = None;
        self.cloud_error = None;
        self.refresh();
        self.agent = crate::agent::Agent::connect(Mode::Mock);
        self.notice = Some("已切换到 Mock 数据；保存的凭据仍保留".into());
        cx.notify();
    }

    fn save_ai(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ai_saving || self.agent.streaming {
            return;
        }
        let mut settings = crate::ai::AiSettings {
            endpoint: self.ai_form.endpoint.read(cx).value().to_string(),
            model: self.ai_form.model.read(cx).value().to_string(),
            api: self.ai_form.api.clone(),
            key_id: None,
            context_window: self
                .ai_form
                .context_window
                .read(cx)
                .value()
                .trim()
                .parse()
                .unwrap_or(0),
            max_tokens: self
                .ai_form
                .max_tokens
                .read(cx)
                .value()
                .trim()
                .parse()
                .unwrap_or(0),
        };
        if let Err(error) = settings.validate() {
            self.ai_error = Some(error);
            cx.notify();
            return;
        }
        let key = zeroize::Zeroizing::new(self.ai_form.key.read(cx).value().to_string());
        self.ai_saving = true;
        self.ai_error = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move { settings.persist(&key) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.ai_saving = false;
                match result {
                    Ok((settings, warning)) => {
                        this.ai_settings = settings;
                        this.ai_form = crate::ai::AiForm::new(&this.ai_settings, window, cx);
                        this.ai_error = warning;
                        this.agent = crate::agent::Agent::connect(this.mode);
                        this.notice = Some("AI 配置已保存，正在连接新会话".into());
                    }
                    Err(error) => this.ai_error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn remove_ai(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ai_saving || self.agent.streaming {
            return;
        }
        self.ai_saving = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async { crate::ai::AiSettings::remove() })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.ai_saving = false;
                this.ai_settings = crate::ai::AiSettings::load().unwrap_or_default();
                this.ai_form = crate::ai::AiForm::new(&this.ai_settings, window, cx);
                this.ai_error = result.err();
                this.agent = crate::agent::Agent::connect(this.mode);
                cx.notify();
            });
        })
        .detach();
    }
}
