use crate::Connection;
use anyhow::{Context, Result, bail};
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct RemoteFile {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub directory: bool,
}

impl Connection {
    pub fn canonical_remote(&self, path: &Path) -> Result<PathBuf> {
        self.sftp()?
            .realpath(path)
            .with_context(|| format!("无法访问 {}", path.display()))
    }
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
    pub fn read_remote_text(&self, path: &Path) -> Result<String> {
        let sftp = self.sftp()?;
        let mut bytes = Vec::new();
        sftp.open(path)
            .with_context(|| format!("无法打开 {}", path.display()))?
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 2 * 1024 * 1024 {
            bail!("文件超过 2 MiB，请下载后编辑");
        }
        if bytes.contains(&0) {
            bail!("这是二进制文件，请下载后打开");
        }
        String::from_utf8(bytes).context("文件不是 UTF-8 文本，请下载后打开")
    }
    /// Follow symlinks, stage the complete contents, then replace. SFTP v3
    /// servers without atomic overwrite retain a backup until replacement has
    /// succeeded. A failed replacement restores it instead of unlinking data.
    pub fn write_remote_text(&self, path: &Path, text: &str) -> Result<()> {
        let sftp = self.sftp()?;
        let original = stat_if_present(&sftp, path)?;
        let destination = if original.is_some() {
            sftp.realpath(path)?
        } else {
            path.to_path_buf()
        };
        let temporary = remote_temporary(&destination);
        let result = (|| -> Result<()> {
            let mut file = sftp.open_mode(
                &temporary,
                ssh2::OpenFlags::WRITE | ssh2::OpenFlags::CREATE | ssh2::OpenFlags::EXCLUSIVE,
                0o600,
                ssh2::OpenType::File,
            )?;
            file.write_all(text.as_bytes())?;
            file.close()?;
            if original.is_some() {
                let stat = sftp
                    .stat(&destination)
                    .context("无法读取原文件属性，已取消保存")?;
                sftp.setstat(
                    &temporary,
                    ssh2::FileStat {
                        perm: stat.perm,
                        size: None,
                        uid: None,
                        gid: None,
                        atime: None,
                        mtime: None,
                    },
                )
                .context("无法保留原文件权限，已取消保存")?;
            }
            if original.is_none() {
                sftp.rename(&temporary, &destination, Some(ssh2::RenameFlags::empty()))?;
            } else if sftp
                .rename(
                    &temporary,
                    &destination,
                    Some(
                        ssh2::RenameFlags::OVERWRITE
                            | ssh2::RenameFlags::ATOMIC
                            | ssh2::RenameFlags::NATIVE,
                    ),
                )
                .is_err()
            {
                install_with_backup(
                    &temporary,
                    &destination,
                    |from, to| {
                        sftp.rename(from, to, Some(ssh2::RenameFlags::empty()))?;
                        Ok(())
                    },
                    |backup| {
                        sftp.unlink(backup)?;
                        Ok(())
                    },
                )?;
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = sftp.unlink(&temporary);
        }
        result
    }
    pub fn upload(&self, local: &Path, remote: &Path) -> Result<u64> {
        self.upload_with_progress(local, remote, |_, _| Ok(()))
    }
    pub fn upload_with_progress(
        &self,
        local: &Path,
        remote: &Path,
        mut progress: impl FnMut(u64, u64) -> Result<()>,
    ) -> Result<u64> {
        let sftp = self.sftp()?;
        if stat_if_present(&sftp, remote)?.is_some() {
            bail!("目标 {} 已存在，未覆盖", remote.display());
        }
        let mut source = fs::File::open(local)?;
        let total = source.metadata()?.len();
        let temporary = remote_temporary(remote);
        let result = (|| -> Result<u64> {
            let mut destination = sftp.open_mode(
                &temporary,
                ssh2::OpenFlags::WRITE | ssh2::OpenFlags::CREATE | ssh2::OpenFlags::EXCLUSIVE,
                0o600,
                ssh2::OpenType::File,
            )?;
            let transferred = copy_progress(&mut source, &mut destination, total, &mut progress)?;
            destination.close()?;
            sftp.rename(&temporary, remote, Some(ssh2::RenameFlags::empty()))?;
            Ok(transferred)
        })();
        if result.is_err() {
            let _ = sftp.unlink(&temporary);
        }
        result
    }
    pub fn download(&self, remote: &Path, local: &Path) -> Result<u64> {
        self.download_with_progress(remote, local, |_, _| Ok(()))
    }
    pub fn download_with_progress(
        &self,
        remote: &Path,
        local: &Path,
        mut progress: impl FnMut(u64, u64) -> Result<()>,
    ) -> Result<u64> {
        let sftp = self.sftp()?;
        let mut source = sftp.open(remote)?;
        let total = source.stat()?.size.unwrap_or(0);
        // Stage in the same directory, then hard-link with no overwrite. A
        // failed or cancelled download neither truncates a file nor leaves a
        // partial destination that looks complete.
        let temporary = remote_temporary(local);
        let result = (|| -> Result<u64> {
            let mut destination = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)?;
            let transferred = copy_progress(&mut source, &mut destination, total, &mut progress)?;
            destination.sync_all()?;
            fs::hard_link(&temporary, local)
                .with_context(|| format!("无法创建 {}（目标已存在？）", local.display()))?;
            Ok(transferred)
        })();
        let _ = fs::remove_file(&temporary);
        result
    }
    pub fn mkdir(&self, path: &Path) -> Result<()> {
        self.sftp()?
            .mkdir(path, 0o755)
            .with_context(|| format!("无法创建 {}", path.display()))?;
        Ok(())
    }
    pub fn remove_file(&self, path: &Path) -> Result<()> {
        let sftp = self.sftp()?;
        if sftp.lstat(path)?.is_dir() {
            sftp.rmdir(path)
        } else {
            sftp.unlink(path)
        }
        .with_context(|| format!("无法删除 {}（目录必须为空）", path.display()))?;
        Ok(())
    }
    pub fn rename_file(&self, from: &Path, to: &Path) -> Result<()> {
        self.sftp()?
            .rename(from, to, Some(ssh2::RenameFlags::empty()))
            .with_context(|| format!("无法重命名 {}，目标可能已存在", from.display()))?;
        Ok(())
    }
    fn sftp(&self) -> Result<ssh2::Sftp> {
        self.session.sftp().context("无法打开 SFTP 通道")
    }
}
fn install_with_backup(
    temporary: &Path,
    destination: &Path,
    mut rename: impl FnMut(&Path, &Path) -> Result<()>,
    cleanup: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    let backup = remote_temporary(destination);
    rename(destination, &backup).context("无法暂存原文件，已取消保存")?;
    if let Err(error) = rename(temporary, destination) {
        if rename(&backup, destination).is_err() {
            bail!("保存失败，原文件保留在 {}：{error}", backup.display());
        }
        return Err(error).context("保存失败，已恢复原文件");
    }
    // The new contents are already durable; a hidden backup is safer than
    // reporting a failed save (which would leave the editor dirty) if the
    // server refuses to remove it.
    let _ = cleanup(&backup);
    Ok(())
}
fn stat_if_present(sftp: &ssh2::Sftp, path: &Path) -> Result<Option<ssh2::FileStat>> {
    match sftp.lstat(path) {
        Ok(stat) => Ok(Some(stat)),
        Err(error) if error.code() == ssh2::ErrorCode::SFTP(2) => Ok(None),
        Err(error) => Err(error).with_context(|| format!("无法检查 {}", path.display())),
    }
}
fn copy_progress(
    source: &mut impl Read,
    destination: &mut impl Write,
    total: u64,
    progress: &mut impl FnMut(u64, u64) -> Result<()>,
) -> Result<u64> {
    let mut buffer = [0u8; 65536];
    let mut transferred = 0;
    progress(0, total)?;
    loop {
        let count = source.read(&mut buffer)?;
        if count == 0 {
            return Ok(transferred);
        }
        destination.write_all(&buffer[..count])?;
        transferred += count as u64;
        progress(transferred, total)?;
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
    #[test]
    fn failed_replacement_restores_the_original_or_retains_a_recoverable_backup() {
        for fail_restore in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let original = directory.path().join("config");
            let candidate = directory.path().join("candidate");
            fs::write(&original, "original").unwrap();
            fs::write(&candidate, "new").unwrap();
            let mut calls = 0;
            let error = install_with_backup(
                &candidate,
                &original,
                |from, to| {
                    calls += 1;
                    if calls == 2 || fail_restore && calls == 3 {
                        bail!("injected rename failure");
                    }
                    fs::rename(from, to)?;
                    Ok(())
                },
                |backup| {
                    fs::remove_file(backup)?;
                    Ok(())
                },
            )
            .unwrap_err();
            if fail_restore {
                let backup = fs::read_dir(directory.path())
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .find(|path| {
                        path.file_name()
                            .unwrap()
                            .to_string_lossy()
                            .starts_with(".config.adash-")
                    })
                    .unwrap();
                assert_eq!(fs::read_to_string(&backup).unwrap(), "original");
                assert!(error.to_string().contains(backup.to_str().unwrap()));
            } else {
                assert_eq!(fs::read_to_string(&original).unwrap(), "original");
            }
            assert_eq!(fs::read_to_string(&candidate).unwrap(), "new");
        }
    }
    #[test]
    fn a_saved_file_is_not_reported_as_failed_when_backup_cleanup_fails() {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("config");
        let candidate = directory.path().join("candidate");
        fs::write(&original, "original").unwrap();
        fs::write(&candidate, "new").unwrap();
        install_with_backup(
            &candidate,
            &original,
            |from, to| {
                fs::rename(from, to)?;
                Ok(())
            },
            |_| bail!("permission denied"),
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&original).unwrap(), "new");
        assert!(!candidate.exists());
    }
    #[test]
    fn cancellation_stops_a_streamed_transfer() {
        let mut source = std::io::Cursor::new(vec![0; 200_000]);
        let mut destination = Vec::new();
        let result = copy_progress(&mut source, &mut destination, 200_000, &mut |done, _| {
            if done >= 65536 {
                bail!("cancelled");
            }
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(destination.len(), 65536);
    }
}
