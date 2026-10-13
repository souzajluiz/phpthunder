//! Thunder Purple: palette, fonts, and widget styles. The window is a
//! purple-gray frame holding rounded "islands" (tree, editor, panels).

use iced::font::Weight;
use iced::widget::{button, container, scrollable, text_editor, text_input};
use iced::{Background, Border, Color, Font, Shadow, Theme, Vector, border};

/// Builds a `Color` from a `0xRRGGBB` literal in a `const` context, so the
/// palette below can be written as hex. Also used for one-off shades in the
/// style functions and in the workbench and palette views.
pub const fn rgb(hex: u32) -> Color {
    Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

// Surfaces
/// Island surface: editor, project tree and bottom panel; also the iced
/// theme background.
pub const BG: Color = rgb(0x1D1D26);
/// Side surfaces inside islands: find bar, bottom tool panel headers.
pub const PANEL: Color = rgb(0x1D1D26);
/// Window frame around the islands: title bar, tool rail and status bar.
pub const CHROME: Color = rgb(0x2F2F40);
/// Background of floating surfaces drawn with `popup`.
pub const POPUP: Color = rgb(0x2C2C3B);
/// 1px separators between panes (`hsep` / `vsep` in the workbench view).
pub const BORDER: Color = rgb(0x2E2D3D);
/// Outline of floating surfaces drawn with `popup`.
pub const BORDER_STRONG: Color = rgb(0x4E4C63);
/// Selected tree/list rows, the active rail button, hovered completion and palette rows.
pub const SELECTED: Color = rgb(0x34334A);
/// Selected row in the completion popup and the navigation palette.
pub const SELECTED_STRONG: Color = rgb(0x5C3A7A);
/// Band behind the caret's line in the editor.
pub const CURRENT_LINE: Color = rgb(0x242433);

// Text
/// Default foreground text; also the iced theme text color.
pub const TEXT: Color = rgb(0xD0D0D9);
/// Emphasized text: active tab, selected rows, dialog titles, input values.
pub const TEXT_BRIGHT: Color = rgb(0xFFFFFF);
/// Secondary text: files in the tree, usage locations, shortcut labels.
pub const TEXT_SOFT: Color = rgb(0xC9C9D1);
/// De-emphasized text and icons: hints, counts, placeholders, inactive tabs.
pub const MUTED: Color = rgb(0x8785A6);
/// Editor line numbers, except the caret line which uses `TEXT`.
pub const GUTTER: Color = rgb(0x575566);
/// Lavender accent: active rail icon, fuzzy-match letters, find marks,
/// enabled toggles and the primary button (dark text on it).
pub const ACCENT: Color = rgb(0xB98BE8);

// Syntax
/// Keywords, modifiers and PHP tags (`crate::editor::format_token`).
pub const SYN_KEYWORD: Color = rgb(0xE6967A);
/// Class, interface and namespace names in type position (`crate::editor::format_token`).
pub const SYN_TYPE: Color = rgb(0xD0D0D9);
/// Function and method names (`crate::editor::format_token`).
pub const SYN_FUNCTION: Color = rgb(0xC7A65D);
/// Variables and constants (`crate::editor::format_token`); also the editor's default
/// text and the signature line of the completion and hover popups.
pub const SYN_VARIABLE: Color = rgb(0xC9C9D1);
/// String literals, heredocs and nowdocs (`crate::editor::format_token`).
pub const SYN_STRING: Color = rgb(0x62A362);
/// Numeric literals (`crate::editor::format_token`).
pub const SYN_NUMBER: Color = rgb(0x37AEF3);
/// Comments (`crate::editor::format_token`).
pub const SYN_COMMENT: Color = rgb(0x676773);
/// `@tag` words inside doc comments (`crate::editor::format_token`).
pub const SYN_DOC_TAG: Color = rgb(0x8C6154);
/// Member and scope operators such as `->` and `::` (`crate::editor::format_token`).
pub const SYN_PUNCT: Color = rgb(0xC9C9D1);
/// Variables, properties and constants (`crate::editor::format_token`).
pub const SYN_FIELD: Color = rgb(0x93A6F5);
/// Commas and semicolons, drawn bold (`crate::editor::format_token`).
pub const SYN_SEPARATOR: Color = rgb(0xE0957B);

// Completion kind badges (background, foreground)
/// Completion badge for methods and functions (`kind_badge` in the workbench view).
pub const BADGE_METHOD: (Color, Color) = (rgb(0x3D2E52), rgb(0xC7A65D));
/// Completion badge for properties.
pub const BADGE_PROPERTY: (Color, Color) = (rgb(0x2B3150), rgb(0x93A6F5));
/// Completion badge for classes, traits, interfaces and enums.
pub const BADGE_CLASS: (Color, Color) = (rgb(0x4A2E3A), rgb(0xE6967A));
/// Completion badge for constants and enum cases.
pub const BADGE_OTHER: (Color, Color) = (rgb(0x34334A), rgb(0xC9C9D1));

/// Proportional UI font; installed as the app's default font in `main`.
pub const UI: Font = Font::with_name("Geist");
/// Medium-weight UI font for button labels and file names in find results.
pub const UI_MEDIUM: Font = Font {
    weight: Weight::Medium,
    ..UI
};
/// Semibold UI font for the wordmark, section headers and dialog titles.
pub const UI_BOLD: Font = Font {
    weight: Weight::Semibold,
    ..UI
};
/// Monospace font for the editor, gutter, code previews and shortcut labels.
pub const MONO: Font = Font::with_name("Geist Mono");
/// Bold monospace, for commas and semicolons in code.
pub const MONO_BOLD: Font = Font {
    weight: Weight::Semibold,
    ..MONO
};

/// Editor metrics. Geist Mono advances 600/1000 em for every glyph.
pub const CODE_SIZE: f32 = 13.5;
/// Editor line height in px; maps lines to y offsets for the caret band,
/// find marks, popups and scrolling.
pub const CODE_LINE: f32 = 22.0;
/// Width of one monospace cell; maps columns to x offsets for popups and find marks.
pub const CHAR_WIDTH: f32 = CODE_SIZE * 0.6;
/// Top padding inside the editor, added to every line-to-y conversion.
pub const EDITOR_PAD_TOP: f32 = 12.0;
/// Left padding between the gutter and the first text column.
pub const EDITOR_PAD_LEFT: f32 = 4.0;

/// Embedded Geist and Geist Mono faces, registered with iced in `main` so the
/// IDE renders the same on machines without the fonts installed.
pub const FONTS: [&[u8]; 6] = [
    include_bytes!("../../assets/fonts/Geist-400.ttf"),
    include_bytes!("../../assets/fonts/Geist-500.ttf"),
    include_bytes!("../../assets/fonts/Geist-600.ttf"),
    include_bytes!("../../assets/fonts/GeistMono-400.ttf"),
    include_bytes!("../../assets/fonts/GeistMono-500.ttf"),
    include_bytes!("../../assets/fonts/GeistMono-600.ttf"),
];

/// The iced `Theme` for the whole window, built from this palette. Installed
/// by `main`; widgets needing finer control use the style functions below.
pub fn app_theme() -> Theme {
    Theme::custom(
        "Thunder Purple".to_string(),
        iced::theme::Palette {
            background: BG,
            text: TEXT,
            primary: ACCENT,
            success: rgb(0x76B366),
            warning: rgb(0xE0AF68),
            danger: rgb(0xDD3962),
        },
    )
}

/// Corner radius of islands (tree, editor, bottom panel).
pub const ISLAND_RADIUS: f32 = 10.0;

/// Gap between islands and around them, showing the window frame.
pub const ISLAND_GAP: f32 = 5.0;

/// Container style of an island: a rounded `BG` surface on the frame.
pub fn island(_: &Theme) -> container::Style {
    container::Style {
        background: Some(BG.into()),
        border: border::rounded(ISLAND_RADIUS),
        ..Default::default()
    }
}

/// Container style that paints a solid `color` background. Used for panes,
/// separators, the caret-line band and the modal backdrop in the workbench view.
pub fn fill(color: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(color.into()),
        ..Default::default()
    }
}

/// Container style for floating surfaces: rounded, outlined, with a drop
/// shadow. Used by tooltips, toasts, the completion and hover popups, the
/// find-in-path dialog and the navigation palette.
pub fn popup(_: &Theme) -> container::Style {
    container::Style {
        background: Some(POPUP.into()),
        border: Border {
            color: BORDER_STRONG,
            width: 1.0,
            radius: 8.0.into(),
        },
        shadow: Shadow {
            color: Color {
                a: 0.45,
                ..Color::BLACK
            },
            offset: Vector::new(0.0, 12.0),
            blur_radius: 32.0,
        },
        text_color: Some(TEXT),
        ..Default::default()
    }
}

/// Container style for a completion kind badge; `colors` is a
/// `(background, foreground)` pair such as `BADGE_METHOD`. Used by
/// `kind_badge` in the workbench view.
pub fn badge(colors: (Color, Color)) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(colors.0.into()),
        text_color: Some(colors.1),
        border: border::rounded(4),
        ..Default::default()
    }
}

/// Small rounded tag, used for the "declaration" marker in the usages panel.
pub fn pill(_: &Theme) -> container::Style {
    container::Style {
        background: Some(rgb(0x262B36).into()),
        text_color: Some(MUTED),
        border: border::rounded(4),
        ..Default::default()
    }
}

/// Flat list/tree row: transparent, highlighted when hovered or selected.
pub fn row_button(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let bg = match (selected, status) {
            (true, _) => Some(SELECTED),
            (false, button::Status::Hovered | button::Status::Pressed) => Some(rgb(0x161A23)),
            _ => None,
        };
        button::Style {
            background: bg.map(Background::Color),
            text_color: if selected { TEXT_BRIGHT } else { TEXT_SOFT },
            border: border::rounded(4),
            ..Default::default()
        }
    }
}

/// Tool-rail button style; `active` marks the open tool with an accent icon
/// on a selected background.
pub fn rail_button(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: match (active, status) {
            (true, _) => Some(SELECTED.into()),
            (false, button::Status::Hovered) => Some(rgb(0x161A23).into()),
            _ => None,
        },
        text_color: if active { ACCENT } else { MUTED },
        border: border::rounded(8),
        ..Default::default()
    }
}

/// Borderless button that only shows a background on hover. Used for close
/// buttons, find navigation arrows and replace actions.
pub fn ghost_button(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: matches!(status, button::Status::Hovered | button::Status::Pressed)
            .then(|| rgb(0x1C2130).into()),
        text_color: MUTED,
        border: border::rounded(4),
        ..Default::default()
    }
}

/// Accent-filled call-to-action button ("Open Folder" on the welcome
/// screen, "Find" in the find-in-path dialog).
pub fn primary_button(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(
            if matches!(status, button::Status::Hovered) {
                rgb(0xFFD873)
            } else {
                ACCENT
            }
            .into(),
        ),
        text_color: rgb(0x14161C),
        border: border::rounded(6),
        ..Default::default()
    }
}

/// Text editor style: transparent so the caret-line band and find marks drawn
/// underneath show through; unhighlighted text uses `SYN_VARIABLE`.
pub fn editor(_: &Theme, _: text_editor::Status) -> text_editor::Style {
    text_editor::Style {
        background: Color::TRANSPARENT.into(),
        border: Border::default(),
        placeholder: MUTED,
        value: SYN_VARIABLE,
        selection: rgb(0x2B3A5C),
    }
}

/// Thin translucent scrollbars that brighten on hover. Applied to the editor,
/// project tree and bottom panel lists.
pub fn scrollbars(_: &Theme, status: scrollable::Status) -> scrollable::Style {
    let hovered = !matches!(status, scrollable::Status::Active { .. });
    let rail = scrollable::Rail {
        background: None,
        border: Border::default(),
        scroller: scrollable::Scroller {
            background: Color {
                a: if hovered { 0.35 } else { 0.18 },
                ..rgb(0x8A91A0)
            }
            .into(),
            border: border::rounded(4),
        },
    };
    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
        auto_scroll: scrollable::AutoScroll {
            background: Color::TRANSPARENT.into(),
            border: Border::default(),
            shadow: Shadow::default(),
            icon: MUTED,
        },
    }
}

/// Borderless text input, used for the navigation palette's query field.
pub fn input(_: &Theme, _: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: Color::TRANSPARENT.into(),
        border: Border::default(),
        icon: MUTED,
        placeholder: rgb(0x5C6475),
        value: TEXT_BRIGHT,
        selection: rgb(0x2B3A5C),
    }
}

/// A boxed input (find bar, dialogs).
pub fn boxed_input(_: &Theme, status: text_input::Status) -> text_input::Style {
    let focused = matches!(status, text_input::Status::Focused { .. });
    text_input::Style {
        background: rgb(0x151821).into(),
        border: Border {
            color: if focused { ACCENT } else { rgb(0x262B36) },
            width: 1.0,
            radius: 5.0.into(),
        },
        icon: MUTED,
        placeholder: rgb(0x5C6475),
        value: TEXT_BRIGHT,
        selection: rgb(0x2B3A5C),
    }
}

/// Small toggle button (regex / case / whole word).
pub fn toggle_button(on: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: Some(
            if on {
                rgb(0x3D3320)
            } else if matches!(status, button::Status::Hovered) {
                rgb(0x1C2130)
            } else {
                Color::TRANSPARENT
            }
            .into(),
        ),
        text_color: if on { ACCENT } else { MUTED },
        border: Border {
            color: if on {
                rgb(0x6B5420)
            } else {
                Color::TRANSPARENT
            },
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}
