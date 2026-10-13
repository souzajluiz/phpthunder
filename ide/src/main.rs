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

fn main() -> iced::Result {
    let mut application = iced::application(App::boot, App::update, App::view)
        .title(App::title)
        .subscription(App::subscription)
        .theme(|_: &App| theme::app_theme())
        .default_font(theme::UI)
        .window_size((1440.0, 900.0))
        .centered()
        .antialiasing(true);
    for font in theme::FONTS {
        application = application.font(font);
    }
    application.run()
}
