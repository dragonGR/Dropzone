// SPDX-License-Identifier: GPL-3.0-or-later

use dropzone::network::interfaces::find_local_lan_ip;
use dropzone::qr::create_qr_widget;
use dropzone::server::routes::start_server;
use dropzone::server::state::ServerHandle;
use dropzone::share::files::SharedFile;
use dropzone::share::session::ShareSession;
use gettextrs::gettext;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{Align, Box, Button, Entry, Label, Orientation, gio};
use libadwaita as adw;
use std::cell::RefCell;
use std::rc::Rc;

pub struct DropzoneWindow {
    window: adw::ApplicationWindow,
    toast_overlay: adw::ToastOverlay,
    view_stack: adw::ViewStack,

    file_name_label: Label,
    file_size_label: Label,
    qr_container: Box,
    url_entry: Entry,

    server_handle: Rc<RefCell<Option<ServerHandle>>>,
    tokio_handle: tokio::runtime::Handle,
}

impl DropzoneWindow {
    pub fn new(app: &adw::Application, tokio_handle: tokio::runtime::Handle) -> Rc<Self> {
        let header_bar = adw::HeaderBar::new();
        let view_stack = adw::ViewStack::new();

        let status_page = adw::StatusPage::builder()
            .icon_name("document-send-symbolic")
            .title(gettext("Dropzone"))
            .description(gettext("Drop files here"))
            .build();

        let choose_button = Button::builder()
            .label(gettext("Choose Files"))
            .css_classes(["suggested-action", "pill"])
            .halign(Align::Center)
            .valign(Align::Center)
            .tooltip_text(gettext("Select a file to share over the local network"))
            .accessible_role(gtk4::AccessibleRole::Button)
            .build();

        status_page.set_child(Some(&choose_button));
        view_stack.add_named(&status_page, Some("idle"));

        let sharing_box = Box::new(Orientation::Vertical, 16);
        sharing_box.set_margin_top(20);
        sharing_box.set_margin_bottom(20);
        sharing_box.set_margin_start(24);
        sharing_box.set_margin_end(24);
        sharing_box.set_halign(Align::Center);
        sharing_box.set_valign(Align::Center);

        let file_name_label = Label::builder()
            .css_classes(["title-2"])
            .wrap(true)
            .justify(gtk4::Justification::Center)
            .max_width_chars(30)
            .ellipsize(gtk4::pango::EllipsizeMode::Middle)
            .build();

        let file_size_label = Label::builder().css_classes(["dim-label"]).build();

        let qr_container = Box::new(Orientation::Vertical, 0);
        qr_container.set_halign(Align::Center);
        qr_container.set_valign(Align::Center);

        let url_box = Box::new(Orientation::Horizontal, 8);
        url_box.set_halign(Align::Center);

        let url_entry = Entry::builder()
            .editable(false)
            .can_focus(true)
            .width_chars(28)
            .tooltip_text(gettext("Temporary capability URL for downloading"))
            .build();

        let copy_button = Button::builder()
            .icon_name("edit-copy-symbolic")
            .tooltip_text(gettext("Copy Link"))
            .accessible_role(gtk4::AccessibleRole::Button)
            .build();
        let copy_label = gettext("Copy Link");
        copy_button.update_property(&[gtk4::accessible::Property::Label(&copy_label)]);

        url_box.append(&url_entry);
        url_box.append(&copy_button);

        let stop_button = Button::builder()
            .label(gettext("Stop Sharing"))
            .css_classes(["destructive-action", "pill"])
            .halign(Align::Center)
            .tooltip_text(gettext("Stop sharing and invalidate the download link"))
            .accessible_role(gtk4::AccessibleRole::Button)
            .build();

        sharing_box.append(&file_name_label);
        sharing_box.append(&file_size_label);
        sharing_box.append(&qr_container);
        sharing_box.append(&url_box);
        sharing_box.append(&stop_button);

        let clamp = adw::Clamp::builder()
            .maximum_size(400)
            .child(&sharing_box)
            .build();

        view_stack.add_named(&clamp, Some("sharing"));

        let content_box = Box::new(Orientation::Vertical, 0);
        content_box.append(&header_bar);

        let toast_overlay = adw::ToastOverlay::new();
        toast_overlay.set_child(Some(&view_stack));
        content_box.append(&toast_overlay);

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title(gettext("Dropzone"))
            .default_width(400)
            .default_height(540)
            .content(&content_box)
            .build();

        let dropzone_window = Rc::new(Self {
            window,
            toast_overlay,
            view_stack,
            file_name_label,
            file_size_label,
            qr_container,
            url_entry,
            server_handle: Rc::new(RefCell::new(None)),
            tokio_handle,
        });

        let self_clone = Rc::clone(&dropzone_window);
        choose_button.connect_clicked(move |_| {
            self_clone.on_choose_files_clicked();
        });

        let self_clone = Rc::clone(&dropzone_window);
        copy_button.connect_clicked(move |_| {
            self_clone.on_copy_link_clicked();
        });

        let self_clone = Rc::clone(&dropzone_window);
        stop_button.connect_clicked(move |_| {
            self_clone.stop_sharing();
        });

        let self_clone = Rc::clone(&dropzone_window);
        dropzone_window.window.connect_close_request(move |_| {
            self_clone.stop_sharing();
            glib::Propagation::Proceed
        });

        dropzone_window
    }

    pub fn present(&self) {
        self.window.present();
    }

    fn show_toast(&self, message: &str) {
        let toast = adw::Toast::new(message);
        self.toast_overlay.add_toast(toast);
    }

    fn on_copy_link_clicked(&self) {
        let text = self.url_entry.text();
        if !text.is_empty()
            && let Some(display) = gtk4::gdk::Display::default()
        {
            display.clipboard().set_text(text.as_str());
            self.show_toast(&gettext("Link copied to clipboard"));
        }
    }

    fn on_choose_files_clicked(&self) {
        let file_dialog = gtk4::FileDialog::new();
        file_dialog.set_title(&gettext("Choose File to Share"));

        let self_ref = self.clone_rc();
        file_dialog.open(
            Some(&self.window),
            gio::Cancellable::NONE,
            move |result| match result {
                Ok(file) => {
                    self_ref.on_file_selected(file);
                }
                Err(err) => {
                    if err.kind::<gtk4::DialogError>() != Some(gtk4::DialogError::Dismissed) {
                        self_ref.show_toast(&gettext("File selection failed"));
                    }
                }
            },
        );
    }

    fn on_file_selected(&self, gio_file: gio::File) {
        let path = match gio_file.path() {
            Some(p) => p,
            None => {
                self.show_toast(&gettext("Couldn’t resolve selected file path"));
                return;
            }
        };

        let shared_file = match SharedFile::from_path(path) {
            Ok(f) => f,
            Err(_) => {
                self.show_toast(&gettext("Couldn’t read selected file"));
                return;
            }
        };

        let lan_ip = match find_local_lan_ip() {
            Ok(ip) => ip,
            Err(_) => {
                self.show_toast(&gettext(
                    "Couldn’t start sharing: No local network connection available",
                ));
                return;
            }
        };

        let session = ShareSession::new(shared_file.clone());
        let token = session.token().as_str().to_string();

        let (sender, receiver) = tokio::sync::oneshot::channel::<Result<ServerHandle, String>>();

        let self_ref = self.clone_rc();
        glib::MainContext::default().spawn_local(async move {
            match receiver.await {
                Ok(Ok(handle)) => {
                    self_ref.on_server_started(handle, &shared_file, &token);
                }
                Ok(Err(_)) => {
                    self_ref.show_toast(&gettext("Couldn’t start local HTTP server"));
                }
                Err(_) => {
                    self_ref.show_toast(&gettext("Server task was cancelled"));
                }
            }
        });

        self.tokio_handle.spawn(async move {
            let result = start_server(lan_ip, session).await;
            let _ = sender.send(result.map_err(|e| e.to_string()));
        });
    }

    fn on_server_started(&self, handle: ServerHandle, file: &SharedFile, token: &str) {
        let share_url = format!("http://{}/s/{}", handle.published_addr, token);

        *self.server_handle.borrow_mut() = Some(handle);

        self.file_name_label.set_text(file.name());
        self.file_size_label.set_text(&file.formatted_size());
        self.url_entry.set_text(&share_url);

        while let Some(child) = self.qr_container.first_child() {
            self.qr_container.remove(&child);
        }

        match create_qr_widget(&share_url) {
            Ok(qr_widget) => {
                self.qr_container.append(&qr_widget);
            }
            Err(_) => {
                self.show_toast(&gettext("Failed to render QR code"));
            }
        }

        self.view_stack.set_visible_child_name("sharing");
    }

    pub fn stop_sharing(&self) {
        let mut handle_opt = self.server_handle.borrow_mut();
        if let Some(mut handle) = handle_opt.take() {
            self.tokio_handle.spawn(async move {
                handle.stop().await;
            });
        }

        while let Some(child) = self.qr_container.first_child() {
            self.qr_container.remove(&child);
        }
        self.url_entry.set_text("");
        self.view_stack.set_visible_child_name("idle");
    }

    fn clone_rc(&self) -> Rc<Self> {
        Rc::new(Self {
            window: self.window.clone(),
            toast_overlay: self.toast_overlay.clone(),
            view_stack: self.view_stack.clone(),
            file_name_label: self.file_name_label.clone(),
            file_size_label: self.file_size_label.clone(),
            qr_container: self.qr_container.clone(),
            url_entry: self.url_entry.clone(),
            server_handle: Rc::clone(&self.server_handle),
            tokio_handle: self.tokio_handle.clone(),
        })
    }
}
