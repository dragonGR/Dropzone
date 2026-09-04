// SPDX-License-Identifier: GPL-3.0-or-later

use crate::window::DropzoneWindow;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
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

        app.connect_startup(|_| {
            let provider = gtk4::CssProvider::new();
            provider.load_from_string(include_str!("style.css"));
            if let Some(display) = gtk4::gdk::Display::default() {
                gtk4::style_context_add_provider_for_display(
                    &display,
                    &provider,
                    gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
                );
            }
        });

        let color_scheme_action = gio::SimpleAction::new_stateful(
            "color-scheme",
            Some(glib::VariantTy::STRING),
            &"default".to_variant(),
        );
        color_scheme_action.connect_activate(|action, parameter| {
            if let Some(param) = parameter.and_then(|p| p.str()) {
                action.set_state(&param.to_variant());
                let style_manager = adw::StyleManager::default();
                match param {
                    "light" => style_manager.set_color_scheme(adw::ColorScheme::ForceLight),
                    "dark" => style_manager.set_color_scheme(adw::ColorScheme::ForceDark),
                    _ => style_manager.set_color_scheme(adw::ColorScheme::Default),
                }
            }
        });
        app.add_action(&color_scheme_action);

        let app_clone = app.clone();
        let about_action = gio::SimpleAction::new("about", None);
        about_action.connect_activate(move |_, _| {
            let active_window = app_clone.active_window();
            let about = adw::AboutDialog::builder()
                .application_name(gettextrs::gettext("Dropzone"))
                .application_icon("io.github.dragonGR.Dropzone")
                .developer_name("dragonGR")
                .version("1.0.0")
                .comments(gettextrs::gettext(
                    "Temporary file sharing over the local network",
                ))
                .website("https://github.com/dragonGR/Dropzone")
                .issue_url("https://github.com/dragonGR/Dropzone/issues")
                .license_type(gtk4::License::Gpl30)
                .build();
            about.present(active_window.as_ref());
        });
        app.add_action(&about_action);

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
