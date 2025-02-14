use std::fmt;
use std::fmt::Formatter;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::future::BoxFuture;
use futures_util::FutureExt;
use tokio_util::task::TaskTracker;

#[pin_project::pin_project]
pub struct Wait(#[pin] BoxFuture<'static, ()>);

impl Wait {
    pub fn new(task_tracker: Arc<TaskTracker>) -> Self {
        let task_tracker = Arc::clone(&task_tracker);
        // TODO: more efficient impl avoiding boxed()?
        Self(async move { task_tracker.wait().await }.boxed())
    }
}

impl Future for Wait {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().0.poll(cx)
    }
}

impl fmt::Debug for Wait {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // TODO: smarter debug impl if we can avoid boxing the future
        f.write_str("Wait(...)")
    }
}
