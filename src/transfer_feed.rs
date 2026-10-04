// SPDX-License-Identifier: GPL-3.0-or-later

use crate::share::transfer::{TransferLifecycleEvent, TransferProgressEvent};
use gtk4::glib;
use tokio::sync::mpsc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferEvent {
    Lifecycle(TransferLifecycleEvent),
    Progress(TransferProgressEvent),
}

/// Delivers one share's transfer events on the thread-default GLib main context.
///
/// Delivery stops when the feed is dropped, so events that a share emits while it
/// shuts down never reach the window after it has moved on.
pub struct TransferFeed {
    task: glib::JoinHandle<()>,
}

impl TransferFeed {
    pub fn spawn(
        mut lifecycle_rx: mpsc::UnboundedReceiver<TransferLifecycleEvent>,
        mut progress_rx: mpsc::Receiver<TransferProgressEvent>,
        mut on_event: impl FnMut(TransferEvent) + 'static,
    ) -> Self {
        let task = glib::spawn_future_local(async move {
            loop {
                // Lifecycle first, so a transfer's Started is handled before its progress.
                let event = tokio::select! {
                    biased;
                    Some(event) = lifecycle_rx.recv() => TransferEvent::Lifecycle(event),
                    Some(event) = progress_rx.recv() => TransferEvent::Progress(event),
                    else => break,
                };
                on_event(event);
            }
        });
        Self { task }
    }
}

impl Drop for TransferFeed {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn run_pending(ctx: &glib::MainContext) {
        while ctx.iteration(false) {}
    }

    #[test]
    fn test_events_stop_arriving_once_the_feed_is_dropped() {
        let ctx = glib::MainContext::new();
        ctx.with_thread_default(|| {
            let (lifecycle_tx, lifecycle_rx) = mpsc::unbounded_channel();
            let (progress_tx, progress_rx) = mpsc::channel(4);
            let seen = Rc::new(RefCell::new(Vec::new()));
            let sink = Rc::clone(&seen);
            let feed = TransferFeed::spawn(lifecycle_rx, progress_rx, move |event| {
                sink.borrow_mut().push(event);
            });

            let started = TransferLifecycleEvent::Started {
                transfer_id: 1,
                file_name: "a".to_string(),
                total_bytes: 10,
            };
            lifecycle_tx.send(started.clone()).expect("feed is running");
            run_pending(&ctx);
            assert_eq!(*seen.borrow(), vec![TransferEvent::Lifecycle(started)]);

            drop(feed);
            let _ = lifecycle_tx.send(TransferLifecycleEvent::Cancelled {
                transfer_id: 1,
                bytes_streamed: 5,
            });
            let _ = progress_tx.try_send(TransferProgressEvent {
                transfer_id: 1,
                bytes_streamed: 5,
            });
            run_pending(&ctx);
            assert_eq!(seen.borrow().len(), 1, "no event may arrive after drop");
        })
        .expect("acquire main context");
    }

    #[test]
    fn test_lifecycle_events_are_delivered_before_queued_progress() {
        let ctx = glib::MainContext::new();
        ctx.with_thread_default(|| {
            let (lifecycle_tx, lifecycle_rx) = mpsc::unbounded_channel();
            let (progress_tx, progress_rx) = mpsc::channel(4);
            let seen = Rc::new(RefCell::new(Vec::new()));
            let sink = Rc::clone(&seen);
            let _feed = TransferFeed::spawn(lifecycle_rx, progress_rx, move |event| {
                sink.borrow_mut().push(event);
            });

            let progress = TransferProgressEvent {
                transfer_id: 1,
                bytes_streamed: 5,
            };
            let started = TransferLifecycleEvent::Started {
                transfer_id: 1,
                file_name: "a".to_string(),
                total_bytes: 10,
            };
            progress_tx.try_send(progress).expect("capacity");
            lifecycle_tx.send(started.clone()).expect("feed is running");
            run_pending(&ctx);

            assert_eq!(
                *seen.borrow(),
                vec![
                    TransferEvent::Lifecycle(started),
                    TransferEvent::Progress(progress)
                ]
            );
        })
        .expect("acquire main context");
    }
}
