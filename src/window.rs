// SPDX-License-Identifier: GPL-3.0-or-later

use crate::network::interfaces::find_local_lan_ip;
use crate::qr::create_qr_widget;
use crate::server::connections::ServerLimits;
use crate::server::routes::start_server;
use crate::server::state::ServerHandle;
use crate::share::files::{SharedFile, format_file_size};
use crate::share::phase::{SharePhase, StartAttempt};
use crate::share::session::ShareSession;
use crate::share::token::FileId;
use crate::share::transfer::{TransferLifecycleEvent, TransferProgressEvent};
use crate::transfer_feed::{TransferEvent, TransferFeed};
use gettextrs::{gettext, ngettext};
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Align, Box, Button, DropTarget, Entry, Image, Label, MenuButton, Orientation, Popover,
    ProgressBar, Separator, ToggleButton, gdk, gio,
};
use libadwaita as adw;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Clone)]
struct ActiveTransferState {
    total_bytes: u64,
    bytes_streamed: u64,
}

/// Everything kept alive while sharing. Dropping it stops event delivery and
/// shuts the server down.
struct ActiveShare {
    server: ServerHandle,
    feed: TransferFeed,
}

pub struct DropzoneWindow {
    window: adw::ApplicationWindow,
    toast_overlay: adw::ToastOverlay,
    view_stack: adw::ViewStack,
    choose_button: Button,

    file_name_label: Label,
    file_size_label: Label,
    qr_container: Box,
    url_entry: Entry,
    transfer_status_label: Label,
    transfer_progress_bar: ProgressBar,
    active_transfers: RefCell<HashMap<u64, ActiveTransferState>>,

    share: RefCell<SharePhase<ActiveShare>>,
    tokio_handle: tokio::runtime::Handle,
}

impl DropzoneWindow {
    pub fn new(app: &adw::Application, tokio_handle: tokio::runtime::Handle) -> Rc<Self> {
        let header_bar = adw::HeaderBar::new();

        let popover = Popover::new();
        popover.add_css_class("dropzone-menu-popover");

        let popover_box = Box::new(Orientation::Vertical, 10);
        popover_box.set_margin_start(10);
        popover_box.set_margin_end(10);
        popover_box.set_margin_top(10);
        popover_box.set_margin_bottom(10);

        let appearance_label = Label::builder()
            .label(gettext("Appearance"))
            .css_classes(["dim-label", "caption-heading"])
            .halign(Align::Start)
            .build();
        popover_box.append(&appearance_label);

        let theme_box = Box::new(Orientation::Horizontal, 6);
        theme_box.set_homogeneous(true);

        let btn_system = ToggleButton::builder()
            .css_classes(["theme-card"])
            .tooltip_text(gettext("System"))
            .build();
        let sys_box = Box::new(Orientation::Vertical, 4);
        let sys_icon = Image::builder()
            .icon_name("preferences-desktop-display-symbolic")
            .pixel_size(20)
            .css_classes(["theme-icon-system"])
            .build();
        let sys_label = Label::builder()
            .label(gettext("System"))
            .css_classes(["caption", "theme-label"])
            .build();
        sys_box.append(&sys_icon);
        sys_box.append(&sys_label);
        btn_system.set_child(Some(&sys_box));

        let btn_light = ToggleButton::builder()
            .css_classes(["theme-card"])
            .tooltip_text(gettext("Light"))
            .group(&btn_system)
            .build();
        let light_box = Box::new(Orientation::Vertical, 4);
        let light_icon = Image::builder()
            .icon_name("weather-clear-symbolic")
            .pixel_size(20)
            .css_classes(["theme-icon-light"])
            .build();
        let light_label = Label::builder()
            .label(gettext("Light"))
            .css_classes(["caption", "theme-label"])
            .build();
        light_box.append(&light_icon);
        light_box.append(&light_label);
        btn_light.set_child(Some(&light_box));

        let btn_dark = ToggleButton::builder()
            .css_classes(["theme-card"])
            .tooltip_text(gettext("Dark"))
            .group(&btn_system)
            .build();
        let dark_box = Box::new(Orientation::Vertical, 4);
        let dark_icon = Image::builder()
            .icon_name("weather-clear-night-symbolic")
            .pixel_size(20)
            .css_classes(["theme-icon-dark"])
            .build();
        let dark_label = Label::builder()
            .label(gettext("Dark"))
            .css_classes(["caption", "theme-label"])
            .build();
        dark_box.append(&dark_icon);
        dark_box.append(&dark_label);
        btn_dark.set_child(Some(&dark_box));

        let style_manager = adw::StyleManager::default();
        match style_manager.color_scheme() {
            adw::ColorScheme::ForceLight => btn_light.set_active(true),
            adw::ColorScheme::ForceDark => btn_dark.set_active(true),
            _ => btn_system.set_active(true),
        }

        btn_system.connect_toggled(|btn| {
            if btn.is_active() {
                adw::StyleManager::default().set_color_scheme(adw::ColorScheme::Default);
            }
        });
        btn_light.connect_toggled(|btn| {
            if btn.is_active() {
                adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);
            }
        });
        btn_dark.connect_toggled(|btn| {
            if btn.is_active() {
                adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
            }
        });

        theme_box.append(&btn_system);
        theme_box.append(&btn_light);
        theme_box.append(&btn_dark);
        popover_box.append(&theme_box);

        let separator = Separator::new(Orientation::Horizontal);
        popover_box.append(&separator);

        let about_button = Button::builder()
            .css_classes(["flat", "menu-action-button"])
            .halign(Align::Fill)
            .build();
        let about_row = Box::new(Orientation::Horizontal, 10);
        let about_icon = Image::from_icon_name("help-about-symbolic");
        let about_text = Label::builder()
            .label(gettext("About Dropzone"))
            .halign(Align::Start)
            .hexpand(true)
            .build();
        about_row.append(&about_icon);
        about_row.append(&about_text);
        about_button.set_child(Some(&about_row));

        let popover_clone = popover.clone();
        let app_clone = app.clone();
        about_button.connect_clicked(move |_| {
            popover_clone.popdown();
            app_clone.activate_action("about", None);
        });
        popover_box.append(&about_button);

        popover.set_child(Some(&popover_box));

        let menu_button = MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .popover(&popover)
            .tooltip_text(gettext("Main Menu"))
            .primary(true)
            .accessible_role(gtk4::AccessibleRole::Button)
            .build();
        header_bar.pack_end(&menu_button);

        let view_stack = adw::ViewStack::new();

        let status_page = adw::StatusPage::builder()
            .icon_name("document-send-symbolic")
            .title(gettext("Dropzone"))
            .description(gettext("Drop files here"))
            .build();
        status_page.add_css_class("dropzone-idle-page");

        let choose_button = Button::builder()
            .label(gettext("Choose Files"))
            .css_classes(["suggested-action", "pill", "choose-button"])
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
        sharing_box.set_halign(Align::Fill);
        sharing_box.set_hexpand(true);
        sharing_box.set_valign(Align::Center);

        let file_name_label = Label::builder()
            .css_classes(["title-2"])
            .wrap(true)
            .justify(gtk4::Justification::Center)
            .halign(Align::Center)
            .max_width_chars(30)
            .ellipsize(gtk4::pango::EllipsizeMode::Middle)
            .build();

        let file_size_label = Label::builder()
            .css_classes(["dim-label"])
            .halign(Align::Center)
            .build();

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

        let transfer_box = Box::new(Orientation::Vertical, 6);
        transfer_box.set_halign(Align::Fill);
        transfer_box.set_hexpand(true);
        transfer_box.set_margin_top(4);
        transfer_box.set_margin_bottom(4);

        let transfer_status_label = Label::builder()
            .label(gettext("Waiting for receiver…"))
            .css_classes(["dim-label", "caption", "transfer-status"])
            .halign(Align::Center)
            .justify(gtk4::Justification::Center)
            .ellipsize(gtk4::pango::EllipsizeMode::Middle)
            .max_width_chars(36)
            .build();

        let transfer_progress_bar = ProgressBar::builder()
            .halign(Align::Fill)
            .hexpand(true)
            .fraction(0.0)
            .visible(true)
            .build();

        transfer_box.append(&transfer_status_label);
        transfer_box.append(&transfer_progress_bar);

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
        sharing_box.append(&transfer_box);
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
            choose_button: choose_button.clone(),
            file_name_label,
            file_size_label,
            qr_container,
            url_entry,
            transfer_status_label,
            transfer_progress_bar,
            active_transfers: RefCell::new(HashMap::new()),
            share: RefCell::new(SharePhase::default()),
            tokio_handle,
        });

        let weak = Rc::downgrade(&dropzone_window);
        choose_button.connect_clicked(move |_| {
            if let Some(window) = weak.upgrade() {
                window.on_choose_files_clicked();
            }
        });

        let drop_target = DropTarget::new(glib::Type::INVALID, gdk::DragAction::COPY);
        drop_target.set_types(&[gdk::FileList::static_type(), gio::File::static_type()]);

        let weak = Rc::downgrade(&dropzone_window);
        let status_page_clone = status_page.clone();
        drop_target.connect_enter(move |_target, _x, _y| {
            if weak
                .upgrade()
                .is_some_and(|window| window.share.borrow().is_idle())
            {
                status_page_clone.add_css_class("drag-hover");
                gdk::DragAction::COPY
            } else {
                gdk::DragAction::empty()
            }
        });

        let status_page_clone = status_page.clone();
        drop_target.connect_leave(move |_target| {
            status_page_clone.remove_css_class("drag-hover");
        });

        let weak = Rc::downgrade(&dropzone_window);
        let status_page_clone = status_page.clone();
        drop_target.connect_drop(move |_target, value, _x, _y| -> bool {
            status_page_clone.remove_css_class("drag-hover");

            let Some(window) = weak.upgrade() else {
                return false;
            };
            let gio_file = if let Ok(file_list) = value.get::<gdk::FileList>() {
                file_list.files().into_iter().next()
            } else {
                value.get::<gio::File>().ok()
            };
            match gio_file {
                Some(file) => window.start_sharing(file),
                None => false,
            }
        });

        dropzone_window.window.add_controller(drop_target);

        let weak = Rc::downgrade(&dropzone_window);
        copy_button.connect_clicked(move |_| {
            if let Some(window) = weak.upgrade() {
                window.on_copy_link_clicked();
            }
        });

        let weak = Rc::downgrade(&dropzone_window);
        stop_button.connect_clicked(move |_| {
            if let Some(window) = weak.upgrade() {
                window.stop_sharing();
            }
        });

        let weak = Rc::downgrade(&dropzone_window);
        dropzone_window.window.connect_close_request(move |_| {
            if let Some(window) = weak.upgrade() {
                window.stop_sharing();
            }
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

    /// Calls `f` when the user closes the window, after sharing has been stopped.
    pub fn connect_closed(&self, f: impl Fn() + 'static) {
        self.window.connect_close_request(move |_| {
            f();
            glib::Propagation::Proceed
        });
    }

    fn on_choose_files_clicked(self: &Rc<Self>) {
        let file_dialog = gtk4::FileDialog::new();
        file_dialog.set_title(&gettext("Choose File to Share"));

        let weak = Rc::downgrade(self);
        file_dialog.open(Some(&self.window), gio::Cancellable::NONE, move |result| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            match result {
                Ok(file) => {
                    window.start_sharing(file);
                }
                Err(err) => {
                    if err.kind::<gtk4::DialogError>() != Some(gtk4::DialogError::Dismissed) {
                        window.show_toast(&gettext("File selection failed"));
                    }
                }
            }
        });
    }

    /// Starts sharing `file` unless a share is already starting or running.
    /// Returns whether a start was begun.
    fn start_sharing(self: &Rc<Self>, file: gio::File) -> bool {
        let Some(attempt) = self.share.borrow_mut().begin_start() else {
            self.show_toast(&gettext(
                "A sharing session is already active. Please stop it first.",
            ));
            return false;
        };
        self.choose_button.set_sensitive(false);

        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            // Queried asynchronously: on network or portal-backed files this may
            // take long enough to freeze the window if done synchronously.
            let info = file
                .query_info_future(
                    SHARED_FILE_ATTRIBUTES,
                    gio::FileQueryInfoFlags::NONE,
                    glib::Priority::DEFAULT,
                )
                .await;

            let Some(window) = weak.upgrade() else {
                return;
            };
            let shared_file = match info
                .map_err(|_| FileRejection::Unreadable)
                .and_then(|info| shared_file_from_info(&file, &info))
            {
                Ok(shared_file) => shared_file,
                Err(rejection) => {
                    window.abandon_start(attempt, &rejection.message());
                    return;
                }
            };
            let Ok(lan_ip) = find_local_lan_ip() else {
                window.abandon_start(
                    attempt,
                    &gettext("Couldn’t start sharing: No local network connection available"),
                );
                return;
            };

            let session = ShareSession::new(shared_file.clone());
            let token = session.token().as_str().to_string();
            let (lifecycle_tx, lifecycle_rx) = mpsc::unbounded_channel();
            let (progress_tx, progress_rx) = mpsc::channel(PROGRESS_EVENT_CAPACITY);
            let (started_tx, started_rx) = oneshot::channel();
            window.tokio_handle.spawn(async move {
                let started = start_server(
                    lan_ip,
                    session,
                    ServerLimits::default(),
                    lifecycle_tx,
                    progress_tx,
                )
                .await;
                // If the receiver is gone, dropping the handle shuts the server down.
                let _ = started_tx.send(started);
            });
            drop(window);

            let started = started_rx.await;
            let Some(window) = weak.upgrade() else {
                return;
            };
            match started {
                Ok(Ok(server)) => {
                    window.finish_start(
                        attempt,
                        server,
                        lifecycle_rx,
                        progress_rx,
                        &shared_file,
                        &token,
                    );
                }
                Ok(Err(_)) => {
                    window.abandon_start(attempt, &gettext("Couldn’t start local HTTP server"));
                }
                Err(_) => {
                    window.abandon_start(attempt, &gettext("Server task was cancelled"));
                }
            }
        });
        true
    }

    fn abandon_start(&self, attempt: StartAttempt, message: &str) {
        let current = self.share.borrow_mut().fail_start(attempt);
        if current {
            self.choose_button.set_sensitive(true);
            self.show_toast(message);
        }
    }

    fn finish_start(
        self: &Rc<Self>,
        attempt: StartAttempt,
        server: ServerHandle,
        lifecycle_rx: mpsc::UnboundedReceiver<TransferLifecycleEvent>,
        progress_rx: mpsc::Receiver<TransferProgressEvent>,
        file: &SharedFile,
        token: &str,
    ) {
        let share_url = format!("http://{}/s/{}", server.published_addr, token);

        let weak = Rc::downgrade(self);
        let feed = TransferFeed::spawn(lifecycle_rx, progress_rx, move |event| {
            if let Some(window) = weak.upgrade() {
                window.on_transfer_event(event);
            }
        });

        let outcome = self
            .share
            .borrow_mut()
            .finish_start(attempt, ActiveShare { server, feed });
        if let Err(stale) = outcome {
            // The user stopped or closed the window while the server was starting.
            self.shut_down(stale);
            return;
        }

        self.file_name_label.set_text(file.name());
        self.file_size_label.set_text(&file.formatted_size());
        self.url_entry.set_text(&share_url);

        self.active_transfers.borrow_mut().clear();
        self.transfer_status_label
            .set_text(&gettext("Waiting for receiver…"));
        self.transfer_status_label
            .set_css_classes(&["dim-label", "caption", "transfer-status"]);
        self.transfer_progress_bar.set_fraction(0.0);
        self.transfer_progress_bar.set_visible(true);

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

    fn shut_down(&self, share: ActiveShare) {
        let ActiveShare { mut server, feed } = share;
        drop(feed);
        self.tokio_handle.spawn(async move {
            server.stop().await;
        });
    }

    fn on_transfer_event(&self, event: TransferEvent) {
        match event {
            TransferEvent::Lifecycle(event) => self.on_transfer_lifecycle(event),
            TransferEvent::Progress(event) => self.on_transfer_progress(event),
        }
    }

    fn on_transfer_lifecycle(&self, event: TransferLifecycleEvent) {
        match event {
            TransferLifecycleEvent::Started {
                transfer_id,
                file_name: _,
                total_bytes,
            } => {
                self.active_transfers.borrow_mut().insert(
                    transfer_id,
                    ActiveTransferState {
                        total_bytes,
                        bytes_streamed: 0,
                    },
                );
                self.update_transfer_ui();
            }
            TransferLifecycleEvent::Completed { transfer_id } => {
                self.active_transfers.borrow_mut().remove(&transfer_id);
                if self.active_transfers.borrow().is_empty() {
                    self.transfer_status_label
                        .set_text(&gettext("Download completed"));
                    self.transfer_status_label
                        .set_css_classes(&["caption", "transfer-status"]);
                    self.transfer_progress_bar.set_visible(true);
                    self.transfer_progress_bar.set_fraction(1.0);
                } else {
                    self.update_transfer_ui();
                }
            }
            TransferLifecycleEvent::Cancelled { transfer_id, .. } => {
                self.active_transfers.borrow_mut().remove(&transfer_id);
                if self.active_transfers.borrow().is_empty() {
                    self.transfer_status_label
                        .set_text(&gettext("Download cancelled"));
                    self.transfer_status_label.set_css_classes(&[
                        "dim-label",
                        "caption",
                        "transfer-status",
                    ]);
                    self.transfer_progress_bar.set_fraction(0.0);
                    self.transfer_progress_bar.set_visible(true);
                } else {
                    self.update_transfer_ui();
                }
            }
            TransferLifecycleEvent::Failed { transfer_id, .. } => {
                self.active_transfers.borrow_mut().remove(&transfer_id);
                if self.active_transfers.borrow().is_empty() {
                    self.transfer_status_label
                        .set_text(&gettext("Download failed"));
                    self.transfer_status_label.set_css_classes(&[
                        "dim-label",
                        "caption",
                        "transfer-status",
                    ]);
                    self.transfer_progress_bar.set_fraction(0.0);
                    self.transfer_progress_bar.set_visible(true);
                } else {
                    self.update_transfer_ui();
                }
            }
        }
    }

    fn on_transfer_progress(&self, event: TransferProgressEvent) {
        let mut transfers = self.active_transfers.borrow_mut();
        if let Some(state) = transfers.get_mut(&event.transfer_id) {
            state.bytes_streamed = event.bytes_streamed;
            drop(transfers);
            self.update_transfer_ui();
        }
    }

    fn update_transfer_ui(&self) {
        let transfers = self.active_transfers.borrow();
        match transfers.len() {
            0 => {
                self.transfer_status_label
                    .set_text(&gettext("Waiting for receiver…"));
                self.transfer_status_label.set_css_classes(&[
                    "dim-label",
                    "caption",
                    "transfer-status",
                ]);
                self.transfer_progress_bar.set_fraction(0.0);
                self.transfer_progress_bar.set_visible(true);
            }
            1 => {
                let (_, transfer) = transfers.iter().next().unwrap();
                let fraction = if transfer.total_bytes > 0 {
                    (transfer.bytes_streamed as f64 / transfer.total_bytes as f64).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                let percent = (fraction * 100.0).round() as u64;
                let template = gettext("{streamed} / {total} ({percent}%)");
                let formatted = format_transfer_status(
                    &template,
                    &format_file_size(transfer.bytes_streamed),
                    &format_file_size(transfer.total_bytes),
                    percent,
                );
                self.transfer_status_label.set_text(&formatted);
                self.transfer_status_label
                    .set_css_classes(&["caption", "transfer-status"]);
                self.transfer_progress_bar.set_fraction(fraction);
                self.transfer_progress_bar.set_visible(true);
            }
            count => {
                let n = count.min(u32::MAX as usize) as u32;
                let plural_template = ngettext(
                    "{count} download in progress",
                    "{count} downloads in progress",
                    n,
                );
                let text = plural_template.replace("{count}", &n.to_string());
                self.transfer_status_label.set_text(&text);
                self.transfer_status_label
                    .set_css_classes(&["caption", "transfer-status"]);
                self.transfer_progress_bar.set_visible(false);
            }
        }
    }

    pub fn stop_sharing(&self) {
        let share = self.share.borrow_mut().stop();
        if let Some(share) = share {
            self.shut_down(share);
        }
        self.choose_button.set_sensitive(true);

        self.active_transfers.borrow_mut().clear();
        self.transfer_status_label
            .set_text(&gettext("Waiting for receiver…"));
        self.transfer_status_label
            .set_css_classes(&["dim-label", "caption", "transfer-status"]);
        self.transfer_progress_bar.set_fraction(0.0);
        self.transfer_progress_bar.set_visible(true);

        while let Some(child) = self.qr_container.first_child() {
            self.qr_container.remove(&child);
        }
        self.url_entry.set_text("");
        self.view_stack.set_visible_child_name("idle");
    }
}

const SHARED_FILE_ATTRIBUTES: &str = "standard::type,standard::size,standard::display-name";
const PROGRESS_EVENT_CAPACITY: usize = 64;

/// Why a selected file cannot be shared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileRejection {
    Directory,
    NotRegularFile,
    NoLocalPath,
    InvalidSize,
    Unreadable,
}

impl FileRejection {
    fn message(self) -> String {
        match self {
            Self::Directory => {
                gettext("Directories cannot be shared directly. Please select a file.")
            }
            Self::NoLocalPath => gettext("Couldn’t resolve selected file path"),
            Self::NotRegularFile | Self::InvalidSize | Self::Unreadable => {
                gettext("Couldn’t read selected file")
            }
        }
    }
}

/// Builds the shared file from metadata queried asynchronously through GIO.
fn shared_file_from_info(
    file: &gio::File,
    info: &gio::FileInfo,
) -> Result<SharedFile, FileRejection> {
    match info.file_type() {
        gio::FileType::Regular => {}
        gio::FileType::Directory => return Err(FileRejection::Directory),
        _ => return Err(FileRejection::NotRegularFile),
    }
    let path = file.path().ok_or(FileRejection::NoLocalPath)?;
    let size = u64::try_from(info.size()).map_err(|_| FileRejection::InvalidSize)?;
    Ok(SharedFile::new(
        FileId::new_random(),
        info.display_name().to_string(),
        path,
        size,
    ))
}

/// Formats a transfer progress string from a translated template with named placeholders.
///
/// Translators may freely reorder `{streamed}`, `{total}`, and `{percent}` to fit natural language syntax.
pub fn format_transfer_status(template: &str, streamed: &str, total: &str, percent: u64) -> String {
    template
        .replace("{streamed}", streamed)
        .replace("{total}", total)
        .replace("{percent}", &percent.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn pump_until(
        ctx: &glib::MainContext,
        timeout: Duration,
        mut done: impl FnMut() -> bool,
    ) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            while ctx.iteration(false) {}
            if done() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    fn http_status(runtime: &tokio::runtime::Runtime, url: &str) -> Option<String> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let rest = url.strip_prefix("http://")?;
        let (authority, path) = rest.split_at(rest.find('/')?);
        runtime.block_on(async {
            let mut stream = tokio::net::TcpStream::connect(authority).await.ok()?;
            let request =
                format!("GET {path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n");
            stream.write_all(request.as_bytes()).await.ok()?;
            let mut head = [0u8; 12];
            stream.read_exact(&mut head).await.ok()?;
            Some(String::from_utf8_lossy(&head).into_owned())
        })
    }

    /// Drives the real window: start, refuse a concurrent start, serve, stop, and
    /// stop while starting.
    #[test]
    #[ignore = "requires a graphical session and a LAN address"]
    fn test_window_share_lifecycle() {
        adw::init().expect("initialize Libadwaita");
        let app = adw::Application::builder()
            .application_id("io.github.dragonGR.Dropzone.LifecycleTest")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(gio::Cancellable::NONE)
            .expect("register application");
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("build runtime");
        let ctx = glib::MainContext::default();

        let path = std::env::temp_dir().join("dropzone_window_lifecycle.txt");
        std::fs::write(&path, b"window lifecycle").expect("write file");
        let window = DropzoneWindow::new(&app, runtime.handle().clone());

        assert!(window.start_sharing(gio::File::for_path(&path)));
        assert!(
            !window.start_sharing(gio::File::for_path(&path)),
            "a second start while starting must be refused"
        );
        assert!(!window.choose_button.is_sensitive());

        assert!(pump_until(&ctx, Duration::from_secs(5), || !window
            .url_entry
            .text()
            .is_empty()));
        let url = window.url_entry.text().to_string();
        assert_eq!(http_status(&runtime, &url).as_deref(), Some("HTTP/1.1 200"));

        window.stop_sharing();
        assert!(window.share.borrow().is_idle());
        assert!(window.choose_button.is_sensitive());
        let refused = pump_until(&ctx, Duration::from_secs(2), || {
            http_status(&runtime, &url).is_none()
        });
        assert!(refused, "the stopped share's URL must stop working");

        // Stop while the server is still starting: the late server must not take over the window.
        assert!(window.start_sharing(gio::File::for_path(&path)));
        window.stop_sharing();
        pump_until(&ctx, Duration::from_millis(500), || false);
        assert!(window.share.borrow().is_idle());
        assert!(window.url_entry.text().is_empty());
        assert_eq!(
            window.view_stack.visible_child_name().as_deref(),
            Some("idle")
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_format_transfer_status_reordering_and_placeholders() {
        let template_en = "{streamed} / {total} ({percent}%)";
        assert_eq!(
            format_transfer_status(template_en, "10 MB", "100 MB", 10),
            "10 MB / 100 MB (10%)"
        );

        // Translator reordering placeholders (e.g. prefix percentage)
        let template_reordered = "({percent}%) {streamed} of {total}";
        assert_eq!(
            format_transfer_status(template_reordered, "10 MB", "100 MB", 10),
            "(10%) 10 MB of 100 MB"
        );

        // Translator omitting percent if desired
        let template_no_percent = "{streamed} / {total}";
        assert_eq!(
            format_transfer_status(template_no_percent, "10 MB", "100 MB", 10),
            "10 MB / 100 MB"
        );
    }

    fn file_info(file_type: gio::FileType, size: i64) -> gio::FileInfo {
        let info = gio::FileInfo::new();
        info.set_file_type(file_type);
        info.set_size(size);
        info.set_display_name("Holiday Photo.jpg");
        info
    }

    #[test]
    fn test_regular_file_is_accepted_with_display_name_and_size() {
        let file = gio::File::for_path("/run/user/1000/doc/1a2b3c/Holiday Photo.jpg");
        let shared = shared_file_from_info(&file, &file_info(gio::FileType::Regular, 4096))
            .expect("regular file");
        assert_eq!(shared.name(), "Holiday Photo.jpg");
        assert_eq!(shared.size_bytes(), 4096);
        assert_eq!(
            shared.path(),
            std::path::Path::new("/run/user/1000/doc/1a2b3c/Holiday Photo.jpg")
        );
    }

    #[test]
    fn test_directory_is_rejected() {
        let file = gio::File::for_path("/home/user/Pictures");
        assert_eq!(
            shared_file_from_info(&file, &file_info(gio::FileType::Directory, 0)),
            Err(FileRejection::Directory)
        );
    }

    #[test]
    fn test_special_file_is_rejected() {
        let file = gio::File::for_path("/tmp/fifo");
        assert_eq!(
            shared_file_from_info(&file, &file_info(gio::FileType::Special, 0)),
            Err(FileRejection::NotRegularFile)
        );
    }

    #[test]
    fn test_file_without_local_path_is_rejected() {
        let file = gio::File::for_uri("http://example.com/file.bin");
        assert_eq!(
            shared_file_from_info(&file, &file_info(gio::FileType::Regular, 1)),
            Err(FileRejection::NoLocalPath)
        );
    }

    #[test]
    fn test_negative_size_is_rejected() {
        let file = gio::File::for_path("/tmp/file.bin");
        assert_eq!(
            shared_file_from_info(&file, &file_info(gio::FileType::Regular, -1)),
            Err(FileRejection::InvalidSize)
        );
    }
}
