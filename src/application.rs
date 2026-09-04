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
        // SAFETY: Initializing the process locale from environment variables at startup before threads are spawned.
        unsafe {
            gettextrs::setlocale(gettextrs::LocaleCategory::LcAll, "");
        }
        let locale_dir = std::env::var("LOCALEDIR").unwrap_or_else(|_| {
            let candidates = [
                format!("{}/build/po", env!("CARGO_MANIFEST_DIR")),
                "/usr/local/share/locale".to_string(),
                "/app/share/locale".to_string(),
                "/usr/share/locale".to_string(),
            ];
            for dir in &candidates {
                let p = std::path::Path::new(dir);
                if p.join("el/LC_MESSAGES/dropzone.mo").exists() {
                    return dir.clone();
                }
            }
            if std::path::Path::new("/app/share/locale").exists() {
                "/app/share/locale".to_string()
            } else if std::path::Path::new("/usr/local/share/locale").exists() {
                "/usr/local/share/locale".to_string()
            } else {
                "/usr/share/locale".to_string()
            }
        });
        let _ = gettextrs::bindtextdomain("dropzone", &locale_dir);
        let _ = gettextrs::bind_textdomain_codeset("dropzone", "UTF-8");
        let _ = gettextrs::textdomain("dropzone");

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
