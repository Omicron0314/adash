use crate::{Connection, ExecResult, MonitorSample, RemoteFile};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

#[derive(Debug)]
pub enum Request {
    List(PathBuf),
    Monitor,
    Exec(String),
    Open(PathBuf),
    Upload(PathBuf, PathBuf),
    Download(PathBuf, PathBuf),
    Write(PathBuf, String),
    Mkdir(PathBuf),
    Remove(PathBuf),
    Rename(PathBuf, PathBuf),
    Close,
}
#[derive(Debug, Clone, Copy)]
pub enum Operation {
    Files,
    Monitor,
    Exec,
    Transfer,
}
#[derive(Debug)]
pub enum Reply {
    Connected,
    Files {
        path: PathBuf,
        entries: Vec<RemoteFile>,
    },
    Monitor(MonitorSample),
    Exec(ExecResult),
    Opened(PathBuf, String),
    Saved(PathBuf, String),
    Renamed(PathBuf, PathBuf),
    Done,
    Error(String),
    OperationError(Operation, String),
    TransferProgress {
        done: u64,
        total: u64,
    },
    Closed,
    HostKeyUnknown(String),
    HostKeyChanged {
        expected: String,
        actual: String,
    },
}
/// One SSH connection, serviced sequentially on its own thread. A session never
/// shares `ssh2::Session` across threads.
pub struct Worker {
    pub requests: mpsc::Sender<Request>,
    pub replies: mpsc::Receiver<Reply>,
    pub alive: Arc<AtomicBool>,
    cancel_transfer: Arc<AtomicBool>,
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Release);
        self.cancel_transfer.store(true, Ordering::Release);
        let _ = self.requests.send(Request::Close);
    }
}
impl Worker {
    pub fn spawn(
        host: crate::Host,
        identity: Option<crate::Identity>,
        known: BTreeMap<String, String>,
        timeout: Duration,
    ) -> Self {
        let (requests, request_rx) = mpsc::channel();
        let (reply_tx, replies) = mpsc::channel();
        let alive = Arc::new(AtomicBool::new(true));
        let worker_alive = alive.clone();
        let cancel_transfer = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel_transfer.clone();
        thread::Builder::new()
            .name("adash-ssh-worker".into())
            .spawn(move || {
                struct Guard(Arc<AtomicBool>);
                impl Drop for Guard {
                    fn drop(&mut self) {
                        self.0.store(false, Ordering::Release);
                    }
                }
                let _guard = Guard(worker_alive.clone());
                let connection = match crate::Authentication::from_identity(identity.as_ref())
                    .and_then(|auth| Connection::open(&host, auth, &known, timeout))
                {
                    Ok(connection) => connection,
                    Err(error) => {
                        let reply = match error.downcast_ref::<crate::HostKeyError>() {
                            Some(crate::HostKeyError::Unknown { fingerprint, .. }) => {
                                Reply::HostKeyUnknown(fingerprint.clone())
                            }
                            Some(crate::HostKeyError::Changed {
                                expected, actual, ..
                            }) => Reply::HostKeyChanged {
                                expected: expected.clone(),
                                actual: actual.clone(),
                            },
                            None => Reply::Error(format!("{error:#}")),
                        };
                        let _ = reply_tx.send(reply);
                        return;
                    }
                };
                if !worker_alive.load(Ordering::Acquire) || reply_tx.send(Reply::Connected).is_err()
                {
                    return;
                }
                while let Ok(request) = request_rx.recv() {
                    if !worker_alive.load(Ordering::Acquire) {
                        break;
                    }
                    let operation = match &request {
                        Request::Monitor => Operation::Monitor,
                        Request::Exec(_) => Operation::Exec,
                        Request::Upload(..) | Request::Download(..) => Operation::Transfer,
                        _ => Operation::Files,
                    };
                    let mut progress = |done, total| -> anyhow::Result<()> {
                        if worker_cancel.load(Ordering::Acquire) {
                            anyhow::bail!("传输已取消");
                        }
                        reply_tx
                            .send(Reply::TransferProgress { done, total })
                            .map_err(|_| anyhow::anyhow!("会话已关闭"))
                    };
                    let reply = match request {
                        Request::List(path) => {
                            connection.canonical_remote(&path).and_then(|path| {
                                connection
                                    .list_files(&path)
                                    .map(|entries| Reply::Files { path, entries })
                            })
                        }
                        Request::Monitor => connection.monitor().map(Reply::Monitor),
                        Request::Exec(command) => connection.execute(&command).map(Reply::Exec),
                        Request::Open(path) => connection
                            .read_remote_text(&path)
                            .map(|text| Reply::Opened(path.clone(), text)),
                        Request::Upload(local, remote) => connection
                            .upload_with_progress(&local, &remote, &mut progress)
                            .map(|_| Reply::Done),
                        Request::Download(remote, local) => connection
                            .download_with_progress(&remote, &local, &mut progress)
                            .map(|_| Reply::Done),
                        Request::Write(path, text) => connection
                            .write_remote_text(&path, &text)
                            .map(|()| Reply::Saved(path, text)),
                        Request::Mkdir(path) => connection.mkdir(&path).map(|()| Reply::Done),
                        Request::Remove(path) => {
                            connection.remove_file(&path).map(|()| Reply::Done)
                        }
                        Request::Rename(from, to) => connection
                            .rename_file(&from, &to)
                            .map(|()| Reply::Renamed(from, to)),
                        Request::Close => {
                            let _ = reply_tx.send(Reply::Closed);
                            break;
                        }
                    };
                    if reply_tx
                        .send(reply.unwrap_or_else(|error| {
                            Reply::OperationError(operation, format!("{error:#}"))
                        }))
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .expect("failed to spawn SSH worker");
        Self {
            requests,
            replies,
            alive,
            cancel_transfer,
        }
    }
    pub fn send(&self, request: Request) -> bool {
        if matches!(request, Request::Upload(..) | Request::Download(..)) {
            self.cancel_transfer.store(false, Ordering::Release);
        }
        self.requests.send(request).is_ok()
    }
    pub fn cancel_transfer(&self) {
        self.cancel_transfer.store(true, Ordering::Release);
    }
}
