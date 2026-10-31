use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Host {
    pub id: String,
    pub name: String,
    pub address: String,
    pub port: u16,
    pub username: String,
    pub identity: Option<String>,
}
impl Host {
    pub fn endpoint(&self) -> String {
        format!("[{}]:{}", self.address, self.port)
    }
    pub fn validate(&self) -> Result<()> {
        if self.address.trim().is_empty() || self.username.trim().is_empty() || self.port == 0 {
            bail!("主机地址、用户名和端口不能为空");
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum IdentityKind {
    Agent,
    Password,
    PrivateKey,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub id: String,
    pub name: String,
    pub kind: IdentityKind,
    pub key_path: Option<PathBuf>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snippet {
    pub id: String,
    pub name: String,
    pub command: String,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ForwardDirection {
    Local,
    Remote,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardRule {
    pub id: String,
    pub name: String,
    pub host_id: String,
    pub direction: ForwardDirection,
    pub bind_port: u16,
    pub target_host: String,
    pub target_port: u16,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Profiles {
    pub hosts: BTreeMap<String, Host>,
    pub identities: BTreeMap<String, Identity>,
    pub snippets: BTreeMap<String, Snippet>,
    pub forwards: BTreeMap<String, ForwardRule>,
    pub known_hosts: BTreeMap<String, String>,
    pub recent: Vec<String>,
}
#[derive(Debug)]
pub struct ProfileStore {
    pub data: Profiles,
    path: PathBuf,
    writable: bool,
}
impl ProfileStore {
    pub fn open(path: PathBuf) -> Result<Self> {
        let data = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).context("主机配置损坏；原文件已保留")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Profiles::default(),
            Err(error) => return Err(error.into()),
        };
        Ok(Self {
            data,
            path,
            writable: true,
        })
    }
    /// Preserve an unreadable/corrupt file and make failed saves explicit.
    pub fn unavailable(path: PathBuf) -> Self {
        Self {
            data: Profiles::default(),
            path,
            writable: false,
        }
    }
    pub fn save(&self) -> Result<()> {
        if !self.writable {
            bail!("主机配置未能读取，已禁止覆盖原文件");
        }
        save_private_json(&self.path, &self.data)
    }
    /// Publish a profile edit only if it has been persisted.
    pub fn update(&mut self, edit: impl FnOnce(&mut Profiles)) -> Result<()> {
        let previous = self.data.clone();
        edit(&mut self.data);
        if let Err(error) = self.save() {
            self.data = previous;
            return Err(error);
        }
        Ok(())
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn remember(&mut self, id: &str, limit: usize) {
        self.data.recent.retain(|previous| previous != id);
        self.data.recent.insert(0, id.to_owned());
        self.data.recent.truncate(limit);
    }
}
/// Create the temporary file privately before writing any content; rename on success.
pub fn save_private_json(path: &Path, data: &impl Serialize) -> Result<()> {
    let parent = path.parent().context("没有配置目录")?;
    fs::create_dir_all(parent)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let temporary = path.with_extension(format!("tmp-{}-{stamp}", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)?;
    let result = (|| -> Result<()> {
        file.write_all(&serde_json::to_vec_pretty(data)?)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
/// Passwords and key passphrases never enter profile JSON or Debug output.
pub fn store_secret(id: &str, secret: &str) -> Result<()> {
    keyring::Entry::new("adash", id)?
        .set_password(secret)
        .context("无法保存到系统钥匙串")
}
pub fn load_secret(id: &str) -> Result<Option<zeroize::Zeroizing<String>>> {
    match keyring::Entry::new("adash", id)?.get_password() {
        Ok(secret) => Ok(Some(zeroize::Zeroizing::new(secret))),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(anyhow::anyhow!("无法读取系统钥匙串：{error}")),
    }
}
pub fn delete_secret(id: &str) -> Result<()> {
    match keyring::Entry::new("adash", id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn profiles_are_private_and_corruption_is_not_discarded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hosts.json");
        let mut store = ProfileStore::open(path.clone()).unwrap();
        store.data.snippets.insert(
            "a".into(),
            Snippet {
                id: "a".into(),
                name: "uptime".into(),
                command: "uptime".into(),
            },
        );
        store.save().unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            ProfileStore::open(path.clone())
                .unwrap()
                .data
                .snippets
                .len(),
            1
        );
        fs::write(&path, "broken").unwrap();
        assert!(ProfileStore::open(path.clone()).is_err());
        let mut unavailable = ProfileStore::unavailable(path.clone());
        assert!(
            unavailable
                .update(|profiles| {
                    profiles
                        .known_hosts
                        .insert("unexpected".into(), "fingerprint".into());
                })
                .is_err()
        );
        assert!(unavailable.data.known_hosts.is_empty());
        assert_eq!(fs::read_to_string(path).unwrap(), "broken");
    }
}
