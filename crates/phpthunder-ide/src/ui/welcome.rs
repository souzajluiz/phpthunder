//! The welcome screen shown before a project is open: the logo and the
//! Open Folder button on one side, the recent projects and the main
//! shortcuts on the other.

use super::*;
use crate::app::{App, Message};
use crate::keymap::{Cmd, hint};
use crate::tr;
use iced::widget::{button, column, container, row, space, text};
use iced::{Center, Element, Fill, Length};
use std::path::Path;

/// Most recent projects listed.
const MAX_RECENT: usize = 6;

/// `/Users/me/code/app` -> `~/code/app`.
fn home_relative(path: &Path) -> String {
    let shown = path.display().to_string();
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && shown.starts_with(&home) => {
            format!("~{}", &shown[home.len()..])
        }
        _ => shown,
    }
}

impl App {
    /// The welcome screen's content (without the window drag strip, which
    /// `welcome` in the workbench adds above it).
    pub(crate) fn welcome_body(&self) -> Element<'_, Message> {
        let open = button(
            row![
                icon(Icon::Folder, 16.0, 2.2, rgb(0x1D1D26)),
                label(tr!("welcome-open-folder"), 14.0, rgb(0x1D1D26)).font(UI_MEDIUM),
            ]
            .spacing(10)
            .align_y(Center),
        )
        .padding([12, 22])
        .style(primary_button)
        .on_press(Message::OpenFolderDialog);
        let intro = column![
            logo(Logo::Lockup, 220.0),
            text(tr!("welcome-hint"))
                .size(14)
                .color(MUTED)
                .width(340)
                .align_x(iced::alignment::Horizontal::Center),
            open,
        ]
        .spacing(22)
        .align_x(Center);

        let mut recent = column![
            container(label(tr!("welcome-recent"), 11.0, MUTED).font(UI_BOLD)).padding([0, 14])
        ]
        .spacing(4);
        let projects: Vec<&Path> = self
            .session
            .recent
            .iter()
            .map(|p| p.as_path())
            .filter(|p| p.is_dir())
            .take(MAX_RECENT)
            .collect();
        if projects.is_empty() {
            recent = recent
                .push(container(label(tr!("welcome-no-recent"), 13.0, MUTED)).padding([8, 14]));
        }
        for path in projects {
            let name = crate::app::file_name(path);
            let initial = name
                .chars()
                .next()
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_default();
            let badge = container(label(initial, 13.0, ACCENT).font(UI_BOLD))
                .width(32)
                .height(32)
                .center(32)
                .style(|_| iced::widget::container::Style {
                    background: Some(SELECTED.into()),
                    border: iced::border::rounded(8),
                    ..Default::default()
                });
            let entry = row![
                badge,
                column![
                    label(name, 13.0, TEXT).font(UI_MEDIUM),
                    label(home_relative(path), 12.0, MUTED),
                ]
                .spacing(2),
            ]
            .spacing(12)
            .align_y(Center);
            recent = recent.push(
                button(entry)
                    .width(Fill)
                    .padding([8, 14])
                    .style(ghost_button)
                    .on_press(Message::FolderPicked(Some(path.to_path_buf()))),
            );
        }

        let shortcut = |cmd: Cmd, name: String| -> Element<'_, Message> {
            row![
                container(label(hint(cmd), 12.0, TEXT_SOFT).font(MONO))
                    .padding([2, 8])
                    .style(|_| iced::widget::container::Style {
                        background: Some(POPUP.into()),
                        border: iced::border::rounded(6).color(BORDER_STRONG).width(1),
                        ..Default::default()
                    }),
                label(name, 13.0, MUTED),
            ]
            .spacing(10)
            .align_y(Center)
            .width(Fill)
            .into()
        };
        let shortcuts = column![
            row![
                shortcut(Cmd::SearchEverywhere, tr!("action-search-everywhere")),
                shortcut(Cmd::GotoClass, tr!("action-go-to-class")),
            ]
            .spacing(18),
            row![
                shortcut(Cmd::ToggleTerminal, tr!("panel-terminal")),
                shortcut(Cmd::ToggleClaude, tr!("panel-claude")),
            ]
            .spacing(18),
        ]
        .spacing(10)
        .padding([0, 14]);

        let side = column![recent, space().height(16), shortcuts]
            .spacing(6)
            .width(Length::Fixed(420.0));
        container(row![intro, side].spacing(72).align_y(Center))
            .center(Fill)
            .padding(24)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_relative_should_shorten_paths_under_home() {
        let home = std::env::var("HOME").unwrap_or_default();
        if home.is_empty() {
            return;
        }
        let path = Path::new(&home).join("code/app");
        assert_eq!(home_relative(&path), "~/code/app");
        assert_eq!(home_relative(Path::new("/opt/x")), "/opt/x");
    }
}
