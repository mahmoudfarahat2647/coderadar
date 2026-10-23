//! Test-only executor for the crate's `BoxFuture` ports.
//!
//! The layer forbids async runtimes, so unit tests drive their futures with a
//! no-op waker. Every fake port in these tests is immediately ready; a pending
//! future yields the thread instead of spinning on a busy loop.

use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

struct NoopWake;

impl Wake for NoopWake {
    fn wake(self: Arc<Self>) {}
}

/// Poll `future` to completion.
pub(crate) fn block_on<F: Future>(future: F) -> F::Output {
    let waker: Waker = Waker::from(Arc::new(NoopWake));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
