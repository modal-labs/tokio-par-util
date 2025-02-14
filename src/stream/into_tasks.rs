use std::any::Any;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::stream::Map;
use futures_util::{Stream, StreamExt};
use pin_project::pin_project;
use tokio::task::{JoinError, JoinHandle};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::stream::attach_task_wiring::AttachTaskWiring;
use crate::task_wiring::{SpawnTaskFn, TaskWiring};

#[derive(Debug)]
#[pin_project]
pub struct IntoTasks<St>(#[pin] Map<AttachTaskWiring<St>, SpawnTaskFn<St::Item>>)
where
    St: Stream,
    St::Item: Future + Send + 'static,
    <St::Item as Future>::Output: Send;

#[pin_project]
pub struct Task<St>(#[pin] JoinHandle<Option<<St::Item as Future>::Output>>)
where
    St: Stream,
    St::Item: Future + Send + 'static,
    <St::Item as Future>::Output: Send;

#[derive(Debug)]
pub struct TaskError(JoinError);

impl<St> IntoTasks<St>
where
    St: Stream,
    St::Item: Future + Send + 'static,
    <St::Item as Future>::Output: Send,
{
    pub fn new(
        cancellation_token: CancellationToken,
        task_tracker: Arc<TaskTracker>,
        stream: St,
    ) -> Self {
        Self(
            AttachTaskWiring::new(cancellation_token, task_tracker, stream)
                .map(TaskWiring::spawn_task as SpawnTaskFn<St::Item>),
        )
    }
}

impl<St> Stream for IntoTasks<St>
where
    St: Stream,
    St::Item: Future + Send + 'static,
    <St::Item as Future>::Output: Send,
{
    type Item = Task<St>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().0.poll_next(cx).map(|o| o.map(Task))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl<St> Future for Task<St>
where
    St: Stream,
    St::Item: Future + Send + 'static,
    <St::Item as Future>::Output: Send,
{
    type Output = Result<Option<<St::Item as Future>::Output>, TaskError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().0.poll(cx).map_err(TaskError)
    }
}

impl<St> fmt::Debug for Task<St>
where
    St: Stream,
    St::Item: Future + Send,
    <St::Item as Future>::Output: fmt::Debug + Send,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Task").field(&self.0).finish()
    }
}

impl TaskError {
    pub fn try_into_panic(self) -> Result<Box<dyn Any + Send + 'static>, JoinError> {
        self.0.try_into_panic()
    }
}
