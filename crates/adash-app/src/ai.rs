//! User-owned model settings. Only the key reference is stored in JSON.
use gpui::{AppContext, Context, Entity, Window, base::input::InputState};
use serde_json::json;
use std::path::PathBuf;
use zeroize::Zeroizing;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AiSettings {
    pub endpoint: String,
    pub model: String,
    pub api: String,
    pub context_window: u32,
    pub max_tokens: u32,
    pub key_id: Option<String>,
}
impl Default for AiSettings {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            model: String::new(),
            api: "openai-completions".into(),
            context_window: 32768,
            max_tokens: 4096,
            key_id: None,
        }
    }
}
trait Secrets {
    fn get(&self, id: &str) -> Result<Zeroizing<String>, String>;
    fn set(&self, id: &str, key: &str) -> Result<(), String>;
    fn delete(&self, id: &str) -> Result<(), String>;
}
struct SystemSecrets;
impl Secrets for SystemSecrets {
    fn get(&self, id: &str) -> Result<Zeroizing<String>, String> {
        keyring::Entry::new("adash-ai", id)
            .and_then(|entry| entry.get_password())
            .map(Zeroizing::new)
            .map_err(|_| "无法读取 AI API key，请检查系统钥匙串或重新保存密钥".into())
    }
    fn set(&self, id: &str, key: &str) -> Result<(), String> {
        keyring::Entry::new("adash-ai", id)
            .and_then(|entry| entry.set_password(key))
            .map_err(|_| "API key 未能写入系统钥匙串，配置未保存".into())
    }
    fn delete(&self, id: &str) -> Result<(), String> {
        match keyring::Entry::new("adash-ai", id).and_then(|entry| entry.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err("钥匙串条目清理失败".into()),
        }
    }
}
impl AiSettings {
    pub fn path() -> PathBuf {
        adash_paths::data_dir().join("ai.json")
    }
    pub fn load() -> Result<Self, String> {
        Self::load_from(&Self::path())
    }
    fn load_from(path: &std::path::Path) -> Result<Self, String> {
        match std::fs::read(path) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|_| "AI 配置无法读取，请检查 ai.json".into())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(_) => Err("无法读取 AI 配置文件".into()),
        }
    }
    pub fn validate(&mut self) -> Result<(), String> {
        self.endpoint = self.endpoint.trim().trim_end_matches('/').into();
        self.model = self.model.trim().into();
        let endpoint = url::Url::parse(&self.endpoint)
            .map_err(|_| "请输入完整的 HTTP 或 HTTPS 服务地址".to_owned())?;
        if !matches!(endpoint.scheme(), "https" | "http")
            || endpoint.host_str().is_none()
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err("Endpoint 必须是 HTTP/HTTPS Base URL，不能包含凭据、查询参数或片段".into());
        }
        if self.model.is_empty() || self.model.chars().any(char::is_control) {
            return Err("请填写服务提供的模型 ID".into());
        }
        if ![
            "openai-completions",
            "openai-responses",
            "anthropic-messages",
        ]
        .contains(&self.api.as_str())
        {
            return Err("不支持的 API 协议".into());
        }
        if self.max_tokens == 0 || self.context_window <= self.max_tokens {
            return Err("上下文长度必须大于最大输出长度，且两者均为正整数".into());
        }
        Ok(())
    }
    pub fn secret(&self) -> Result<Zeroizing<String>, String> {
        let id = self.key_id.as_deref().ok_or("请在账户页填写 API key")?;
        SystemSecrets.get(id)
    }
    pub fn persist(self, key: &str) -> Result<(Self, Option<String>), String> {
        self.persist_with(key, &Self::path(), &SystemSecrets)
    }
    fn persist_with(
        mut self,
        key: &str,
        path: &std::path::Path,
        secrets: &impl Secrets,
    ) -> Result<(Self, Option<String>), String> {
        self.validate()?;
        // A corrupt config must remain recoverable by entering a new key.
        let previous = Self::load_from(path).unwrap_or_default();
        let fresh = !key.trim().is_empty();
        if fresh {
            if key.chars().any(char::is_control) {
                return Err("API key 不能包含换行或控制字符".into());
            }
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| "无法生成密钥引用")?
                .as_nanos();
            let id = format!("provider-{stamp}");
            secrets.set(&id, key.trim())?;
            self.key_id = Some(id);
        } else {
            if self.endpoint != previous.endpoint {
                return Err("更换 Endpoint 时请重新填写 API key，避免向新服务发送旧密钥".into());
            }
            self.key_id = previous.key_id.clone();
            secrets.get(self.key_id.as_deref().ok_or("请填写 API key")?)?;
        }
        if adash_ssh::save_private_json(path, &self).is_err() {
            if fresh && let Some(id) = &self.key_id {
                let _ = secrets.delete(id);
            }
            return Err("AI 配置保存失败，原配置已保留".into());
        }
        let warning = previous
            .key_id
            .filter(|id| Some(id) != self.key_id.as_ref())
            .and_then(|id| {
                secrets
                    .delete(&id)
                    .err()
                    .map(|_| "新配置已保存，但旧密钥清理失败".into())
            });
        Ok((self, warning))
    }
    pub fn remove() -> Result<(), String> {
        let previous = Self::load().unwrap_or_default();
        adash_ssh::save_private_json(&Self::path(), &Self::default())
            .map_err(|_| "无法移除 AI 配置")?;
        if let Some(id) = previous.key_id {
            SystemSecrets
                .delete(&id)
                .map_err(|_| "配置已移除，但系统钥匙串中的旧密钥清理失败")?;
        }
        Ok(())
    }
    pub fn startup(&self, key: &str) -> serde_json::Value {
        json!({"endpoint":self.endpoint,"model":self.model,"api":self.api,"context_window":self.context_window,"max_tokens":self.max_tokens,"api_key":key})
    }
}
pub struct AiForm {
    pub endpoint: Entity<InputState>,
    pub model: Entity<InputState>,
    pub key: Entity<InputState>,
    pub context_window: Entity<InputState>,
    pub max_tokens: Entity<InputState>,
    pub api: String,
}
impl AiForm {
    pub fn new(
        settings: &AiSettings,
        window: &mut Window,
        cx: &mut Context<crate::app::Adash>,
    ) -> Self {
        let mut field = |value: &str, masked: bool, cx: &mut Context<crate::app::Adash>| {
            let state = cx.new(|cx| InputState::new(window, cx).masked(masked));
            state.update(cx, |state, cx| state.set_value(value, window, cx));
            state
        };
        Self {
            endpoint: field(&settings.endpoint, false, cx),
            model: field(&settings.model, false, cx),
            key: field("", true, cx),
            context_window: field(&settings.context_window.to_string(), false, cx),
            max_tokens: field(&settings.max_tokens.to_string(), false, cx),
            api: settings.api.clone(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct MemorySecrets {
        values: std::cell::RefCell<std::collections::BTreeMap<String, String>>,
        fail: std::cell::Cell<bool>,
    }
    impl Secrets for MemorySecrets {
        fn get(&self, id: &str) -> Result<Zeroizing<String>, String> {
            self.values
                .borrow()
                .get(id)
                .cloned()
                .map(Zeroizing::new)
                .ok_or("missing key".into())
        }
        fn set(&self, id: &str, key: &str) -> Result<(), String> {
            if self.fail.get() {
                return Err("keyring unavailable".into());
            }
            self.values.borrow_mut().insert(id.into(), key.into());
            Ok(())
        }
        fn delete(&self, id: &str) -> Result<(), String> {
            self.values.borrow_mut().remove(id);
            Ok(())
        }
    }
    #[test]
    fn endpoint_changes_require_a_new_key_and_keyring_failures_preserve_settings() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ai.json");
        let secrets = MemorySecrets::default();
        let first = AiSettings {
            endpoint: "https://one.example/v1".into(),
            model: "user/model".into(),
            ..Default::default()
        };
        let (saved, _) = first.persist_with("KEY_ONE", &path, &secrets).unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(!String::from_utf8_lossy(&before).contains("KEY_ONE"));
        let changed = AiSettings {
            endpoint: "https://two.example/v1".into(),
            ..saved.clone()
        };
        assert!(changed.clone().persist_with("", &path, &secrets).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let (second, _) = changed.persist_with("KEY_TWO", &path, &secrets).unwrap();
        assert!(
            !secrets
                .values
                .borrow()
                .contains_key(saved.key_id.as_ref().unwrap())
        );
        assert_eq!(
            secrets
                .get(second.key_id.as_ref().unwrap())
                .unwrap()
                .as_str(),
            "KEY_TWO"
        );
        let (same, _) = second.clone().persist_with("", &path, &secrets).unwrap();
        assert_eq!(same.key_id, second.key_id);
        secrets.fail.set(true);
        let before = std::fs::read(&path).unwrap();
        assert!(
            second
                .clone()
                .persist_with("KEY_THREE", &path, &secrets)
                .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        std::fs::write(&path, "not json").unwrap();
        secrets.fail.set(false);
        let (recovered, _) = second.persist_with("KEY_FOUR", &path, &secrets).unwrap();
        assert_eq!(recovered.endpoint, "https://two.example/v1");
        assert_eq!(
            secrets
                .get(recovered.key_id.as_ref().unwrap())
                .unwrap()
                .as_str(),
            "KEY_FOUR"
        );
    }
    #[test]
    fn user_endpoints_and_model_ids_are_preserved_without_credentials_in_config() {
        let mut settings = AiSettings {
            endpoint: " https://models.example.org/proxy/v1/ ".into(),
            model: "vendor/nested/model".into(),
            ..Default::default()
        };
        settings.validate().unwrap();
        assert_eq!(settings.endpoint, "https://models.example.org/proxy/v1");
        assert_eq!(settings.model, "vendor/nested/model");
        assert!(
            !serde_json::to_string(&settings)
                .unwrap()
                .contains("api_key")
        );
        settings.endpoint = "https://secret@example.org/v1".into();
        assert!(settings.validate().is_err());
        settings.endpoint = "https://example.org/v1?api_key=secret".into();
        assert!(settings.validate().is_err());
    }
}
