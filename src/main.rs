// SPDX-License-Identifier: GPL-3.0-or-later

use dropzone::application::DropzoneApplication;
use gtk4::glib;
use libadwaita as adw;

fn main() -> glib::ExitCode {
    adw::init().expect("Failed to initialize Libadwaita");

    match DropzoneApplication::new() {
        Ok(app) => app.run(),
        Err(err) => {
            eprintln!("Failed to initialize Dropzone application: {}", err);
            glib::ExitCode::FAILURE
        }
    }
}
