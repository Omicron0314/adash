//! Real local OpenSSH integration. Run with:
//! `ADASH_SSH_INTEGRATION=1 cargo test -p adash-ssh --test localhost -- --nocapture`
//! Uses an unprivileged loopback sshd and temporary host/user keys. No daemon,
//! user account, trusted host, or cloud resource is modified.
use adash_ssh::*;
use anyhow::{Context, Result, bail};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Server {
    child: Child,
    directory: tempfile::TempDir,
    host: Host,
    key: PathBuf,
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Server {
    fn start() -> Result<Self> {
        let directory = tempfile::tempdir()?;
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))?;
        let key = directory.path().join("identity");
        let host_key = directory.path().join("host-key");
        for path in [&key, &host_key] {
            let status = Command::new("ssh-keygen")
                .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                .arg(path)
                .status()?;
            if !status.success() {
                bail!("ssh-keygen failed");
            }
        }
        fs::copy(
            key.with_extension("pub"),
            directory.path().join("authorized_keys"),
        )?;
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        drop(listener);
        let config = directory.path().join("sshd_config");
        let user = String::from_utf8(Command::new("id").arg("-un").output()?.stdout)?
            .trim()
            .to_owned();
        fs::write(
            &config,
            format!(
                "Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nPidFile {}\nAuthorizedKeysFile {}\nStrictModes no\nUsePAM no\nPasswordAuthentication no\nKbdInteractiveAuthentication no\nPubkeyAuthentication yes\nPermitRootLogin yes\nAllowUsers {user}\nAllowTcpForwarding yes\nSubsystem sftp /usr/lib/ssh/sftp-server\nLogLevel ERROR\n",
                host_key.display(),
                directory.path().join("sshd.pid").display(),
                directory.path().join("authorized_keys").display()
            ),
        )?;
        let child = Command::new("/usr/bin/sshd")
            .args(["-D", "-e", "-f"])
            .arg(&config)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()?;
        let host = Host {
            id: "local-test".into(),
            name: "Local SSH test".into(),
            address: "127.0.0.1".into(),
            port,
            username: user,
            identity: None,
        };
        let server = Self {
            child,
            directory,
            host,
            key,
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return Ok(server);
            }
            thread::sleep(Duration::from_millis(30));
        }
        bail!("temporary sshd did not start")
    }
    fn auth(&self) -> Authentication {
        Authentication::KeyFile {
            path: self.key.clone(),
            passphrase: None,
        }
    }
    fn connect(&self, known: &BTreeMap<String, String>) -> Result<Connection> {
        Connection::open(&self.host, self.auth(), known, Duration::from_secs(5))
    }
}
fn echo_server() -> Result<(u16, thread::JoinHandle<()>)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let thread = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut data = Vec::new();
        stream.read_to_end(&mut data).unwrap();
        stream.write_all(&data).unwrap();
    });
    Ok((port, thread))
}
fn echo_client(port: u16) -> Result<()> {
    let mut client = TcpStream::connect(("127.0.0.1", port))?;
    client.set_read_timeout(Some(Duration::from_secs(5)))?;
    let data = vec![b'x'; 128 * 1024];
    client.write_all(&data)?;
    client.shutdown(std::net::Shutdown::Write)?;
    let mut reply = Vec::new();
    client.read_to_end(&mut reply)?;
    assert_eq!(reply, data);
    Ok(())
}
#[test]
fn real_ssh_terminal_sftp_monitor_and_both_forward_directions() -> Result<()> {
    if std::env::var_os("ADASH_SSH_INTEGRATION").is_none() {
        eprintln!("local sshd integration skipped; set ADASH_SSH_INTEGRATION=1 to run");
        return Ok(());
    }
    let server = Server::start()?;
    let mut known = BTreeMap::new();
    let error = match server.connect(&known) {
        Ok(_) => panic!("unknown host was accepted"),
        Err(error) => error,
    };
    let fingerprint = match error.downcast_ref::<HostKeyError>() {
        Some(HostKeyError::Unknown { fingerprint, .. }) => fingerprint.clone(),
        other => bail!("unexpected handshake result: {other:?}: {error:#}"),
    };
    known.insert(server.host.endpoint(), "SHA256:wrong".into());
    assert!(server.connect(&known).is_err());
    known.insert(server.host.endpoint(), fingerprint);
    let connection = server.connect(&known)?;
    let output = connection.execute("printf 'ssh-ok'; printf ' stderr-ok' >&2; exit 7")?;
    assert_eq!(output.exit_status, 7);
    assert!(output.stdout.contains("ssh-ok"));
    assert!(output.stdout.contains("stderr-ok"));
    assert!(connection.execute("head -c 5000000 /dev/zero").is_err());
    assert_eq!(
        connection.execute("printf 'still-connected'")?.stdout,
        "still-connected"
    );
    let remote = server.directory.path().join("remote");
    connection.mkdir(&remote)?;
    let local = server.directory.path().join("source.txt");
    fs::write(&local, "short")?;
    let remote_file = remote.join("file.txt");
    connection.upload(&local, &remote_file)?;
    assert!(
        connection.upload(&local, &remote_file).is_err(),
        "upload must not overwrite"
    );
    assert_eq!(connection.read_remote_text(&remote_file)?, "short");
    let fresh = remote.join("fresh.txt");
    connection
        .write_remote_text(&fresh, "brand new\n")
        .map_err(|e| anyhow::anyhow!("write fresh: {e:#}"))?;
    assert_eq!(connection.read_remote_text(&fresh)?, "brand new\n");
    let longer = "a much longer edited remote file\n";
    connection
        .write_remote_text(&remote_file, longer)
        .map_err(|e| anyhow::anyhow!("write_remote_text: {e:#}"))?;
    assert_eq!(connection.read_remote_text(&remote_file)?, longer);
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&remote_file, fs::Permissions::from_mode(0o640))?;
    let symlink = remote.join("linked.txt");
    std::os::unix::fs::symlink(&remote_file, &symlink)?;
    connection.write_remote_text(&symlink, longer)?;
    assert!(fs::symlink_metadata(&symlink)?.file_type().is_symlink());
    assert_eq!(
        fs::metadata(&remote_file)?.permissions().mode() & 0o777,
        0o640
    );
    let binary = remote.join("binary");
    fs::write(&binary, [0, 1, 2])?;
    assert!(connection.read_remote_text(&binary).is_err());
    let large = server.directory.path().join("large.bin");
    fs::write(&large, vec![7u8; 200_000])?;
    let cancelled = remote.join("cancelled.bin");
    assert!(
        connection
            .upload_with_progress(&large, &cancelled, |done, _| {
                if done >= 65536 {
                    bail!("cancelled");
                }
                Ok(())
            })
            .is_err()
    );
    assert!(!cancelled.exists());
    assert!(
        connection
            .list_files(&remote)?
            .iter()
            .all(|file| !file.name.contains(".adash-"))
    );
    assert!(
        connection
            .list_files(&remote)?
            .iter()
            .any(|file| file.name == "file.txt")
    );
    let download = server.directory.path().join("download.txt");
    connection.download(&remote_file, &download)?;
    assert_eq!(fs::read_to_string(&download)?, longer);
    assert!(connection.download(&remote_file, &download).is_err());
    assert_eq!(fs::read_to_string(&download)?, longer);
    let cancelled_download = server.directory.path().join("cancelled-download");
    assert!(
        connection
            .download_with_progress(&remote_file, &cancelled_download, |_, _| bail!("cancelled"))
            .is_err()
    );
    assert!(!cancelled_download.exists());
    let empty_directory = remote.join("empty");
    connection.mkdir(&empty_directory)?;
    connection.remove_file(&empty_directory)?;
    assert!(!empty_directory.exists());
    assert!(connection.rename_file(&remote_file, &fresh).is_err());
    assert_eq!(connection.read_remote_text(&fresh)?, "brand new\n");
    let renamed = remote.join("renamed.txt");
    connection.rename_file(&remote_file, &renamed)?;
    connection.remove_file(&renamed)?;
    let remaining = connection.list_files(&remote)?;
    assert!(
        !remaining
            .iter()
            .any(|file| file.name == "renamed.txt" || file.name == "file.txt")
    );
    assert!(remaining.iter().any(|file| file.name == "fresh.txt"));
    let sample = connection.monitor()?;
    assert!(sample.memory_total > 0);
    assert!(sample.cpu_total > 0);
    assert!(sample.uptime_seconds > 0.);
    let terminal = Terminal::start(server.connect(&known)?, 24, 100)?;
    terminal.input(b"printf '\\nPTY_OK\\n'; stty size\r".to_vec());
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && !terminal.contents().contains("24 100") {
        thread::sleep(Duration::from_millis(25));
    }
    assert!(terminal.contents().contains("PTY_OK"));
    assert!(terminal.contents().contains("24 100"));
    terminal.resize(30, 110);
    terminal.input(b"stty size\r".to_vec());
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && !terminal.contents().contains("30 110") {
        thread::sleep(Duration::from_millis(25));
    }
    assert!(terminal.contents().contains("30 110"));
    terminal.close();
    for direction in [ForwardDirection::Local, ForwardDirection::Remote] {
        let (target_port, echo_thread) = echo_server()?;
        let rule = ForwardRule {
            id: "test".into(),
            name: "test".into(),
            host_id: server.host.id.clone(),
            direction,
            bind_port: 0,
            target_host: "127.0.0.1".into(),
            target_port,
        };
        let forward = ForwardHandle::start(server.connect(&known)?, rule)?;
        echo_client(forward.bound_port).with_context(|| format!("{direction:?} forward"))?;
        echo_thread.join().unwrap();
        forward.stop();
    }
    Ok(())
}
