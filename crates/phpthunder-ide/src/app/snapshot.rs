//! Development snapshots: with `PHPTHUNDER_SNAPSHOT=out.png` the IDE opens
//! `PHPTHUNDER_SNAPSHOT_FILE` (relative to the project) if given, waits for
//! indexing to settle, saves a PNG of its window and quits. Used to check
//! the look without a screen recorder.

use super::*;
use iced::Task;
use iced::window;
use std::path::PathBuf;
use std::time::Duration;

/// Delay before opening the snapshot file.
const OPEN_DELAY: Duration = Duration::from_secs(3);

/// Delay before taking the snapshot.
const SHOT_DELAY: Duration = Duration::from_secs(8);

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
                    Message::OpenFile(path.clone())
                })
            }
            _ => Task::none(),
        };
        let shot = Task::perform(tokio::time::sleep(SHOT_DELAY), |()| Message::TakeSnapshot);
        Task::batch([open, shot])
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
