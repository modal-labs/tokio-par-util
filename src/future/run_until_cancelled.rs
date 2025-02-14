use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio_util::sync::{CancellationToken, WaitForCancellationFutureOwned};

/// A Future that is resolved once the corresponding [`CancellationToken`]
/// is cancelled or a given Future gets resolved. It is biased towards the
/// Future completion.
#[derive(Debug)]
#[pin_project::pin_project]
#[must_use = "futures do nothing unless polled"]
pub struct RunUntilCancelled<F> {
    #[pin]
    cancellation: WaitForCancellationFutureOwned,
    #[pin]
    future: F,
}

impl<F> RunUntilCancelled<F> {
    pub(crate) fn new(cancellation_token: CancellationToken, future: F) -> Self {
        let cancellation = cancellation_token.cancelled_owned();
        Self {
            cancellation,
            future,
        }
    }
}

impl<F> Future for RunUntilCancelled<F>
where
    F: Future,
{
    type Output = Option<F::Output>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        if let Poll::Ready(res) = this.future.poll(cx) {
            Poll::Ready(Some(res))
        } else if this.cancellation.poll(cx).is_ready() {
            Poll::Ready(None)
        } else {
            Poll::Pending
        }
    }
}
