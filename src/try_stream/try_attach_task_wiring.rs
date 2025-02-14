//! Internal (crate-private) utility stream for wiring up tasks.
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::{Stream, TryStream};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::task_wiring::TaskWiring;

/// Transforms the stream `St` by wrapping each `Ok(...)` element (presumable a
/// future) `Fut` in a `TaskWiring<Fut>`.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
#[pin_project::pin_project]
pub struct TryAttachTaskWiring<St> {
    cancellation_token: CancellationToken,
    task_tracker: Arc<TaskTracker>,
    #[pin]
    stream: St,
}

impl<St> TryAttachTaskWiring<St> {
    pub fn new(
        cancellation_token: CancellationToken,
        task_tracker: Arc<TaskTracker>,
        stream: St,
    ) -> Self {
        Self {
            cancellation_token,
            task_tracker,
            stream,
        }
    }
}

impl<St> Stream for TryAttachTaskWiring<St>
where
    St: TryStream,
{
    type Item = Result<TaskWiring<St::Ok>, St::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let cancellation_token = self.cancellation_token.clone();
        let task_tracker = Arc::clone(&self.task_tracker);
        let this = self.project();
        this.stream.try_poll_next(cx).map(|opt| {
            opt.map(|res| {
                res.map(|future| TaskWiring::new(cancellation_token, task_tracker, future))
            })
        })
    }
}
