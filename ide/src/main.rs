//! phpthunder IDE: a native PHP editor built on the phpthunder analysis engine.
//!
//! Usage: `phpthunder-ide [project-folder]`

mod actions;
mod app;
mod document;
mod find;
mod highlight;
mod icons;
mod keymap;
mod language;
mod palette;
mod theme;
mod tree;
mod view;

use app::App;
use iced::window;

/// On macOS the native title bar is made transparent so the traffic-light
/// buttons sit inside the app's own title bar, left of the logo.
fn window_settings() -> window::Settings {
    let mut settings = window::Settings {
        size: iced::Size::new(1440.0, 900.0),
        ..Default::default()
    };
    #[cfg(target_os = "macos")]
    {
        settings.platform_specific = window::settings::PlatformSpecific {
            title_hidden: true,
            titlebar_transparent: true,
            fullsize_content_view: true,
        };
    }
    settings
}

/// Entry point: builds the iced application around `App` (boot, update, view,
/// subscription), applies the Thunder Dark theme and default font, registers
/// the embedded fonts and runs the event loop until the window closes.
///
/// # Errors
///
/// Returns iced's error if the window or graphics backend cannot be created.
fn main() -> iced::Result {
    let mut application = iced::application(App::boot, App::update, App::view)
        .title(App::title)
        .subscription(App::subscription)
        .theme(|_: &App| theme::app_theme())
        .default_font(theme::UI)
        .window(window_settings())
        .centered()
        .antialiasing(true);
    for font in theme::FONTS {
        application = application.font(font);
    }
    application.run()
}
