//! User-owned UCloud account configuration.
//!
//! The private key lives only in the system keyring. `cloud.json` stores the
//! public key, project/region/endpoint and whether the account is enabled.
//! Both the GUI and `adash-tool` load this profile, so the agent's cloud reads
//! use the same account without putting secrets in model context.
use crate::{Credentials, Error, Result, UcloudClient};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

const SERVICE: &str = "adash-cloud";
const USER: &str = "private-key";
const DEFAULT_ENDPOINT: &str = "https://api.ucloud.cn";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CloudProfile {
    pub public_key: String,
    pub project_id: String,
    pub region: String,
    pub endpoint: String,
    pub enabled: bool,
}
impl CloudProfile {
    /// `<data dir>/cloud.json`.
    pub fn path() -> PathBuf {
        adash_paths::data_dir().join("cloud.json")
    }

    /// Load the saved profile. Missing is `None`; corrupt is an error so the
    /// caller does not silently overwrite it.
    pub fn load() -> Result<Option<Self>> {
        match std::fs::read(Self::path()) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|error| Error::Credentials(format!("云账户配置损坏：{error}"))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(Error::Credentials(format!("无法读取云账户配置：{error}"))),
        }
    }

    /// True when a profile exists and was explicitly enabled.
    pub fn is_enabled() -> bool {
        Self::load()
            .ok()
            .flatten()
            .is_some_and(|profile| profile.enabled)
    }

    /// Display form of the public key.
    pub fn public_key_redacted(&self) -> String {
        crate::redact(&self.public_key)
    }

    pub fn validate(&mut self) -> std::result::Result<(), String> {
        self.public_key = self.public_key.trim().to_owned();
        self.project_id = self.project_id.trim().to_owned();
        self.region = self.region.trim().to_owned();
        self.endpoint = self.endpoint.trim().trim_end_matches('/').to_owned();
        if self.public_key.is_empty() || self.public_key.chars().any(char::is_control) {
            return Err("请填写 UCloud PublicKey".into());
        }
        if !self.endpoint.is_empty()
            && (!(self.endpoint.starts_with("https://") || self.endpoint.starts_with("http://"))
                || self.endpoint.chars().any(char::is_whitespace))
        {
            return Err("Endpoint 必须是以 http:// 或 https:// 开头的地址".into());
        }
        Ok(())
    }

    /// Store the private key in the keyring and publish the non-secret profile.
    /// A failed config write restores the previous keyring value.
    pub fn save(mut self, private_key: Option<&str>) -> std::result::Result<Self, String> {
        self.validate()?;
        let private_key = private_key.map(str::trim).filter(|value| !value.is_empty());
        if private_key.is_none() {
            let previous = Self::load()
                .map_err(|error| error.to_string())?
                .ok_or("请填写 UCloud PrivateKey")?;
            if previous.public_key != self.public_key {
                return Err("更换 PublicKey 时请重新填写 PrivateKey".into());
            }
            if previous.endpoint != self.endpoint {
                return Err("更换 Endpoint 时请重新填写 PrivateKey".into());
            }
            Self::read_secret().map_err(|error| error.to_string())?;
            self.write().map_err(|error| error.to_string())?;
            return Ok(self);
        }
        let private_key = private_key.unwrap();
        if private_key.chars().any(char::is_control) {
            return Err("PrivateKey 不能包含换行或控制字符".into());
        }
        let previous = Self::read_secret().ok();
        entry()
            .and_then(|entry| entry.set_password(private_key))
            .map_err(|_| "UCloud PrivateKey 未能写入系统钥匙串，配置未保存")?;
        if let Err(error) = self.write() {
            match previous {
                Some(previous) => {
                    let _ = entry().and_then(|entry| entry.set_password(&previous));
                }
                None => {
                    let _ = entry().and_then(|entry| entry.delete_credential());
                }
            }
            return Err(format!("云账户配置保存失败，原配置已保留：{error}"));
        }
        Ok(self)
    }

    /// Enable or disable the saved account without touching its key.
    pub fn set_enabled(&self, enabled: bool) -> std::result::Result<(), String> {
        let mut profile = self.clone();
        profile.enabled = enabled;
        profile.write().map_err(|error| error.to_string())
    }

    fn write(&self) -> Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| Error::Credentials(format!("无法创建配置目录：{error}")))?;
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temporary = path.with_extension(format!("tmp-{stamp}"));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|error| Error::Credentials(format!("无法写入云账户配置：{error}")))?;
        let result = (|| -> Result<()> {
            serde_json::to_writer_pretty(&mut file, self)
                .map_err(|error| Error::Credentials(error.to_string()))?;
            file.write_all(b"\n")
                .map_err(|error| Error::Credentials(error.to_string()))?;
            file.sync_all()
                .map_err(|error| Error::Credentials(error.to_string()))?;
            std::fs::rename(&temporary, &path)
                .map_err(|error| Error::Credentials(format!("无法发布云账户配置：{error}")))
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result
    }

    fn read_secret() -> Result<String> {
        entry()
            .and_then(|entry| entry.get_password())
            .map_err(|_| Error::Credentials("钥匙串中没有 UCloud PrivateKey".into()))
    }

    /// Resolve credentials for this profile.
    pub fn credentials(&self) -> Result<Credentials> {
        self.credentials_with(&self.public_key, &self.project_id, &self.region)
    }

    /// Reuse the stored key with form-edited non-secret fields.
    pub fn credentials_with(
        &self,
        public_key: &str,
        project_id: &str,
        region: &str,
    ) -> Result<Credentials> {
        Credentials::new(
            public_key.to_owned(),
            Self::read_secret()?,
            Some(project_id.to_owned()).filter(|value| !value.is_empty()),
            Some(region.to_owned()).filter(|value| !value.is_empty()),
        )
    }

    /// Build a live client from this saved profile.
    pub fn client(&self) -> Result<UcloudClient> {
        UcloudClient::live_with(
            self.credentials()?,
            Some(self.endpoint.clone()).filter(|value| !value.is_empty()),
            Some(self.region.clone()).filter(|value| !value.is_empty()),
        )
    }

    /// Remove the profile and its keyring entry.
    pub fn remove() -> std::result::Result<(), String> {
        let path = Self::path();
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("无法删除云账户配置：{error}")),
        }
        match entry().and_then(|entry| entry.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err("配置已删除，但系统钥匙串中的 PrivateKey 清理失败".into()),
        }
    }

    /// The endpoint the API client should use.
    pub fn endpoint_or_default(&self) -> String {
        if self.endpoint.is_empty() {
            DEFAULT_ENDPOINT.to_owned()
        } else {
            self.endpoint.clone()
        }
    }
}

fn entry() -> keyring::Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, USER)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_json_never_contains_the_private_key() {
        let profile = CloudProfile {
            public_key: "public-key".into(),
            project_id: "org-demo".into(),
            region: "cn-bj2".into(),
            endpoint: "https://api.example.test".into(),
            enabled: true,
        };
        let json = serde_json::to_string(&profile).unwrap();
        assert!(json.contains("public-key"));
        assert!(!json.contains("private"));
        assert!(!json.contains("secret"));
    }
    #[test]
    fn endpoint_validation_accepts_http_and_rejects_whitespace() {
        let mut profile = CloudProfile {
            public_key: "public".into(),
            endpoint: "https://api.ucloud.cn".into(),
            ..Default::default()
        };
        assert!(profile.validate().is_ok());
        profile.endpoint = "https://api.ucloud.cn/v1?key=x".into();
        assert!(profile.validate().is_ok()); // query is allowed for a custom proxy
        profile.endpoint = "ftp://example.test".into();
        assert!(profile.validate().is_err());
        profile.endpoint = "https://example.test/a b".into();
        assert!(profile.validate().is_err());
    }
}
