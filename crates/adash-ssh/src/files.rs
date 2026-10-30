use crate::Connection;
use anyhow::{Context, Result, bail};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

/// One directory entry. `path` is absolute as reported by the server.
#[derive(Debug, Clone)]
pub struct RemoteFile {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub directory: bool,
}

impl Connection {
    pub fn list_files(&self, path: &Path) -> Result<Vec<RemoteFile>> {
        let sftp = self.sftp()?;
        let mut files: Vec<_> = sftp
            .readdir(path)
            .with_context(|| format!("无法列出 {}", path.display()))?
            .into_iter()
            .filter_map(|(path, stat)| {
                let name = path.file_name()?.to_string_lossy().into_owned();
                if name == "." || name == ".." {
                    return None;
                }
                Some(RemoteFile {
                    path,
                    name,
                    size: stat.size.unwrap_or(0),
                    directory: stat.is_dir(),
                })
            })
            .collect();
        files.sort_by(|a, b| b.directory.cmp(&a.directory).then(a.name.cmp(&b.name)));
        Ok(files)
    }

    /// Reads text only; binary files are rejected instead of lossily decoded.
    pub fn read_remote_text(&self, path: &Path) -> Result<String> {
        let sftp = self.sftp()?;
        let file = sftp
            .open(path)
            .with_context(|| format!("无法打开 {}", path.display()))?;
        let mut bytes = Vec::new();
        file.take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .with_context(|| format!("读取 {} 时连接中断", path.display()))?;
        if bytes.len() > 2 * 1024 * 1024 {
            bail!("文件超过 2 MiB，请下载后编辑");
        }
        String::from_utf8(bytes).context("这是二进制文件，请下载后打开")
    }

    /// Writes through a sibling temporary file and renames it, so a failed
    /// transfer never truncates the original. Permissions are preserved when
    /// the server supports SETSTAT.
    pub fn write_remote_text(&self, path: &Path, text: &str) -> Result<()> {
        let sftp = self.sftp()?;
        let temporary = remote_temporary(path);
        let original = sftp.stat(path).ok();
        let result = (|| -> Result<()> {
            let mut file = sftp
                .open_mode(
                    &temporary,
                    ssh2::OpenFlags::WRITE | ssh2::OpenFlags::CREATE | ssh2::OpenFlags::EXCLUSIVE,
                    0o600,
                    ssh2::OpenType::File,
                )
                .context("无法创建远程临时文件")?;
            file.write_all(text.as_bytes())
                .context("写入远程文件失败")?;
            file.close().context("关闭远程临时文件失败")?;
            if let Some(perm) = original.and_then(|stat| stat.perm) {
                let _ = sftp.setstat(
                    &temporary,
                    ssh2::FileStat {
                        perm: Some(perm),
                        size: None,
                        uid: None,
                        gid: None,
                        atime: None,
                        mtime: None,
                    },
                );
            }
            // Plain rename follows SFTP v3; OpenSSH's server replaces an existing
            // destination with rename(2), so the swap stays atomic there. Flagged
            // variants need the posix-rename extension and fail without it.
            // Prefer posix-rename (atomic replace). Servers that reject it get a
            // short unlink+rename window instead of losing the edit entirely.
            if sftp
                .rename(
                    &temporary,
                    path,
                    Some(ssh2::RenameFlags::ATOMIC | ssh2::RenameFlags::OVERWRITE),
                )
                .is_err()
            {
                let _ = sftp.unlink(path);
                sftp.rename(&temporary, path, None)
                    .with_context(|| format!("无法替换 {}", path.display()))?;
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = sftp.unlink(&temporary);
        }
        result
    }

    /// Streams a local file up; an existing remote file is never overwritten.
    pub fn upload(&self, local: &Path, remote: &Path) -> Result<u64> {
        let sftp = self.sftp()?;
        let mut source =
            fs::File::open(local).with_context(|| format!("无法打开 {}", local.display()))?;
        let mut destination = sftp
            .open_mode(
                remote,
                ssh2::OpenFlags::WRITE | ssh2::OpenFlags::CREATE | ssh2::OpenFlags::EXCLUSIVE,
                0o600,
                ssh2::OpenType::File,
            )
            .with_context(|| format!("无法创建 {}（目标已存在？）", remote.display()))?;
        let transferred = std::io::copy(&mut source, &mut destination).context("上传中断")?;
        destination.close().context("关闭远程文件失败")?;
        Ok(transferred)
    }

    /// Streams a remote file down; an existing local file is never overwritten.
    pub fn download(&self, remote: &Path, local: &Path) -> Result<u64> {
        let sftp = self.sftp()?;
        let mut source = sftp
            .open(remote)
            .with_context(|| format!("无法读取 {}", remote.display()))?;
        let mut destination = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(local)
            .with_context(|| format!("无法创建 {}", local.display()))?;
        std::io::copy(&mut source, &mut destination).context("下载中断")
    }

    pub fn mkdir(&self, path: &Path) -> Result<()> {
        self.sftp()?
            .mkdir(path, 0o755)
            .with_context(|| format!("无法创建 {}", path.display()))?;
        Ok(())
    }

    pub fn remove_file(&self, path: &Path) -> Result<()> {
        self.sftp()?
            .unlink(path)
            .with_context(|| format!("无法删除 {}", path.display()))?;
        Ok(())
    }

    pub fn rename_file(&self, from: &Path, to: &Path) -> Result<()> {
        self.sftp()?
            .rename(from, to, None)
            .with_context(|| format!("无法重命名 {}", from.display()))?;
        Ok(())
    }

    fn sftp(&self) -> Result<ssh2::Sftp> {
        self.session.sftp().context("无法打开 SFTP 通道")
    }
}

fn remote_temporary(path: &Path) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    path.with_file_name(format!(
        ".{}.adash-{stamp}",
        path.file_name().unwrap_or_default().to_string_lossy()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporary_sibling_is_hidden_and_unique() {
        let path = Path::new("/srv/app/config.yaml");
        let temporary = remote_temporary(path);
        assert_eq!(temporary.parent(), path.parent());
        assert!(
            temporary
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".config.yaml.adash-")
        );
        assert_ne!(temporary, path);
    }
}
