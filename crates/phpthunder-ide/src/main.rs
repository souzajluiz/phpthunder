//! PhpThunder IDE: a native PHP editor built on the phpthunder analysis engine.
//!
//! Usage: `phpthunder-ide [project-folder]`

mod app;
mod claude_usage;
mod composer;
mod debug;
mod editor;
mod filetype;
mod formatter;
mod git;
mod i18n;
mod keymap;
mod language;
mod local_history;
mod log;
mod notify;
mod packages;
mod palette;
mod phpstan;
mod project;
mod search;
mod settings;
mod shell_env;
mod template;
mod terminal;
mod testing;
mod ui;
mod watch;

use app::App;
use iced::window;

/// Product name shown in the window title, the custom title bar and the
/// welcome screen.
pub const APP_NAME: &str = "PhpThunder";

/// Side length in pixels of the embedded window icon.
const APP_ICON_SIZE: u32 = 128;

/// Spider app icon as raw 128x128 RGBA pixels, pre-rendered from
/// `assets/brand/app-icon.svg` so no image decoder is needed at runtime.
const APP_ICON_RGBA: &[u8] = include_bytes!("../assets/brand/app-icon-128.rgba");

/// Builds the window icon shown in the title bar and taskbar on Windows and
/// Linux; used by `window_settings`. macOS ignores it and takes the Dock icon
/// from the bundle's `app-icon.icns` instead.
///
/// Returns `None` if the embedded pixel buffer does not match
/// `APP_ICON_SIZE`, so a bad asset falls back to the platform default icon
/// rather than aborting startup.
fn app_icon() -> Option<window::Icon> {
    window::icon::from_rgba(APP_ICON_RGBA.to_vec(), APP_ICON_SIZE, APP_ICON_SIZE).ok()
}

/// On macOS the native title bar is made transparent so the traffic-light
/// buttons sit inside the app's own title bar, left of the logo.
fn window_settings() -> window::Settings {
    let mut settings = window::Settings {
        size: iced::Size::new(1440.0, 900.0),
        icon: app_icon(),
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
/// subscription), applies the Thunder Purple theme and default font, registers
/// the embedded fonts and runs the event loop until the window closes.
///
/// # Errors
///
/// Returns iced's error if the window or graphics backend cannot be created.
fn main() -> iced::Result {
    log::install_panic_hook();
    shell_env::import_login_path();
    i18n::init_locale(i18n::detect_locale());
    let mut application = iced::application(App::boot, App::update, App::view)
        .title(App::title)
        .subscription(App::subscription)
        .theme(|_: &App| ui::app_theme())
        .scale_factor(|app: &App| app.zoom)
        .default_font(ui::UI)
        .window(window_settings())
        .centered()
        .antialiasing(true);
    for font in ui::FONTS {
        application = application.font(font);
    }
    application.run()
}
