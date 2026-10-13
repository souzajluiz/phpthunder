//! Markdown tabs: the source/split/preview layout and the rendered preview.

use super::*;
use crate::app::Message;
use crate::editor::Document;
use crate::tr;
use iced::widget::text::{LineHeight, Wrapping};
use iced::widget::{button, container, row, scrollable, space, stack, text, tooltip};
use iced::{Fill, Length, Padding, Theme};

/// Element type of the workbench views.
type El<'a> = iced::Element<'a, Message>;

/// A Markdown tab laid out by its [`MarkdownView`](crate::editor::MarkdownView):
/// `source` alone, beside the rendered preview, or the preview alone, with
/// the view toggle floating in the top-right corner.
pub(crate) fn markdown_layout<'a>(doc: &'a Document, source: El<'a>) -> El<'a> {
    use crate::editor::MarkdownView;
    let body: El<'a> = match doc.markdown_view {
        MarkdownView::Editor => source,
        MarkdownView::Split => row![
            container(source).width(Length::FillPortion(1)),
            container(space()).width(1).height(Fill).style(fill(BORDER)),
            container(markdown_preview(doc)).width(Length::FillPortion(1)),
        ]
        .into(),
        MarkdownView::Preview => markdown_preview(doc),
    };
    let choice = |view: MarkdownView, glyph: Icon, tip: String| -> El<'a> {
        let on = doc.markdown_view == view;
        tooltip(
            button(icon(glyph, 14.0, 2.0, if on { ACCENT } else { MUTED }))
                .padding(4)
                .style(toggle_button(on))
                .on_press(Message::MarkdownView(view)),
            container(label(tip, 12.0, TEXT))
                .padding([4, 8])
                .style(popup),
            tooltip::Position::Bottom,
        )
        .into()
    };
    let toggle = container(
        row![
            choice(MarkdownView::Editor, Icon::Lines, tr!("markdown-editor")),
            choice(MarkdownView::Split, Icon::Columns, tr!("markdown-split")),
            choice(MarkdownView::Preview, Icon::Eye, tr!("markdown-preview")),
        ]
        .spacing(2),
    )
    .padding(3)
    .style(popup);
    stack![
        body,
        container(toggle)
            .width(Fill)
            .align_right(Fill)
            .padding(Padding {
                top: 8.0,
                right: 16.0,
                bottom: 0.0,
                left: 0.0,
            }),
    ]
    .width(Fill)
    .height(Fill)
    .into()
}

/// The rendered Markdown of `doc`, in Geist on the island, at a readable
/// measure; clicked links arrive as [`Message::MarkdownLink`].
fn markdown_preview(doc: &Document) -> El<'_> {
    use iced::widget::markdown;
    let style = markdown::Style {
        font: UI,
        inline_code_highlight: markdown::Highlight {
            background: SELECTED.into(),
            border: iced::border::rounded(4),
        },
        inline_code_padding: Padding::from([0, 3]),
        inline_code_color: SYN_STRING,
        inline_code_font: MONO,
        code_block_font: MONO,
        link_color: ACCENT,
    };
    let mut settings = markdown::Settings::with_text_size(14, style);
    settings.code_size = CODE_SIZE.into();
    let page = markdown::view_with(&doc.markdown, settings, &PreviewViewer);
    scrollable(
        container(container(page).max_width(760))
            .padding(Padding {
                top: 48.0,
                right: 32.0,
                bottom: 120.0,
                left: 32.0,
            })
            .center_x(Fill),
    )
    .width(Fill)
    .height(Fill)
    .style(scrollbars)
    .into()
}

/// Markdown viewer of the preview pane: iced's defaults, except code blocks,
/// which iced draws in the body font (its spans override the block font);
/// here they are plain Geist Mono on a recessed well.
struct PreviewViewer;

impl<'a> iced::widget::markdown::Viewer<'a, Message> for PreviewViewer {
    fn on_link_click(url: iced::widget::markdown::Uri) -> Message {
        Message::MarkdownLink(url)
    }

    fn code_block(
        &self,
        settings: iced::widget::markdown::Settings,
        _language: Option<&'a str>,
        code: &'a str,
        _lines: &'a [iced::widget::markdown::Text],
    ) -> El<'a> {
        container(
            scrollable(
                container(
                    text(code.trim_end_matches('\n'))
                        .font(MONO)
                        .size(settings.code_size)
                        .line_height(LineHeight::Absolute(CODE_LINE.into()))
                        .color(SYN_VARIABLE)
                        .wrapping(Wrapping::None),
                )
                .padding([12, 16]),
            )
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::default().width(6).scroller_width(6),
            ))
            .style(scrollbars),
        )
        .width(Fill)
        .style(|_: &Theme| container::Style {
            background: Some(rgb(0x151821).into()),
            border: iced::border::rounded(6),
            ..Default::default()
        })
        .into()
    }
}
