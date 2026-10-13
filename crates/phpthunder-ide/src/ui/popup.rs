//! Caret popups and dialogs added with inspections and refactorings: the
//! `⌥↵` menu, parameter info, and the rename dialog.

use super::*;
use crate::app::{App, Intention, Intentions, Message, RENAME_INPUT, SignatureView};
use iced::widget::text::Span;
use iced::widget::{Id, button, column, container, rich_text, row, space, text_input};
use iced::{Center, Element, Fill, Font, Length, Padding, Theme};

/// Row height of the `⌥↵` menu.
const INTENTION_ROW: f32 = 28.0;
/// Width of the `⌥↵` menu.
pub(crate) const INTENTIONS_WIDTH: f32 = 380.0;

/// The `⌥↵` menu with its height, placed at the caret by the overlays layer.
pub(crate) fn intentions_popup(menu: &Intentions) -> (Element<'_, Message>, f32) {
    let rows = menu.items.iter().enumerate().map(|(i, item)| {
        let selected = i == menu.selected;
        let (mark, color) = match item {
            Intention::Fix(_) => ("◆", ACCENT),
            Intention::ImplementMethods { .. } => ("+", SYN_TYPE),
            Intention::OptimizeImports => ("≡", MUTED),
        };
        button(
            row![
                container(label(mark, 11.0, color)).width(16).center_x(16),
                label(
                    item.title(),
                    13.0,
                    if selected { TEXT_BRIGHT } else { TEXT }
                ),
            ]
            .spacing(8)
            .align_y(Center),
        )
        .on_press(Message::IntentionPick(i))
        .width(Fill)
        .height(INTENTION_ROW)
        .padding([0, 8])
        .style(move |_: &Theme, status| button::Style {
            background: match (selected, status) {
                (true, _) => Some(SELECTED_STRONG.into()),
                (false, button::Status::Hovered) => Some(SELECTED.into()),
                _ => None,
            },
            text_color: TEXT,
            border: iced::border::rounded(5),
            ..Default::default()
        })
        .into()
    });
    let height = menu.items.len() as f32 * INTENTION_ROW + 10.0;
    let popup = container(column(rows).padding(4))
        .width(INTENTIONS_WIDTH)
        .style(popup);
    (popup.into(), height)
}

/// Parameter info: the callee signature with the active parameter in the
/// accent color, shown above the caret line.
pub(crate) fn signature_popup(view: &SignatureView) -> Element<'_, Message> {
    let help = &view.help;
    let mut spans: Vec<Span<'_, (), Font>> = Vec::new();
    // Highlight the active parameter's text inside the full signature.
    let active = help.params.get(help.active).map(String::as_str);
    match active.and_then(|p| help.label.find(p).map(|i| (i, p.len()))) {
        Some((start, len)) => {
            spans.push(Span::new(help.label[..start].to_string()).color(SYN_VARIABLE));
            spans.push(Span::new(help.label[start..start + len].to_string()).color(ACCENT));
            spans.push(Span::new(help.label[start + len..].to_string()).color(SYN_VARIABLE));
        }
        None => spans.push(Span::new(help.label.clone()).color(SYN_VARIABLE)),
    }
    container(rich_text(spans).font(MONO).size(12.5))
        .padding([6, 10])
        .max_width(720)
        .style(popup)
        .into()
}

impl App {
    /// The rename dialog (`⇧F6`), drawn as a modal by the workbench.
    pub(crate) fn rename_dialog_view(&self) -> Element<'_, Message> {
        let Some(d) = &self.rename else {
            return space().into();
        };
        let input = text_input("New name", &d.name)
            .id(Id::new(RENAME_INPUT))
            .on_input(Message::RenameInput)
            .on_submit(Message::RenameSubmit)
            .size(14)
            .font(MONO)
            .padding([8, 10])
            .style(boxed_input);
        let mut content = column![
            row![
                label(format!("Rename {}", d.original), 14.0, TEXT_BRIGHT).font(UI_BOLD),
                space().width(Fill),
                label("↵ rename   esc cancel", 11.0, MUTED),
            ]
            .align_y(Center),
            input,
            label("Updates every usage in the project; a class file named after the class is renamed too.", 12.0, MUTED),
        ]
        .spacing(12)
        .padding(16)
        .width(Length::Fixed(560.0));
        if let Some(error) = &d.error {
            content = content.push(label(error.clone(), 12.0, rgb(0xF7768E)));
        }
        container(container(content).style(popup))
            .width(Fill)
            .height(Fill)
            .align_x(iced::alignment::Horizontal::Center)
            .padding(Padding {
                top: 110.0,
                ..Padding::ZERO
            })
            .into()
    }
}
