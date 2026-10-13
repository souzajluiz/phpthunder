//! Development snapshots: with `PHPTHUNDER_SNAPSHOT=out.png` the IDE opens
//! `PHPTHUNDER_SNAPSHOT_FILE` (relative to the project) if given, waits for
//! indexing to settle, saves a PNG of its window and quits. With
//! `PHPTHUNDER_SNAPSHOT_TYPE=text` (and `PHPTHUNDER_SNAPSHOT_LINE=n`, 1-based)
//! it first types `text` on a new line after line `n` and opens completion;
//! `PHPTHUNDER_SNAPSHOT_WRAP=1` turns soft wrap on, and
//! `PHPTHUNDER_SNAPSHOT_CARETS=n` types with `n` carets on consecutive lines.
//! Used to check the look without a screen recorder.

use super::*;
use iced::Task;
use iced::window;
use std::path::PathBuf;
use std::time::Duration;

/// Delay before opening the snapshot file.
const OPEN_DELAY: Duration = Duration::from_secs(3);

/// Delay before typing the snapshot text (after indexing).
const TYPE_DELAY: Duration = Duration::from_secs(14);

/// Delay before taking the snapshot.
const SHOT_DELAY: Duration = Duration::from_secs(8);

/// Delay before the snapshot when text is typed first.
const SHOT_AFTER_TYPING: Duration = Duration::from_secs(16);

impl App {
    /// Tasks that open the snapshot file and take the snapshot, when
    /// `PHPTHUNDER_SNAPSHOT` is set; nothing otherwise. Called from `boot`.
    pub(crate) fn snapshot_tasks(root: Option<&std::path::Path>) -> Task<Message> {
        if std::env::var_os("PHPTHUNDER_SNAPSHOT").is_none() {
            return Task::none();
        }
        let open = match (root, std::env::var_os("PHPTHUNDER_SNAPSHOT_FILE")) {
            (Some(root), Some(file)) => {
                let path = root.join(file);
                Task::perform(tokio::time::sleep(OPEN_DELAY), move |()| {
                    Message::OpenFile(path)
                })
            }
            _ => Task::none(),
        };
        let typing = std::env::var("PHPTHUNDER_SNAPSHOT_TYPE").ok();
        let delay = if typing.is_some() {
            SHOT_AFTER_TYPING
        } else {
            SHOT_DELAY
        };
        let typed = match typing {
            Some(text) => Task::perform(tokio::time::sleep(TYPE_DELAY), move |()| {
                Message::SnapshotType(text)
            }),
            None => Task::none(),
        };
        let shot = Task::perform(tokio::time::sleep(delay), |()| Message::TakeSnapshot);
        // `PHPTHUNDER_SNAPSHOT_WRAP=1`: turn soft wrap on after opening.
        let wrap = if std::env::var_os("PHPTHUNDER_SNAPSHOT_WRAP").is_some() {
            Task::perform(tokio::time::sleep(OPEN_DELAY * 2), |()| {
                Message::Command(crate::keymap::Cmd::ToggleSoftWrap)
            })
        } else {
            Task::none()
        };
        Task::batch([open, typed, wrap, shot])
    }

    /// Types `text` on a new line after `PHPTHUNDER_SNAPSHOT_LINE` (default
    /// 1) of the active document and opens completion.
    pub(crate) fn snapshot_type(&mut self, text: &str) -> Task<Message> {
        let line = std::env::var("PHPTHUNDER_SNAPSHOT_LINE")
            .ok()
            .and_then(|l| l.parse::<usize>().ok())
            .unwrap_or(1)
            .max(1);
        let wanted = std::env::var_os("PHPTHUNDER_SNAPSHOT_FILE")
            .zip(self.tree.root.clone())
            .map(|(file, root)| root.join(file));
        if let Some(i) = wanted.and_then(|w| self.docs.iter().position(|d| d.path == w)) {
            self.active = Some(i);
        }
        let Some(doc) = self.doc_mut() else {
            return Task::none();
        };
        // `PHPTHUNDER_SNAPSHOT_CARETS=n`: type at the end of `n` lines at
        // once, with multiple carets, instead of on a new line.
        let carets = std::env::var("PHPTHUNDER_SNAPSHOT_CARETS")
            .ok()
            .and_then(|n| n.parse::<usize>().ok());
        if let Some(n) = carets {
            let at = doc.line_end(line - 1);
            let position = doc.position_of(at);
            doc.move_to(position);
            for _ in 1..n {
                doc.add_caret_vertical(false);
            }
            doc.multi_edit(crate::editor::MultiEdit::Insert(text.to_string()));
            self.after_change(true);
            return self.reveal_cursor();
        }
        let at = doc.line_end(line - 1);
        doc.replace(at, at, &format!("\n{text}"));
        self.after_change(true);
        self.refresh_completion(true);
        self.reveal_cursor()
    }

    /// Captures the window (answered by [`Message::Snapshot`]).
    pub(crate) fn take_snapshot(&self) -> Task<Message> {
        window::latest()
            .and_then(window::screenshot)
            .map(Message::Snapshot)
    }

    /// Writes the captured window to the `PHPTHUNDER_SNAPSHOT` path and quits.
    pub(crate) fn save_snapshot(&self, shot: window::Screenshot) -> Task<Message> {
        let Some(path) = std::env::var_os("PHPTHUNDER_SNAPSHOT").map(PathBuf::from) else {
            return Task::none();
        };
        if let Err(error) = write_png(&path, &shot) {
            eprintln!("snapshot failed: {error}");
        }
        iced::exit()
    }
}

/// Encodes RGBA pixels as a PNG file.
///
/// # Errors
/// Returns the I/O or encoding error.
fn write_png(path: &std::path::Path, shot: &window::Screenshot) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(
        std::io::BufWriter::new(file),
        shot.size.width,
        shot.size.height,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer
        .write_image_data(&shot.rgba)
        .map_err(|e| e.to_string())
}
