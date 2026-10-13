//! The Compare panel: a heading, Restore for history versions, and the
//! diff from the other text to the file.

use super::*;
use crate::app::{App, Message};
use crate::tr;
use iced::widget::{button, column, container, row, space};
use iced::{Center, Element, Fill};

impl App {
    /// Body of the Compare panel; empty until something was compared.
    pub(crate) fn compare_body(&self) -> Element<'_, Message> {
        let Some(compare) = &self.compare else {
            return space().into();
        };
        let mut header = row![
            label(compare.title.clone(), 12.0, TEXT_SOFT),
            space().width(Fill)
        ]
        .align_y(Center)
        .padding([6, 16]);
        if compare.restore.is_some() {
            header = header.push(
                button(label(tr!("compare-restore"), 12.0, TEXT).font(UI_MEDIUM))
                    .padding([3, 10])
                    .style(primary_button)
                    .on_press(Message::RestoreCompared),
            );
        }
        let body: Element<'_, Message> = if compare.diff.is_empty() {
            container(label(tr!("compare-identical"), 12.0, MUTED))
                .padding([10, 16])
                .into()
        } else {
            diff_list(&compare.diff, |n| Message::JumpToLine(n.saturating_sub(1)))
        };
        column![header, body].into()
    }
}
