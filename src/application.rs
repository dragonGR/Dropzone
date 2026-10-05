// SPDX-License-Identifier: GPL-3.0-or-later

use crate::window::DropzoneWindow;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::ffi::OsString;
use std::path::PathBuf;
use std::rc::Rc;
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
        let locale_dir = locale_dir(
            std::env::var_os("DROPZONE_LOCALEDIR"),
            option_env!("DROPZONE_LOCALEDIR"),
        );
        // Without translations the app still works in English, so a failure here
        // is reported and startup continues.
        if let Err(err) = gettextrs::bindtextdomain(GETTEXT_DOMAIN, locale_dir)
            .and_then(|_| gettextrs::bind_textdomain_codeset(GETTEXT_DOMAIN, "UTF-8"))
            .and_then(|_| gettextrs::textdomain(GETTEXT_DOMAIN))
        {
            eprintln!("Failed to set up translations: {err}");
        }

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
                .version(env!("CARGO_PKG_VERSION"))
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
        let tokio_handle = self.tokio_runtime.handle().clone();
        // The only strong reference to the window controller; its signal handlers
        // hold weak ones.
        let main_window: Rc<RefCell<Option<Rc<DropzoneWindow>>>> = Rc::default();

        self.app.connect_activate(move |application| {
            if let Some(window) = main_window.borrow().as_ref() {
                window.present();
                return;
            }

            let window = DropzoneWindow::new(application, tokio_handle.clone());
            let slot = Rc::downgrade(&main_window);
            window.connect_closed(move || {
                if let Some(slot) = slot.upgrade() {
                    let closed = slot.borrow_mut().take();
                    drop(closed);
                }
            });
            window.present();
            *main_window.borrow_mut() = Some(window);
        });

        self.app.run()
    }
}

const GETTEXT_DOMAIN: &str = "dropzone";

/// Gettext's standard catalog directory, used when the build did not set one.
const SYSTEM_LOCALE_DIR: &str = "/usr/share/locale";

/// Picks the directory holding the compiled translations.
///
/// Meson sets `DROPZONE_LOCALEDIR` at build time to the install location for the
/// configured prefix. Setting the same variable at run time overrides it, which
/// is how a build tree's `po` directory is used during development.
fn locale_dir(runtime_override: Option<OsString>, build_time: Option<&'static str>) -> PathBuf {
    if let Some(dir) = runtime_override.filter(|dir| !dir.is_empty()) {
        return PathBuf::from(dir);
    }
    PathBuf::from(
        build_time
            .filter(|dir| !dir.is_empty())
            .unwrap_or(SYSTEM_LOCALE_DIR),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_override_wins() {
        assert_eq!(
            locale_dir(Some(OsString::from("build/po")), Some("/app/share/locale")),
            PathBuf::from("build/po")
        );
    }

    #[test]
    fn test_build_time_dir_is_used_without_override() {
        assert_eq!(
            locale_dir(None, Some("/app/share/locale")),
            PathBuf::from("/app/share/locale")
        );
    }

    #[test]
    fn test_empty_values_are_ignored() {
        assert_eq!(
            locale_dir(Some(OsString::new()), Some("/app/share/locale")),
            PathBuf::from("/app/share/locale")
        );
        assert_eq!(
            locale_dir(Some(OsString::new()), Some("")),
            PathBuf::from(SYSTEM_LOCALE_DIR)
        );
    }

    #[test]
    fn test_system_dir_is_used_when_not_built_by_meson() {
        assert_eq!(locale_dir(None, None), PathBuf::from(SYSTEM_LOCALE_DIR));
    }
}
