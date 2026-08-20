// SPDX-License-Identifier: GPL-3.0-or-later

use crate::window::DropzoneWindow;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use std::sync::Arc;

pub struct DropzoneApplication {
    app: adw::Application,
    tokio_runtime: Arc<tokio::runtime::Runtime>,
}

impl DropzoneApplication {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let app = adw::Application::builder()
            .application_id("io.github.dragonGR.Dropzone")
            .build();

        let tokio_runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("dropzone-tokio")
                .build()?,
        );

        Ok(Self { app, tokio_runtime })
    }

    pub fn run(&self) -> glib::ExitCode {
        let app = self.app.clone();
        let tokio_handle = self.tokio_runtime.handle().clone();

        app.connect_activate(move |application| {
            let window = DropzoneWindow::new(application, tokio_handle.clone());
            window.present();
        });

        self.app.run()
    }
}
