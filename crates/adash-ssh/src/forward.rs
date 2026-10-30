use crate::{Connection, ForwardDirection, ForwardRule};
use anyhow::{Context, Result, bail};
use ssh2::Channel;
use std::{
    collections::VecDeque,
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream, ToSocketAddrs},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

pub struct ForwardHandle {
    stop: Arc<AtomicBool>,
    pub status: Arc<Mutex<String>>,
    pub bound_port: u16,
}
impl ForwardHandle {
    /// Both directions bind loopback; exposing ports to a LAN is deliberately not implicit.
    pub fn start(connection: Connection, rule: ForwardRule) -> Result<Self> {
        if rule.target_port == 0 || rule.target_host.trim().is_empty() {
            bail!("转发目标地址或端口无效");
        }
        let (listener, bound_port) = match rule.direction {
            ForwardDirection::Local => {
                let listener =
                    TcpListener::bind(("127.0.0.1", rule.bind_port)).context("无法监听本地端口")?;
                listener.set_nonblocking(true)?;
                let port = listener.local_addr()?.port();
                (Listener::Local(listener), port)
            }
            ForwardDirection::Remote => {
                let (listener, port) = connection
                    .session
                    .channel_forward_listen(rule.bind_port, Some("127.0.0.1"), Some(32))
                    .context("服务器拒绝远程端口转发")?;
                (Listener::Remote(listener), port)
            }
        };
        connection.session.set_blocking(false);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let status = Arc::new(Mutex::new(format!("监听 127.0.0.1:{bound_port}")));
        let worker_status = status.clone();
        thread::Builder::new()
            .name("adash-port-forward".into())
            .spawn(move || {
                let mut relays = Vec::<Relay>::new();
                let mut listener = listener;
                while !worker_stop.load(Ordering::Acquire) {
                    if relays.len() < 128 {
                        match &mut listener {
                            Listener::Local(listener) => {
                                if let Ok((stream, _)) = listener.accept() {
                                    connection.session.set_blocking(true);
                                    let channel = connection.session.channel_direct_tcpip(
                                        &rule.target_host,
                                        rule.target_port,
                                        None,
                                    );
                                    connection.session.set_blocking(false);
                                    match channel.map_err(anyhow::Error::from).and_then(|channel| {
                                        stream.set_nonblocking(true)?;
                                        Ok(Relay::new(stream, channel))
                                    }) {
                                        Ok(relay) => relays.push(relay),
                                        Err(_) => {
                                            *worker_status.lock().unwrap() =
                                                "目标不可达；继续监听".into()
                                        }
                                    }
                                }
                            }
                            Listener::Remote(listener) => {
                                if let Ok(channel) = listener.accept() {
                                    let target = (rule.target_host.as_str(), rule.target_port)
                                        .to_socket_addrs()
                                        .ok()
                                        .and_then(|mut values| values.next());
                                    if let Some(target) = target
                                        && let Ok(stream) = TcpStream::connect_timeout(
                                            &target,
                                            Duration::from_secs(5),
                                        )
                                        && stream.set_nonblocking(true).is_ok()
                                    {
                                        relays.push(Relay::new(stream, channel));
                                    }
                                }
                            }
                        }
                    }
                    relays.retain_mut(|relay| relay.pump().is_ok_and(|alive| alive));
                    thread::sleep(Duration::from_millis(5));
                }
                drop(listener);
                drop(relays);
                drop(connection);
                *worker_status.lock().unwrap() = "已停止".into();
            })?;
        Ok(Self {
            stop,
            status,
            bound_port,
        })
    }
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
    }
}
impl Drop for ForwardHandle {
    fn drop(&mut self) {
        self.stop();
    }
}
enum Listener {
    Local(TcpListener),
    Remote(ssh2::Listener),
}
struct Relay {
    stream: TcpStream,
    channel: Channel,
    to_remote: VecDeque<u8>,
    to_local: VecDeque<u8>,
    local_eof: bool,
    remote_eof: bool,
    sent_eof: bool,
    sent_shutdown: bool,
}
impl Relay {
    fn new(stream: TcpStream, channel: Channel) -> Self {
        Self {
            stream,
            channel,
            to_remote: VecDeque::new(),
            to_local: VecDeque::new(),
            local_eof: false,
            remote_eof: false,
            sent_eof: false,
            sent_shutdown: false,
        }
    }
    fn pump(&mut self) -> std::io::Result<bool> {
        let mut buffer = [0u8; 16384];
        if !self.local_eof && self.to_remote.len() < 1024 * 1024 {
            match self.stream.read(&mut buffer) {
                Ok(0) => self.local_eof = true,
                Ok(count) => self.to_remote.extend(&buffer[..count]),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error),
            }
        }
        if !self.remote_eof && self.to_local.len() < 1024 * 1024 {
            match self.channel.read(&mut buffer) {
                Ok(0) => self.remote_eof = true,
                Ok(count) => self.to_local.extend(&buffer[..count]),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error),
            }
        }
        flush_pending(&mut self.channel, &mut self.to_remote)?;
        flush_pending(&mut self.stream, &mut self.to_local)?;
        if self.local_eof
            && self.to_remote.is_empty()
            && !self.sent_eof
            && self.channel.send_eof().is_ok()
        {
            self.sent_eof = true;
        }
        if self.remote_eof && self.to_local.is_empty() && !self.sent_shutdown {
            self.stream.shutdown(Shutdown::Write)?;
            self.sent_shutdown = true;
        }
        Ok(!(self.local_eof
            && self.remote_eof
            && self.to_local.is_empty()
            && self.to_remote.is_empty()))
    }
}
fn flush_pending(writer: &mut impl Write, buffer: &mut VecDeque<u8>) -> std::io::Result<()> {
    if !buffer.is_empty() {
        match writer.write(buffer.make_contiguous()) {
            Ok(count) => {
                buffer.drain(..count);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
