// SPDX-License-Identifier: GPL-3.0-or-later

use crate::share::transfer::{TransferLifecycleEvent, TransferProgressEvent};
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, ReadBuf};
use tokio::sync::mpsc;
use tokio_util::sync::{CancellationToken, WaitForCancellationFutureOwned};

/// An `AsyncRead` adapter that monitors stream consumption into the HTTP pipeline.
///
/// Lifecycle events are transmitted over an unbounded channel so they are not dropped
/// due to channel capacity. Progress updates are transmitted over a bounded channel
/// with time and byte throttling to prevent UI flooding.
///
/// Cancellation is given strict priority on every `poll_read` call.
pub struct ProgressReader<R> {
    inner: R,
    transfer_id: u64,
    total_bytes: u64,
    bytes_streamed: u64,
    last_reported_bytes: u64,
    last_reported_time: Instant,
    lifecycle_tx: mpsc::UnboundedSender<TransferLifecycleEvent>,
    progress_tx: mpsc::Sender<TransferProgressEvent>,
    cancel_token: CancellationToken,
    cancel_future: Pin<Box<WaitForCancellationFutureOwned>>,
    completed: bool,
}

impl<R> ProgressReader<R> {
    pub fn new(
        inner: R,
        transfer_id: u64,
        total_bytes: u64,
        lifecycle_tx: mpsc::UnboundedSender<TransferLifecycleEvent>,
        progress_tx: mpsc::Sender<TransferProgressEvent>,
        cancel_token: CancellationToken,
    ) -> Self {
        Self {
            inner,
            transfer_id,
            total_bytes,
            bytes_streamed: 0,
            last_reported_bytes: 0,
            last_reported_time: Instant::now(),
            lifecycle_tx,
            progress_tx,
            cancel_future: Box::pin(cancel_token.clone().cancelled_owned()),
            cancel_token,
            completed: false,
        }
    }

    pub fn bytes_streamed(&self) -> u64 {
        self.bytes_streamed
    }

    pub fn is_completed(&self) -> bool {
        self.completed
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for ProgressReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // 1. Give cancellation priority: check token and poll/register cancellation waker FIRST
        if self.cancel_token.is_cancelled() || self.cancel_future.as_mut().poll(cx).is_ready() {
            if !self.completed {
                self.completed = true;
                let _ = self.lifecycle_tx.send(TransferLifecycleEvent::Cancelled {
                    transfer_id: self.transfer_id,
                    bytes_streamed: self.bytes_streamed,
                });
            }
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "Sharing session was stopped",
            )));
        }

        // 2. Poll the inner reader
        let before = buf.filled().len();
        match Pin::new(&mut self.inner).poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                let bytes_read = (buf.filled().len() - before) as u64;
                if bytes_read > 0 {
                    self.bytes_streamed += bytes_read;
                    // Refresh cancellation future for subsequent polls
                    self.cancel_future = Box::pin(self.cancel_token.clone().cancelled_owned());

                    let now = Instant::now();
                    let time_elapsed =
                        now.duration_since(self.last_reported_time) >= Duration::from_millis(150);
                    let step = (self.total_bytes / 100).clamp(64 * 1024, 1024 * 1024);
                    let bytes_elapsed = self.bytes_streamed - self.last_reported_bytes >= step;

                    if (time_elapsed && bytes_elapsed) || self.bytes_streamed == self.total_bytes {
                        self.last_reported_time = now;
                        self.last_reported_bytes = self.bytes_streamed;
                        let _ = self.progress_tx.try_send(TransferProgressEvent {
                            transfer_id: self.transfer_id,
                            bytes_streamed: self.bytes_streamed,
                        });
                    }

                    // If we reached the expected total bytes, verify clean EOF from inner reader
                    if self.bytes_streamed == self.total_bytes && !self.completed {
                        let mut eof_check = [0u8; 1];
                        let mut check_buf = ReadBuf::new(&mut eof_check);
                        if let Poll::Ready(res) =
                            Pin::new(&mut self.inner).poll_read(cx, &mut check_buf)
                        {
                            match res {
                                Ok(()) if check_buf.filled().is_empty() => {
                                    // Clean EOF verified at expected byte count
                                    self.completed = true;
                                    let _ =
                                        self.lifecycle_tx.send(TransferLifecycleEvent::Completed {
                                            transfer_id: self.transfer_id,
                                        });
                                }
                                Ok(()) => {
                                    // File grew or has unexpected data past expected size
                                    self.completed = true;
                                    let _ =
                                        self.lifecycle_tx.send(TransferLifecycleEvent::Failed {
                                            transfer_id: self.transfer_id,
                                            bytes_streamed: self.bytes_streamed,
                                        });
                                    return Poll::Ready(Err(io::Error::new(
                                        io::ErrorKind::InvalidData,
                                        "File contains unexpected data past expected size",
                                    )));
                                }
                                Err(e) => {
                                    self.completed = true;
                                    let _ =
                                        self.lifecycle_tx.send(TransferLifecycleEvent::Failed {
                                            transfer_id: self.transfer_id,
                                            bytes_streamed: self.bytes_streamed,
                                        });
                                    return Poll::Ready(Err(e));
                                }
                            }
                        }
                    }

                    Poll::Ready(Ok(()))
                } else if buf.remaining() > 0 {
                    // Buffer had capacity, yet 0 bytes read: clean EOF from inner stream.
                    if !self.completed {
                        self.completed = true;
                        if self.bytes_streamed == self.total_bytes {
                            let _ = self.lifecycle_tx.send(TransferLifecycleEvent::Completed {
                                transfer_id: self.transfer_id,
                            });
                            Poll::Ready(Ok(()))
                        } else {
                            // Truncated or altered source file: size mismatch at EOF
                            let _ = self.lifecycle_tx.send(TransferLifecycleEvent::Failed {
                                transfer_id: self.transfer_id,
                                bytes_streamed: self.bytes_streamed,
                            });
                            Poll::Ready(Err(io::Error::new(
                                io::ErrorKind::UnexpectedEof,
                                "File size mismatch or truncated before expected EOF",
                            )))
                        }
                    } else {
                        Poll::Ready(Ok(()))
                    }
                } else {
                    Poll::Ready(Ok(()))
                }
            }
            Poll::Ready(Err(e)) => {
                if !self.completed {
                    self.completed = true;
                    let _ = self.lifecycle_tx.send(TransferLifecycleEvent::Failed {
                        transfer_id: self.transfer_id,
                        bytes_streamed: self.bytes_streamed,
                    });
                }
                Poll::Ready(Err(e))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<R> Drop for ProgressReader<R> {
    fn drop(&mut self) {
        if !self.completed {
            self.completed = true;
            if self.bytes_streamed == self.total_bytes {
                let _ = self.lifecycle_tx.send(TransferLifecycleEvent::Completed {
                    transfer_id: self.transfer_id,
                });
            } else {
                let _ = self.lifecycle_tx.send(TransferLifecycleEvent::Cancelled {
                    transfer_id: self.transfer_id,
                    bytes_streamed: self.bytes_streamed,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn test_progress_reader_clean_eof_emits_completed() {
        let (lifecycle_tx, mut lifecycle_rx) = mpsc::unbounded_channel();
        let (progress_tx, _progress_rx) = mpsc::channel(32);
        let cancel_token = CancellationToken::new();

        let data = vec![1u8; 1024];
        let cursor = Cursor::new(data);
        let mut reader =
            ProgressReader::new(cursor, 1, 1024, lifecycle_tx, progress_tx, cancel_token);

        let mut output = Vec::new();
        reader.read_to_end(&mut output).await.expect("read_to_end");
        assert_eq!(output.len(), 1024);
        assert!(reader.is_completed());

        // Lifecycle event must be Completed
        let event = lifecycle_rx.try_recv().expect("receive lifecycle event");
        assert_eq!(event, TransferLifecycleEvent::Completed { transfer_id: 1 });

        // Drop after completion must not emit duplicate Cancelled event
        drop(reader);
        assert!(lifecycle_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_progress_reader_early_eof_truncated_emits_failed() {
        let (lifecycle_tx, mut lifecycle_rx) = mpsc::unbounded_channel();
        let (progress_tx, _progress_rx) = mpsc::channel(32);
        let cancel_token = CancellationToken::new();

        // Expected 2048 bytes, but stream truncated at 512 bytes
        let data = vec![1u8; 512];
        let cursor = Cursor::new(data);
        let mut reader =
            ProgressReader::new(cursor, 2, 2048, lifecycle_tx, progress_tx, cancel_token);

        let mut output = Vec::new();
        let res = reader.read_to_end(&mut output).await;
        assert!(res.is_err(), "Must error on truncated source file");
        assert!(reader.is_completed());

        // Lifecycle event must be Failed (not Cancelled, not Completed)
        let event = lifecycle_rx.try_recv().expect("receive failed event");
        assert_eq!(
            event,
            TransferLifecycleEvent::Failed {
                transfer_id: 2,
                bytes_streamed: 512,
            }
        );

        // Drop after failure must not emit Cancelled
        drop(reader);
        assert!(lifecycle_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_progress_reader_early_drop_emits_cancelled() {
        let (lifecycle_tx, mut lifecycle_rx) = mpsc::unbounded_channel();
        let (progress_tx, _progress_rx) = mpsc::channel(32);
        let cancel_token = CancellationToken::new();

        let data = vec![2u8; 2048];
        let cursor = Cursor::new(data);

        {
            let mut reader =
                ProgressReader::new(cursor, 3, 2048, lifecycle_tx, progress_tx, cancel_token);
            let mut buf = [0u8; 512];
            let n = reader.read(&mut buf).await.expect("read chunk");
            assert_eq!(n, 512);
            assert_eq!(reader.bytes_streamed(), 512);
            assert!(!reader.is_completed());
            // Drops here before EOF
        }

        let event = lifecycle_rx.try_recv().expect("receive cancelled event");
        assert_eq!(
            event,
            TransferLifecycleEvent::Cancelled {
                transfer_id: 3,
                bytes_streamed: 512,
            }
        );
    }

    struct FailingReader;
    impl AsyncRead for FailingReader {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            Poll::Ready(Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "disk read error",
            )))
        }
    }

    #[tokio::test]
    async fn test_progress_reader_error_emits_failed() {
        let (lifecycle_tx, mut lifecycle_rx) = mpsc::unbounded_channel();
        let (progress_tx, _progress_rx) = mpsc::channel(32);
        let cancel_token = CancellationToken::new();

        let mut reader = ProgressReader::new(
            FailingReader,
            4,
            500,
            lifecycle_tx,
            progress_tx,
            cancel_token,
        );
        let mut buf = [0u8; 64];
        let res = reader.read(&mut buf).await;
        assert!(res.is_err());
        assert!(reader.is_completed());

        let event = lifecycle_rx.try_recv().expect("receive failed event");
        assert_eq!(
            event,
            TransferLifecycleEvent::Failed {
                transfer_id: 4,
                bytes_streamed: 0,
            }
        );

        // Drop after failure should not emit Cancelled
        drop(reader);
        assert!(lifecycle_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_progress_reader_cancellation_token_prioritized_and_wakes_stream() {
        let (lifecycle_tx, mut lifecycle_rx) = mpsc::unbounded_channel();
        let (progress_tx, _progress_rx) = mpsc::channel(32);
        let cancel_token = CancellationToken::new();

        // Create reader with infinite pending stream
        struct PendingReader;
        impl AsyncRead for PendingReader {
            fn poll_read(
                self: Pin<&mut Self>,
                _cx: &mut Context<'_>,
                _buf: &mut ReadBuf<'_>,
            ) -> Poll<io::Result<()>> {
                Poll::Pending
            }
        }

        let mut reader = ProgressReader::new(
            PendingReader,
            5,
            10000,
            lifecycle_tx,
            progress_tx,
            cancel_token.clone(),
        );

        // Spawn read task that will wait on Pending
        let read_handle = tokio::spawn(async move {
            let mut buf = [0u8; 512];
            reader.read(&mut buf).await
        });

        // Small yield to allow read_handle to poll and register waker
        tokio::task::yield_now().await;

        // Cancel token
        cancel_token.cancel();

        // The read must wake up immediately and return ConnectionAborted
        let res = read_handle.await.expect("join handle");
        assert!(res.is_err(), "Cancelled read must return error");
        let err = res.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::ConnectionAborted);

        // Must emit Cancelled event
        let event = lifecycle_rx.try_recv().expect("receive cancelled event");
        assert_eq!(
            event,
            TransferLifecycleEvent::Cancelled {
                transfer_id: 5,
                bytes_streamed: 0,
            }
        );
        assert!(lifecycle_rx.try_recv().is_err());
    }
}
