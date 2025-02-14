//! Internal (crate-private) utility stream for wiring up tasks.
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::Stream;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::task_wiring::TaskWiring;

/// Transforms the stream `St` by wrapping each element (presumably a future)
/// `Fut` in a `TaskWiring<Fut>`.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
#[pin_project::pin_project]
pub struct AttachTaskWiring<St> {
    cancellation_token: CancellationToken,
    task_tracker: Arc<TaskTracker>,
    #[pin]
    stream: St,
}

impl<St> AttachTaskWiring<St> {
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

impl<St> Stream for AttachTaskWiring<St>
where
    St: Stream,
{
    type Item = TaskWiring<St::Item>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let cancellation_token = self.cancellation_token.clone();
        let task_tracker = Arc::clone(&self.task_tracker);
        let this = self.project();
        this.stream
            .poll_next(cx)
            .map(|opt| opt.map(|future| TaskWiring::new(cancellation_token, task_tracker, future)))
    }
}
