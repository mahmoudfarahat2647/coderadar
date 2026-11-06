//! IPC throttler for batching findings streamed to the frontend (CR-SPEC-05, Phase 3).
//!
//! Emits findings in bounded batches at fixed intervals (default 100 ms) to keep
//! the webview UI smooth and responsive without IPC congestion.

use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use coderadar_adapters::{FindingDTO, RadarPresenter};
use coderadar_domain::Finding;
use serde::{Deserialize, Serialize};

/// Default batch flush interval: 100 ms (per Phase 3 spec).
pub const DEFAULT_FLUSH_INTERVAL: Duration = Duration::from_millis(100);

/// Payload emitted for each batched flush of findings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FindingBatch {
    /// Findings contained in this batch, already projected to radar coordinates.
    pub findings: Vec<FindingDTO>,
    /// Elapsed time since the start of the scan (in milliseconds).
    pub elapsed_ms: u64,
    /// Number of findings in this batch.
    pub count: usize,
    /// `true` when this is the final flush of the scan.
    pub is_last: bool,
}

/// Sender handle used by scanner threads to push findings to the throttler.
#[derive(Clone)]
pub struct ThrottlerSender {
    tx: Sender<Vec<Finding>>,
}

impl ThrottlerSender {
    /// Send a single finding to be throttled.
    pub fn send(&self, finding: Finding) {
        let _ = self.tx.send(vec![finding]);
    }

    /// Send a batch of findings from a scanner chunk evaluation.
    pub fn send_batch(&self, findings: &[Finding]) {
        if !findings.is_empty() {
            let _ = self.tx.send(findings.to_vec());
        }
    }
}

/// Handle to the background throttler worker thread.
pub struct ThrottlerHandle {
    handle: Option<JoinHandle<usize>>,
}

impl ThrottlerHandle {
    /// Wait for the throttler thread to finish flushing and return the total
    /// number of findings dispatched.
    pub fn join(mut self) -> usize {
        if let Some(h) = self.handle.take() {
            h.join().unwrap_or(0)
        } else {
            0
        }
    }
}

/// Background IPC throttler that collects findings from scanner threads
/// and flushes them in batches every `flush_interval`.
pub struct FindingThrottler;

impl FindingThrottler {
    /// Start a throttler background worker that calls `on_batch` at most every
    /// `interval` when findings are available, and once at the end.
    pub fn start<F>(interval: Duration, mut on_batch: F) -> (ThrottlerSender, ThrottlerHandle)
    where
        F: FnMut(FindingBatch) + Send + 'static,
    {
        let (tx, rx): (Sender<Vec<Finding>>, Receiver<Vec<Finding>>) = channel();
        let started = Instant::now();

        let handle = thread::spawn(move || {
            let mut buffer: Vec<FindingDTO> = Vec::new();
            let mut last_flush = Instant::now();
            let mut total_findings = 0usize;

            loop {
                let elapsed = last_flush.elapsed();
                let timeout = if elapsed >= interval {
                    Duration::from_millis(0)
                } else {
                    interval - elapsed
                };

                let received = if buffer.is_empty() {
                    rx.recv().map_err(|_| RecvTimeoutError::Disconnected)
                } else {
                    rx.recv_timeout(timeout)
                };
                match received {
                    Ok(batch) => {
                        for finding in batch {
                            let coord = RadarPresenter::project_to_polar(&finding);
                            buffer.push(FindingDTO::from_finding(&finding, coord));
                        }
                        if last_flush.elapsed() >= interval && !buffer.is_empty() {
                            let count = buffer.len();
                            total_findings += count;
                            on_batch(FindingBatch {
                                findings: std::mem::take(&mut buffer),
                                elapsed_ms: started.elapsed().as_millis() as u64,
                                count,
                                is_last: false,
                            });
                            last_flush = Instant::now();
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        if !buffer.is_empty() {
                            let count = buffer.len();
                            total_findings += count;
                            on_batch(FindingBatch {
                                findings: std::mem::take(&mut buffer),
                                elapsed_ms: started.elapsed().as_millis() as u64,
                                count,
                                is_last: false,
                            });
                            last_flush = Instant::now();
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        let count = buffer.len();
                        total_findings += count;
                        on_batch(FindingBatch {
                            findings: std::mem::take(&mut buffer),
                            elapsed_ms: started.elapsed().as_millis() as u64,
                            count,
                            is_last: true,
                        });
                        break;
                    }
                }
            }

            total_findings
        });

        (
            ThrottlerSender { tx },
            ThrottlerHandle {
                handle: Some(handle),
            },
        )
    }
}
