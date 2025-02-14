use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::stream::MapOk;
use futures_util::{Stream, TryFuture, TryStream, TryStreamExt};
use pin_project::pin_project;
use tokio::task::{JoinError, JoinHandle};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::task_wiring::{TaskWiring, TrySpawnTaskFn};
use crate::try_stream::try_attach_task_wiring::TryAttachTaskWiring;

#[derive(Debug)]
#[pin_project]
pub struct TryIntoTasks<St>(#[pin] MapOk<TryAttachTaskWiring<St>, TrySpawnTaskFn<St::Ok>>)
where
    St: TryStream,
    St::Ok: TryFuture + Send + 'static,
    <St::Ok as TryFuture>::Ok: Send,
    <St::Ok as TryFuture>::Error: Send;

type StreamResult<St> =
    Result<<<St as TryStream>::Ok as TryFuture>::Ok, <<St as TryStream>::Ok as TryFuture>::Error>;

#[pin_project]
pub struct TryTask<St>(#[pin] JoinHandle<Option<StreamResult<St>>>)
where
    St: TryStream,
    St::Ok: TryFuture + Send + 'static,
    <St::Ok as TryFuture>::Ok: Send,
    <St::Ok as TryFuture>::Error: Send;

pub enum TryTaskError<St>
where
    St: TryStream,
    St::Ok: TryFuture + Send + 'static,
{
    Cancelled,
    Stream(St::Error),
    Future(<St::Ok as TryFuture>::Error),
    Join(JoinError),
}

impl<St> TryIntoTasks<St>
where
    St: TryStream,
    St::Ok: TryFuture + Send + 'static,
    <St::Ok as TryFuture>::Ok: Send,
    <St::Ok as TryFuture>::Error: Send,
{
    pub fn new(
        cancellation_token: CancellationToken,
        task_tracker: Arc<TaskTracker>,
        stream: St,
    ) -> Self {
        Self(
            TryAttachTaskWiring::new(cancellation_token, task_tracker, stream)
                .map_ok(TaskWiring::try_spawn_task as TrySpawnTaskFn<St::Ok>),
        )
    }
}

impl<St> Stream for TryIntoTasks<St>
where
    St: TryStream,
    St::Ok: TryFuture + Send + 'static,
    <St::Ok as TryFuture>::Ok: Send,
    <St::Ok as TryFuture>::Error: Send,
{
    type Item = Result<TryTask<St>, TryTaskError<St>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project()
            .0
            .try_poll_next(cx)
            .map(|opt| opt.map(|res| res.map(TryTask).map_err(TryTaskError::Stream)))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl<St> Future for TryTask<St>
where
    St: TryStream,
    St::Ok: TryFuture + Send + 'static,
    <St::Ok as TryFuture>::Ok: Send,
    <St::Ok as TryFuture>::Error: Send,
{
    type Output = Result<<St::Ok as TryFuture>::Ok, TryTaskError<St>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.project().0.poll(cx) {
            Poll::Ready(Ok(Some(Ok(v)))) => Poll::Ready(Ok(v)),
            Poll::Ready(Ok(Some(Err(e)))) => Poll::Ready(Err(TryTaskError::Future(e))),
            Poll::Ready(Ok(None)) => Poll::Ready(Err(TryTaskError::Cancelled)),
            Poll::Ready(Err(e)) => Poll::Ready(Err(TryTaskError::Join(e))),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<St> fmt::Debug for TryTask<St>
where
    St: TryStream,
    St::Ok: TryFuture + Send,
    <St::Ok as TryFuture>::Ok: fmt::Debug + Send,
    <St::Ok as TryFuture>::Error: fmt::Debug + Send,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("TryTask").field(&self.0).finish()
    }
}

impl<St> fmt::Debug for TryTaskError<St>
where
    St: TryStream,
    St::Ok: TryFuture + Send,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            TryTaskError::Cancelled => f.write_str("Cancelled"),
            TryTaskError::Stream(_) => f.debug_tuple("Stream").field(&"...").finish(),
            TryTaskError::Future(_) => f.debug_tuple("Future").field(&"...").finish(),
            TryTaskError::Join(_) => f.debug_tuple("Join").field(&"...").finish(),
        }
    }
}
