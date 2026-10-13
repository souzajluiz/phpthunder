//! phpthunder IDE: a native PHP editor built on the phpthunder analysis engine.
//!
//! Usage: `phpthunder-ide [project-folder]`

mod app;
mod document;
mod highlight;
mod icons;
mod language;
mod theme;
mod tree;
mod view;

use app::App;
use iced::window;

/// On macOS the native title bar is made transparent so the traffic-light
/// buttons sit inside the app's own title bar, left of the logo.
fn window_settings() -> window::Settings {
    let mut settings = window::Settings { size: iced::Size::new(1440.0, 900.0), ..Default::default() };
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
