// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests that need a graphical session. Run with `cargo test --test window_tests -- --ignored`.

use dropzone::window::DropzoneWindow;
use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use std::rc::Rc;

#[test]
#[ignore = "requires a graphical session"]
fn test_window_controller_is_freed_when_its_owner_drops_it() {
    adw::init().expect("initialize Libadwaita");
    let app = adw::Application::builder()
        .application_id("io.github.dragonGR.Dropzone.WindowTests")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(gio::Cancellable::NONE)
        .expect("register application");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("build runtime");

    let window = DropzoneWindow::new(&app, runtime.handle().clone());
    let weak = Rc::downgrade(&window);
    drop(window);

    assert!(
        weak.upgrade().is_none(),
        "signal handlers must not keep the window controller alive"
    );
}
