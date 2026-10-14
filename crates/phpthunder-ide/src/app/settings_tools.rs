//! Settings backup and sync commands: export to a file, import from one,
//! and sync through the Git repository named in the settings.

use super::*;
use crate::tr;
use iced::Task;
use std::path::PathBuf;

impl App {
    /// Asks where to save the settings export.
    pub(crate) fn export_settings(&mut self) -> Task<Message> {
        let dialog = rfd::AsyncFileDialog::new()
            .set_file_name("phpthunder-settings.json")
            .add_filter("JSON", &["json"]);
        Task::perform(
            async move { dialog.save_file().await.map(|h| h.path().to_path_buf()) },
            Message::SettingsExportPicked,
        )
    }

    /// Writes the settings export to the chosen file.
    pub(crate) fn settings_export_picked(&mut self, path: Option<PathBuf>) -> Task<Message> {
        let (Some(path), Some(dir)) = (path, crate::settings::user_config_dir()) else {
            return Task::none();
        };
        match std::fs::write(&path, crate::settings_sync::export(&dir)) {
            Ok(()) => self.notice = Some(tr!("settings-exported")),
            Err(error) => self.fail(error.to_string()),
        }
        Task::none()
    }

    /// Asks for a settings export to import.
    pub(crate) fn import_settings(&mut self) -> Task<Message> {
        let dialog = rfd::AsyncFileDialog::new().add_filter("JSON", &["json"]);
        Task::perform(
            async move { dialog.pick_file().await.map(|h| h.path().to_path_buf()) },
            Message::SettingsImportPicked,
        )
    }

    /// Imports the chosen export; settings apply at the next start.
    pub(crate) fn settings_import_picked(&mut self, path: Option<PathBuf>) -> Task<Message> {
        let (Some(path), Some(dir)) = (path, crate::settings::user_config_dir()) else {
            return Task::none();
        };
        let result = std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| crate::settings_sync::import(&dir, &text));
        match result {
            Ok(count) => self.notice = Some(tr!("settings-imported", count = count)),
            Err(error) => self.fail(error),
        }
        Task::none()
    }

    /// Syncs the settings folder with the configured Git repository.
    pub(crate) fn sync_settings(&mut self) -> Task<Message> {
        let Some(dir) = crate::settings::user_config_dir() else {
            return Task::none();
        };
        let Some(remote) = crate::settings::settings().settings_repository.clone() else {
            self.notice = Some(tr!("settings-sync-no-repository"));
            return Task::none();
        };
        self.notice = Some(tr!("settings-syncing"));
        Task::perform(
            crate::settings_sync::sync(dir, remote),
            Message::SettingsSynced,
        )
    }

    /// Reports how a sync went.
    pub(crate) fn settings_synced(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => self.notice = Some(tr!("settings-synced")),
            Err(error) => self.fail(tr!("settings-sync-failed", error = error)),
        }
    }
}
