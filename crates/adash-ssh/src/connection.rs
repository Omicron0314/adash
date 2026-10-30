use crate::{Host, Identity, IdentityKind, load_secret};
use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD_NO_PAD};
use sha2::{Digest, Sha256};
use ssh2::Session;
use std::{
    collections::BTreeMap,
    io::Read,
    net::{TcpStream, ToSocketAddrs},
    path::PathBuf,
    time::Duration,
};
use zeroize::Zeroizing;

#[derive(thiserror::Error, Debug, Clone)]
pub enum HostKeyError {
    #[error("首次连接 {}，请核对主机指纹 {}", .endpoint, .fingerprint)]
    Unknown {
        endpoint: String,
        fingerprint: String,
    },
    #[error("主机 {} 的指纹已变更，连接已拒绝。原指纹 {}；当前 {}", .endpoint, .expected, .actual)]
    Changed {
        endpoint: String,
        expected: String,
        actual: String,
    },
}
pub enum Authentication {
    Agent,
    Password(Zeroizing<String>),
    KeyFile {
        path: PathBuf,
        passphrase: Option<Zeroizing<String>>,
    },
}
impl std::fmt::Debug for Authentication {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Agent => "Agent",
            Self::Password(_) => "Password([redacted])",
            Self::KeyFile { .. } => "KeyFile([redacted])",
        })
    }
}
impl Authentication {
    pub fn from_identity(identity: Option<&Identity>) -> Result<Self> {
        let Some(identity) = identity else {
            return Ok(Self::Agent);
        };
        Ok(match identity.kind {
            IdentityKind::Agent => Self::Agent,
            IdentityKind::Password => {
                Self::Password(load_secret(&identity.id)?.context("钥匙串中没有该身份的密码")?)
            }
            IdentityKind::PrivateKey => Self::KeyFile {
                path: identity.key_path.clone().context("没有私钥文件")?,
                passphrase: load_secret(&identity.id)?,
            },
        })
    }
}
#[derive(Debug, Clone)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_status: i32,
}
/// Owns an authenticated transport. Construct only on a worker thread.
pub struct Connection {
    pub(crate) session: Session,
}
impl Connection {
    pub fn open(
        host: &Host,
        auth: Authentication,
        known: &BTreeMap<String, String>,
        timeout: Duration,
    ) -> Result<Self> {
        host.validate()?;
        let mut last = None;
        let mut socket = None;
        for address in (host.address.as_str(), host.port).to_socket_addrs()? {
            match TcpStream::connect_timeout(&address, timeout) {
                Ok(stream) => {
                    socket = Some(stream);
                    break;
                }
                Err(error) => last = Some(error),
            }
        }
        let stream = socket.context(last.map_or_else(
            || "主机没有可用地址".to_owned(),
            |error| format!("无法连接主机：{error}"),
        ))?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        let mut session = Session::new()?;
        session.set_tcp_stream(stream);
        session.set_timeout(timeout.as_millis().min(u32::MAX as u128) as u32);
        session.handshake().context("SSH 握手失败")?;
        let key = session.host_key().context("服务器没有提供主机公钥")?.0;
        let fingerprint = format!("SHA256:{}", STANDARD_NO_PAD.encode(Sha256::digest(key)));
        verify_host_key(&host.endpoint(), &fingerprint, known)?;
        // Host identity is checked before sending any authentication material.
        match auth {
            Authentication::Agent => {
                let mut agent = session.agent().context("无法访问 SSH agent")?;
                agent
                    .connect()
                    .context("SSH agent 不可用，请选择密码或私钥身份")?;
                agent.list_identities()?;
                let mut accepted = false;
                for identity in agent.identities()? {
                    if agent.userauth(&host.username, &identity).is_ok() {
                        accepted = true;
                        break;
                    }
                }
                if !accepted {
                    bail!("SSH agent 中没有可用身份");
                }
            }
            Authentication::Password(password) => session
                .userauth_password(&host.username, &password)
                .map_err(|_| anyhow::anyhow!("SSH 密码认证失败"))?,
            Authentication::KeyFile { path, passphrase } => session
                .userauth_pubkey_file(
                    &host.username,
                    None,
                    &path,
                    passphrase.as_deref().map(|value| value.as_str()),
                )
                .map_err(|_| anyhow::anyhow!("SSH 私钥认证失败，请检查文件与口令"))?,
        }
        if !session.authenticated() {
            bail!("SSH 认证未完成");
        }
        Ok(Self { session })
    }
    /// Bounded output prevents a remote command from exhausting the desktop.
    pub fn execute(&self, command: &str) -> Result<ExecResult> {
        let mut channel = self.session.channel_session()?;
        channel.exec(command)?;
        // Extended data is merged while reading to avoid stderr pipe deadlocks.
        channel.handle_extended_data(ssh2::ExtendedData::Merge)?;
        let mut output = Vec::new();
        channel
            .by_ref()
            .take(4 * 1024 * 1024)
            .read_to_end(&mut output)?;
        channel.close()?;
        channel.wait_close()?;
        Ok(ExecResult {
            stdout: String::from_utf8_lossy(&output).into_owned(),
            stderr: String::new(),
            exit_status: channel.exit_status()?,
        })
    }
}
pub fn verify_host_key(
    endpoint: &str,
    fingerprint: &str,
    known: &BTreeMap<String, String>,
) -> Result<()> {
    match known.get(endpoint) {
        None => Err(HostKeyError::Unknown {
            endpoint: endpoint.into(),
            fingerprint: fingerprint.into(),
        }
        .into()),
        Some(expected) if expected != fingerprint => Err(HostKeyError::Changed {
            endpoint: endpoint.into(),
            expected: expected.clone(),
            actual: fingerprint.into(),
        }
        .into()),
        Some(_) => Ok(()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changed_keys_cannot_authenticate_and_unknown_keys_require_consent() {
        let mut known = BTreeMap::new();
        assert!(matches!(
            verify_host_key("host", "first", &known)
                .unwrap_err()
                .downcast_ref::<HostKeyError>(),
            Some(HostKeyError::Unknown { .. })
        ));
        known.insert("host".into(), "first".into());
        assert!(verify_host_key("host", "first", &known).is_ok());
        assert!(matches!(
            verify_host_key("host", "changed", &known)
                .unwrap_err()
                .downcast_ref::<HostKeyError>(),
            Some(HostKeyError::Changed { .. })
        ));
        assert_eq!(known["host"], "first");
    }
    #[test]
    fn credentials_debug_is_redacted() {
        assert!(
            !format!(
                "{:?}",
                Authentication::Password(Zeroizing::new("private-test-password".into()))
            )
            .contains("private-test-password")
        );
    }
}
