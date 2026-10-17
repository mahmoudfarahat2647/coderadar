//! Minimal executor used to drive the `std::future`-based ports.
//!
//! Layer 2 expresses asynchronous boundaries with plain
//! `std::future::Future` trait objects and therefore cannot depend on an async
//! runtime. Layer 4 owns this tiny park/unpark executor instead: the Phase 2
//! evaluators complete synchronously, but the same driver keeps working when a
//! genuinely asynchronous evaluator (live Jev, Phase 5) is plugged in.

use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

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

/// Block the current thread until `future` completes.
pub(crate) fn block_on<F: Future>(future: F) -> F::Output {
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

#[cfg(test)]
mod tests {
    use super::block_on;
    use std::future::Future;
    use std::task::{Context, Poll};

    #[test]
    fn block_on_returns_ready_value() {
        assert_eq!(block_on(async { 40 + 2 }), 42);
    }

    #[test]
    fn block_on_handles_pending_future() {
        struct PendingOnce {
            polled: bool,
        }

        impl Future for PendingOnce {
            type Output = &'static str;

            fn poll(
                mut self: std::pin::Pin<&mut Self>,
                cx: &mut Context<'_>,
            ) -> Poll<Self::Output> {
                if self.polled {
                    Poll::Ready("done")
                } else {
                    self.polled = true;
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            }
        }

        assert_eq!(block_on(PendingOnce { polled: false }), "done");
    }
}
