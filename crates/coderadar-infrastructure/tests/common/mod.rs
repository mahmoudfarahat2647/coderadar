//! Shared helpers for the infrastructure integration tests.
//!
//! The crate keeps its own executor internally, so the integration tests
//! need a tiny blocking helper to drive the ports of Layer 2.

#![allow(dead_code)]

use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use coderadar_domain::{DomainError, ScanSession, ScanSessionSummary};
use coderadar_usecases::{BoxFuture, ScanHistoryRepository};

/// Block the current thread until `future` completes.
pub fn block_on<F: std::future::Future>(future: F) -> F::Output {
    struct ThreadWaker {
        thread: std::thread::Thread,
    }

    impl Wake for ThreadWaker {
        fn wake(self: Arc<Self>) {
            self.thread.unpark();
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.thread.unpark();
        }
    }

    let waker = Waker::from(Arc::new(ThreadWaker {
        thread: std::thread::current(),
    }));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::park(),
        }
    }
}

/// A history repository that is always offline.
///
/// The double proves that a broken history cannot invalidate a finished scan:
/// every write and every read fails with [`DomainError::Io`].
#[derive(Debug, Clone, Copy, Default)]
pub struct FailingHistory;

impl FailingHistory {
    /// The error every operation of the double fails with.
    pub const ERROR: &'static str = "the history is offline";
}

impl ScanHistoryRepository for FailingHistory {
    fn save_scan_session<'a>(
        &'a self,
        _session: &'a ScanSession,
    ) -> BoxFuture<'a, Result<(), DomainError>> {
        Box::pin(async { Err(DomainError::Io(Self::ERROR.to_string())) })
    }

    fn get_history<'a>(
        &'a self,
        _limit: usize,
        _root: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Vec<ScanSessionSummary>, DomainError>> {
        Box::pin(async { Err(DomainError::Io(Self::ERROR.to_string())) })
    }
}
