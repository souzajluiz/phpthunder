//! A shell running in a pseudo-terminal, with its screen kept by a VT100
//! parser on a reader thread.

use crate::tr;
use iced::futures::Stream;
use parking_lot::Mutex;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

/// Lines kept above the visible screen.
const SCROLLBACK: usize = 5_000;

/// Source of unique session ids (subscription keys).
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Output notifiers waiting to be picked up by [`output_events`], by session.
///
/// CONTEXT: `Subscription::run_with` only accepts a `fn` pointer, so the
/// receiver created with the session is parked here and taken by id.
fn pending_receivers() -> &'static Mutex<HashMap<u64, UnboundedReceiver<()>>> {
    static RECEIVERS: OnceLock<Mutex<HashMap<u64, UnboundedReceiver<()>>>> = OnceLock::new();
    RECEIVERS.get_or_init(Default::default)
}

/// One running shell.
pub struct Session {
    /// Unique id; keys the output subscription.
    pub id: u64,
    /// Terminal state (screen, scrollback, modes), updated by the reader thread.
    parser: Arc<Mutex<vt100::Parser>>,
    /// Input to the shell.
    writer: Box<dyn Write + Send>,
    /// PTY master, kept to resize the terminal.
    master: Box<dyn MasterPty + Send>,
    /// The shell process; killed when the session is dropped.
    child: Box<dyn Child + Send + Sync>,
    /// Set by the reader thread when the shell closed its output.
    exited: Arc<AtomicBool>,
    /// Set while an output notification is in flight, so bursts coalesce
    /// into one redraw.
    notified: Arc<AtomicBool>,
    /// Current size as (rows, cols).
    size: (u16, u16),
}

impl Session {
    /// Starts the user's login shell in `cwd` with a `rows` x `cols` screen.
    ///
    /// # Errors
    /// Returns a message when the PTY cannot be opened or the shell fails to
    /// start.
    pub fn spawn(cwd: &Path, rows: u16, cols: u16) -> Result<Self, String> {
        Self::start(CommandBuilder::new_default_prog(), cwd, rows, cols)
    }

    /// Starts `program` (a command line, e.g. `claude`) in `cwd` through the
    /// user's login shell (`$SHELL -lc 'exec …'`), so it is found on the
    /// `PATH` the user's profile sets up even when the IDE was launched from
    /// the Finder. The session ends when the program exits.
    ///
    /// # Errors
    /// Returns a message when the PTY cannot be opened or the shell fails to
    /// start. A program missing from `PATH` is reported by the shell, inside
    /// the session.
    pub fn spawn_program(cwd: &Path, rows: u16, cols: u16, program: &str) -> Result<Self, String> {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        let mut cmd = CommandBuilder::new(shell);
        cmd.args(["-lc", &format!("exec {program}")]);
        Self::start(cmd, cwd, rows, cols)
    }

    /// Opens a PTY, runs `cmd` in it with `cwd` as working directory and a
    /// `rows` x `cols` screen, and starts the output reader thread.
    fn start(mut cmd: CommandBuilder, cwd: &Path, rows: u16, cols: u16) -> Result<Self, String> {
        let pty = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| tr!("error-terminal-open", error = e.to_string()))?;
        cmd.cwd(cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        let child = pty
            .slave
            .spawn_command(cmd)
            .map_err(|e| tr!("error-terminal-shell", error = e.to_string()))?;
        let mut reader = pty.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pty.master.take_writer().map_err(|e| e.to_string())?;
        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, SCROLLBACK)));
        let exited = Arc::new(AtomicBool::new(false));
        let notified = Arc::new(AtomicBool::new(false));
        let (tx, rx): (UnboundedSender<()>, _) = unbounded_channel();
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        pending_receivers().lock().insert(id, rx);

        let (p, e, n) = (parser.clone(), exited.clone(), notified.clone());
        std::thread::Builder::new()
            .name(format!("terminal-{id}"))
            .spawn(move || {
                let mut buf = [0u8; 16 * 1024];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n_read) => p.lock().process(&buf[..n_read]),
                    }
                    if !n.swap(true, Ordering::AcqRel) && tx.send(()).is_err() {
                        break;
                    }
                }
                e.store(true, Ordering::Release);
                let _ = tx.send(());
            })
            .map_err(|e| e.to_string())?;

        Ok(Self {
            id,
            parser,
            writer,
            master: pty.master,
            child,
            exited,
            notified,
            size: (rows, cols),
        })
    }

    /// Sends input bytes (keys, pasted text) to the shell.
    pub fn write(&mut self, bytes: &[u8]) {
        // A closed shell just ignores input; the panel shows that it exited.
        let _ = self
            .writer
            .write_all(bytes)
            .and_then(|()| self.writer.flush());
    }

    /// Resizes the PTY and the screen when the panel size changed.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        let (rows, cols) = (rows.max(2), cols.max(10));
        if (rows, cols) == self.size {
            return;
        }
        self.size = (rows, cols);
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
        self.parser.lock().screen_mut().set_size(rows, cols);
    }

    /// Runs `f` with the current screen (for drawing and key encoding).
    pub fn with_screen<R>(&self, f: impl FnOnce(&vt100::Screen) -> R) -> R {
        f(self.parser.lock().screen())
    }

    /// Scrolls the view into the scrollback by `delta` lines (positive is up).
    pub fn scroll(&mut self, delta: i32) {
        let mut parser = self.parser.lock();
        let current = parser.screen().scrollback() as i32;
        parser
            .screen_mut()
            .set_scrollback((current + delta).max(0) as usize);
    }

    /// Whether the shell has exited.
    pub fn has_exited(&self) -> bool {
        self.exited.load(Ordering::Acquire)
    }

    /// Marks the last output notification as handled so the next output
    /// produces a new one. Called when the UI processes it.
    pub fn acknowledge_output(&self) {
        self.notified.store(false, Ordering::Release);
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        pending_receivers().lock().remove(&self.id);
    }
}

/// Stream of "new output" events for session `id`, for
/// `Subscription::run_with`. Ends when the shell exits.
pub fn output_events(id: &u64) -> impl Stream<Item = u64> + use<> {
    let id = *id;
    let receiver = pending_receivers().lock().remove(&id);
    iced::futures::stream::unfold(receiver, move |receiver| async move {
        let mut receiver = receiver?;
        receiver.recv().await?;
        Some((id, Some(receiver)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn session_should_run_commands_and_show_their_output() {
        let mut session = Session::spawn(&std::env::temp_dir(), 10, 60).expect("shell starts");
        session.write(b"echo phpthunder-$((40+2))\r");
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut seen = String::new();
        while Instant::now() < deadline {
            seen = session.with_screen(vt100::Screen::contents);
            if seen.contains("phpthunder-42") {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(seen.contains("phpthunder-42"), "screen was: {seen}");
        session.resize(20, 80);
        assert_eq!(session.with_screen(vt100::Screen::size), (20, 80));
    }

    #[test]
    fn spawn_program_should_run_it_through_the_login_shell_and_end_with_it() {
        let session = Session::spawn_program(
            &std::env::temp_dir(),
            10,
            60,
            "printf 'phpthunder-%s' $((40+2))",
        )
        .expect("program starts");
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline && !session.has_exited() {
            std::thread::sleep(Duration::from_millis(20));
        }
        let seen = session.with_screen(vt100::Screen::contents);
        assert!(seen.contains("phpthunder-42"), "screen was: {seen}");
        assert!(session.has_exited());
    }
}
