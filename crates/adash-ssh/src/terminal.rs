use crate::Connection;
use anyhow::Result;
use std::{
    collections::VecDeque,
    io::{Read, Write},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

pub enum TerminalCommand {
    Input(Vec<u8>),
    Resize(u16, u16),
    Scroll(i32),
    Close,
}
pub struct Terminal {
    commands: mpsc::Sender<TerminalCommand>,
    pub screen: Arc<Mutex<vt100::Parser>>,
    pub changed: Arc<AtomicBool>,
    pub status: Arc<Mutex<String>>,
    requested_size: Mutex<(u16, u16)>,
}
impl Terminal {
    pub fn start(connection: Connection, rows: u16, cols: u16) -> Result<Self> {
        let mut channel = connection.session.channel_session()?;
        channel.request_pty(
            "xterm-256color",
            None,
            Some((cols as u32, rows as u32, 0, 0)),
        )?;
        channel.shell()?;
        connection.session.set_blocking(false);
        let (sender, receiver) = mpsc::channel();
        let screen = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 10_000)));
        let changed = Arc::new(AtomicBool::new(true));
        let status = Arc::new(Mutex::new("已连接".to_owned()));
        let worker_screen = screen.clone();
        let worker_changed = changed.clone();
        let worker_status = status.clone();
        thread::Builder::new()
            .name("adash-ssh-pty".into())
            .spawn(move || {
                let mut pending = VecDeque::new();
                let mut buffer = [0u8; 16384];
                'session: loop {
                    for command in receiver.try_iter() {
                        match command {
                            TerminalCommand::Input(bytes) => {
                                worker_screen.lock().unwrap().screen_mut().set_scrollback(0);
                                worker_changed.store(true, Ordering::Release);
                                pending.extend(bytes);
                            }
                            TerminalCommand::Resize(rows, cols) => {
                                connection.session.set_blocking(true);
                                let result =
                                    channel.request_pty_size(cols as u32, rows as u32, None, None);
                                connection.session.set_blocking(false);
                                if result.is_ok() {
                                    worker_screen
                                        .lock()
                                        .unwrap()
                                        .screen_mut()
                                        .set_size(rows, cols);
                                    worker_changed.store(true, Ordering::Release);
                                }
                            }
                            TerminalCommand::Scroll(offset) => {
                                let mut parser = worker_screen.lock().unwrap();
                                let screen = parser.screen_mut();
                                if !screen.alternate_screen() {
                                    let next = (screen.scrollback() as i64 + i64::from(offset))
                                        .max(0)
                                        as usize;
                                    screen.set_scrollback(next);
                                    worker_changed.store(true, Ordering::Release);
                                }
                            }
                            TerminalCommand::Close => break 'session,
                        }
                    }
                    if !pending.is_empty() {
                        match channel.write(pending.make_contiguous()) {
                            Ok(count) => {
                                pending.drain(..count);
                            }
                            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                            Err(_) => {
                                *worker_status.lock().unwrap() = "输入通道已关闭".into();
                                break;
                            }
                        }
                    }
                    loop {
                        match channel.read(&mut buffer) {
                            Ok(0) => break,
                            Ok(count) => {
                                worker_screen.lock().unwrap().process(&buffer[..count]);
                                worker_changed.store(true, Ordering::Release);
                            }
                            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                            Err(_) => {
                                *worker_status.lock().unwrap() = "连接中断".into();
                                break 'session;
                            }
                        }
                    }
                    if channel.eof() {
                        break;
                    }
                    thread::sleep(Duration::from_millis(12));
                }
                let _ = channel.close();
                *worker_status.lock().unwrap() = "已断开".into();
                worker_changed.store(true, Ordering::Release);
            })?;
        Ok(Self {
            commands: sender,
            screen,
            changed,
            status,
            requested_size: Mutex::new((rows, cols)),
        })
    }
    pub fn input_sender(&self) -> mpsc::Sender<TerminalCommand> {
        self.commands.clone()
    }
    pub fn input(&self, bytes: impl Into<Vec<u8>>) {
        self.screen.lock().unwrap().screen_mut().set_scrollback(0);
        self.changed.store(true, Ordering::Release);
        let _ = self.commands.send(TerminalCommand::Input(bytes.into()));
    }
    pub fn resize(&self, rows: u16, cols: u16) {
        let size = (rows.clamp(2, 200), cols.clamp(2, 500));
        let mut requested = self.requested_size.lock().unwrap();
        if *requested != size {
            *requested = size;
            let _ = self.commands.send(TerminalCommand::Resize(size.0, size.1));
        }
    }
    pub fn scroll(&self, lines: i32) {
        let _ = self.commands.send(TerminalCommand::Scroll(lines));
    }
    pub fn is_connected(&self) -> bool {
        self.status.lock().unwrap().as_str() == "已连接"
    }
    pub fn close(&self) {
        let _ = self.commands.send(TerminalCommand::Close);
    }
    pub fn contents(&self) -> String {
        self.screen.lock().unwrap().screen().contents()
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn terminal_emulates_cursor_clear_and_color() {
        let mut terminal = vt100::Parser::new(4, 20, 100);
        terminal.process(b"abc\rX\x1b[31mred\x1b[0m");
        assert!(terminal.screen().contents().starts_with("Xred"));
        assert_eq!(
            terminal.screen().cell(0, 1).unwrap().fgcolor(),
            vt100::Color::Idx(1)
        );
        terminal.process(b"\x1b[2J\x1b[Hclean");
        assert_eq!(terminal.screen().contents(), "clean");
    }
}
