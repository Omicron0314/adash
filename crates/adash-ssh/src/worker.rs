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
    Close,
}
#[derive(Debug)]
pub enum Reply {
    Connected,
    Files(Vec<RemoteFile>),
    Monitor(MonitorSample),
    Exec(ExecResult),
    Opened(PathBuf, String),
    Done,
    Error(String),
    Closed,
    HostKeyUnknown(String),
    HostKeyChanged { expected: String, actual: String },
}
/// One SSH connection, serviced sequentially on its own thread. A session never
/// shares `ssh2::Session` across threads.
pub struct Worker {
    pub requests: mpsc::Sender<Request>,
    pub replies: mpsc::Receiver<Reply>,
    pub alive: Arc<AtomicBool>,
}
impl Worker {
    pub fn spawn(
        host: crate::Host,
        auth: crate::Authentication,
        known: BTreeMap<String, String>,
        timeout: Duration,
    ) -> Self {
        let (requests, request_rx) = mpsc::channel();
        let (reply_tx, replies) = mpsc::channel();
        let alive = Arc::new(AtomicBool::new(true));
        let worker_alive = alive.clone();
        thread::Builder::new()
            .name("adash-ssh-worker".into())
            .spawn(move || {
                struct Guard(Arc<AtomicBool>);
                impl Drop for Guard {
                    fn drop(&mut self) {
                        self.0.store(false, Ordering::Release);
                    }
                }
                let _guard = Guard(worker_alive);
                let connection = match Connection::open(&host, auth, &known, timeout) {
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
                let _ = reply_tx.send(Reply::Connected);
                while let Ok(request) = request_rx.recv() {
                    let reply = match request {
                        Request::List(path) => connection.list_files(&path).map(Reply::Files),
                        Request::Monitor => connection.monitor().map(Reply::Monitor),
                        Request::Exec(command) => connection.execute(&command).map(Reply::Exec),
                        Request::Open(path) => connection
                            .read_remote_text(&path)
                            .map(|text| Reply::Opened(path.clone(), text)),
                        Request::Upload(local, remote) => {
                            connection.upload(&local, &remote).map(|_| Reply::Done)
                        }
                        Request::Download(remote, local) => {
                            connection.download(&remote, &local).map(|_| Reply::Done)
                        }
                        Request::Write(path, text) => connection
                            .write_remote_text(&path, &text)
                            .map(|()| Reply::Done),
                        Request::Mkdir(path) => connection.mkdir(&path).map(|()| Reply::Done),
                        Request::Remove(path) => {
                            connection.remove_file(&path).map(|()| Reply::Done)
                        }
                        Request::Close => {
                            let _ = reply_tx.send(Reply::Closed);
                            break;
                        }
                    };
                    if reply_tx
                        .send(reply.unwrap_or_else(|error| Reply::Error(format!("{error:#}"))))
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
        }
    }
    pub fn send(&self, request: Request) -> bool {
        self.requests.send(request).is_ok()
    }
}
