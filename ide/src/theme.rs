//! Thunder Dark: palette, fonts, and widget styles.

use iced::font::Weight;
use iced::widget::{button, container, scrollable, text_editor, text_input};
use iced::{Background, Border, Color, Font, Shadow, Theme, Vector, border};

pub const fn rgb(hex: u32) -> Color {
    Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

// Surfaces
pub const BG: Color = rgb(0x101218);
pub const PANEL: Color = rgb(0x0E1015);
pub const CHROME: Color = rgb(0x0B0D12);
pub const POPUP: Color = rgb(0x161A22);
pub const BORDER: Color = rgb(0x1E222B);
pub const BORDER_STRONG: Color = rgb(0x2A3040);
pub const SELECTED: Color = rgb(0x1C2130);
pub const SELECTED_STRONG: Color = rgb(0x24304A);
pub const CURRENT_LINE: Color = rgb(0x161A23);

// Text
pub const TEXT: Color = rgb(0xD6D9E0);
pub const TEXT_BRIGHT: Color = rgb(0xFFFFFF);
pub const TEXT_SOFT: Color = rgb(0xC3C8D2);
pub const MUTED: Color = rgb(0x8A91A0);
pub const GUTTER: Color = rgb(0x4A5163);
pub const ACCENT: Color = rgb(0xF2C14E);

// Syntax
pub const SYN_KEYWORD: Color = rgb(0x7AA2F7);
pub const SYN_TYPE: Color = rgb(0x5FD4C4);
pub const SYN_FUNCTION: Color = rgb(0xE8B36A);
pub const SYN_VARIABLE: Color = rgb(0xE2E5EC);
pub const SYN_STRING: Color = rgb(0x9ECE6A);
pub const SYN_NUMBER: Color = rgb(0xFF9E64);
pub const SYN_COMMENT: Color = rgb(0x7F8799);
pub const SYN_DOC_TAG: Color = rgb(0xBB9AF7);
pub const SYN_PUNCT: Color = rgb(0x9AA3B5);

// Completion kind badges (background, foreground)
pub const BADGE_METHOD: (Color, Color) = (rgb(0x3B2F5C), rgb(0xC4A8FF));
pub const BADGE_PROPERTY: (Color, Color) = (rgb(0x1F3B39), rgb(0x6FE0D0));
pub const BADGE_CLASS: (Color, Color) = (rgb(0x3D3320), rgb(0xF2C14E));
pub const BADGE_OTHER: (Color, Color) = (rgb(0x262B36), rgb(0xC3C8D2));

pub const UI: Font = Font::with_name("Geist");
pub const UI_MEDIUM: Font = Font {
    weight: Weight::Medium,
    ..UI
};
pub const UI_BOLD: Font = Font {
    weight: Weight::Semibold,
    ..UI
};
pub const MONO: Font = Font::with_name("Geist Mono");

/// Editor metrics. Geist Mono advances 600/1000 em for every glyph.
pub const CODE_SIZE: f32 = 13.5;
pub const CODE_LINE: f32 = 22.0;
pub const CHAR_WIDTH: f32 = CODE_SIZE * 0.6;
pub const EDITOR_PAD_TOP: f32 = 12.0;
pub const EDITOR_PAD_LEFT: f32 = 4.0;

pub const FONTS: [&[u8]; 6] = [
    include_bytes!("../assets/fonts/Geist-400.ttf"),
    include_bytes!("../assets/fonts/Geist-500.ttf"),
    include_bytes!("../assets/fonts/Geist-600.ttf"),
    include_bytes!("../assets/fonts/GeistMono-400.ttf"),
    include_bytes!("../assets/fonts/GeistMono-500.ttf"),
    include_bytes!("../assets/fonts/GeistMono-600.ttf"),
];

pub fn app_theme() -> Theme {
    Theme::custom(
        "Thunder Dark".to_string(),
        iced::theme::Palette {
            background: BG,
            text: TEXT,
            primary: ACCENT,
            success: rgb(0x9ECE6A),
            warning: rgb(0xE0AF68),
            danger: rgb(0xF7768E),
        },
    )
}

pub fn fill(color: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(color.into()),
        ..Default::default()
    }
}

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

pub fn badge(colors: (Color, Color)) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(colors.0.into()),
        text_color: Some(colors.1),
        border: border::rounded(4),
        ..Default::default()
    }
}

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

pub fn ghost_button(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: matches!(status, button::Status::Hovered | button::Status::Pressed)
            .then(|| rgb(0x1C2130).into()),
        text_color: MUTED,
        border: border::rounded(4),
        ..Default::default()
    }
}

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

pub fn editor(_: &Theme, _: text_editor::Status) -> text_editor::Style {
    text_editor::Style {
        background: Color::TRANSPARENT.into(),
        border: Border::default(),
        placeholder: MUTED,
        value: SYN_VARIABLE,
        selection: rgb(0x2B3A5C),
    }
}

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
