// SPDX-License-Identifier: GPL-3.0-or-later

/// Lifecycle event for a transfer session.
///
/// Lifecycle events are not dropped because of channel capacity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferLifecycleEvent {
    Started {
        transfer_id: u64,
        file_name: String,
        total_bytes: u64,
    },
    Completed {
        transfer_id: u64,
    },
    Cancelled {
        transfer_id: u64,
        bytes_streamed: u64,
    },
    Failed {
        transfer_id: u64,
        bytes_streamed: u64,
    },
}

impl TransferLifecycleEvent {
    pub fn transfer_id(&self) -> u64 {
        match self {
            Self::Started { transfer_id, .. }
            | Self::Completed { transfer_id }
            | Self::Cancelled { transfer_id, .. }
            | Self::Failed { transfer_id, .. } => *transfer_id,
        }
    }
}

/// High-frequency intermediate progress event.
///
/// Transmitted over a bounded channel using non-blocking best-effort updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferProgressEvent {
    pub transfer_id: u64,
    pub bytes_streamed: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lifecycle_event_accessors() {
        let started = TransferLifecycleEvent::Started {
            transfer_id: 42,
            file_name: "test.iso".to_string(),
            total_bytes: 1024,
        };
        assert_eq!(started.transfer_id(), 42);

        let completed = TransferLifecycleEvent::Completed { transfer_id: 42 };
        assert_eq!(completed.transfer_id(), 42);

        let cancelled = TransferLifecycleEvent::Cancelled {
            transfer_id: 42,
            bytes_streamed: 512,
        };
        assert_eq!(cancelled.transfer_id(), 42);

        let failed = TransferLifecycleEvent::Failed {
            transfer_id: 42,
            bytes_streamed: 256,
        };
        assert_eq!(failed.transfer_id(), 42);
    }

    #[test]
    fn test_progress_event() {
        let progress = TransferProgressEvent {
            transfer_id: 7,
            bytes_streamed: 8192,
        };
        assert_eq!(progress.transfer_id, 7);
        assert_eq!(progress.bytes_streamed, 8192);
    }
}
