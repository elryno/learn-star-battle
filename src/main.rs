mod ui;

use adw::prelude::*;
use gtk::{gio, glib};
use std::{cell::RefCell, rc::Rc};

fn main() -> glib::ExitCode {
    glib::set_application_name("Learn Star Battle");
    let app = adw::Application::builder()
        .application_id("io.github.starbattle.Solver")
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();
    #[cfg(feature = "capture")]
    if std::env::var_os("STAR_BATTLE_CAPTURE_PATH").is_some()
        && std::env::var_os("STAR_BATTLE_TEST_UNIQUE").is_none()
    {
        // Visual checks must not activate an already running player's window.
        app.set_flags(app.flags() | gio::ApplicationFlags::NON_UNIQUE);
    }
    let desktop: Rc<RefCell<Option<Rc<ui::Desktop>>>> = Rc::default();
    let active = desktop.clone();
    app.connect_activate(move |app| {
        if active.borrow().is_none() {
            *active.borrow_mut() = Some(ui::Desktop::open(app));
        }
        active.borrow().as_ref().unwrap().present();
    });
    app.connect_open(move |app, files, _| {
        app.activate();
        desktop.borrow().as_ref().unwrap().open_files(files);
    });
    app.run()
}
