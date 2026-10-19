mod ui;

use adw::prelude::*;
use gtk::glib;

fn main() -> glib::ExitCode {
    glib::set_application_name("Learn Star Battle");
    let path = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let app = adw::Application::builder()
        .application_id("io.github.starbattle.Solver")
        .build();
    #[cfg(feature = "capture")]
    if std::env::var_os("STAR_BATTLE_CAPTURE_PATH").is_some() {
        // Visual checks must not activate an already running player's window.
        app.set_flags(gtk::gio::ApplicationFlags::NON_UNIQUE);
    }
    app.connect_activate(move |app| {
        if let Some(window) = app.active_window() {
            window.present();
        } else {
            ui::Desktop::open(app, path.clone());
        }
    });
    app.run_with_args(&["star-battle-solver"])
}
