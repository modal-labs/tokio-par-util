use std::future::Future;
use std::marker::PhantomData;
use std::panic;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::Stream;
use futures_util::{TryFuture, TryStream};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::future::Wait;
use crate::try_stream::assert_try_stream;
use crate::try_stream::try_future_buffer::TryFutureBuffer;
use crate::try_stream::try_into_tasks::{TryIntoTasks, TryTask, TryTaskError};

#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
#[pin_project::pin_project(PinnedDrop)]
pub struct TryParallelBuffer<St, TryFutBuffer>
where
    St: TryStream,
    St::Ok: TryFuture + Send + 'static,
{
    awaiting_completion: bool,
    cancellation_token: CancellationToken,
    task_tracker: Arc<TaskTracker>,
    task_err: Option<TryTaskError<St>>,
    #[pin]
    buffer: Option<TryFutBuffer>,
    #[pin]
    wait: Wait,
    phantom: PhantomData<St>,
}

impl<St, TryFutBuffer> TryParallelBuffer<St, TryFutBuffer>
where
    St: TryStream,
    St::Ok: TryFuture + Send + 'static,
{
    pub fn awaiting_completion(mut self, value: bool) -> Self {
        self.awaiting_completion = value;
        self
    }
}

impl<St, TryFutBuffer> TryParallelBuffer<St, TryFutBuffer>
where
    St: TryStream,
    St::Ok: TryFuture<Error = St::Error> + Send + 'static,
    St::Error: Send,
    <St::Ok as TryFuture>::Ok: Send,
    <St::Ok as TryFuture>::Error: Send,
    TryFutBuffer: TryFutureBuffer<TryIntoTasks<St>>,
    TryFutBuffer: TryStream<Ok = <St::Ok as TryFuture>::Ok, Error = TryTaskError<St>>,
{
    pub(crate) fn new(stream: St, cancellation_token: CancellationToken, limit: usize) -> Self {
        let awaiting_completion = true;
        let task_tracker = Arc::new(TaskTracker::new());
        let task_err = None;

        let stream = assert_try_stream::<TryTask<St>, TryTaskError<St>, _>(TryIntoTasks::new(
            cancellation_token.clone(),
            Arc::clone(&task_tracker),
            stream,
        ));

        let buffer = assert_try_stream::<<St::Ok as TryFuture>::Ok, TryTaskError<St>, _>(
            TryFutBuffer::try_buffer(stream, limit),
        );

        let buffer = Some(buffer);

        let wait = Wait::new(Arc::clone(&task_tracker));
        let phantom = PhantomData;

        Self {
            awaiting_completion,
            cancellation_token,
            task_tracker,
            task_err,
            buffer,
            wait,
            phantom,
        }
    }
}

impl<St, TryFutBuffer> Stream for TryParallelBuffer<St, TryFutBuffer>
where
    St: TryStream,
    St::Ok: TryFuture<Error = St::Error> + Send + 'static,
    St::Error: Send,
    <St::Ok as TryFuture>::Ok: Send,
    <St::Ok as TryFuture>::Error: Send,
    TryFutBuffer: TryFutureBuffer<TryIntoTasks<St>>,
    TryFutBuffer: TryStream<Ok = <St::Ok as TryFuture>::Ok, Error = TryTaskError<St>>,
{
    type Item = Result<<St::Ok as TryFuture>::Ok, <St::Ok as TryFuture>::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        let mut buffer_cell = this.buffer.as_mut();
        if let Some(stream) = buffer_cell.as_mut().as_pin_mut() {
            match stream.try_poll_next(cx) {
                Poll::Ready(Some(Ok(item))) => {
                    // Happy case: next item is available
                    return Poll::Ready(Some(Ok(item)));
                }
                Poll::Ready(Some(Err(err))) => {
                    // Encountered err indicating stream termination; fall through
                    buffer_cell.set(None);
                    *this.task_err = Some(err);
                }
                Poll::Ready(None) => {
                    // The stream ended
                    buffer_cell.set(None);
                }
                Poll::Pending => {
                    // No item available yet
                    return Poll::Pending;
                }
            }
        };

        // If we reach this point, we have fallen through and are at end-of-stream, so
        // cancel any remaining tasks
        this.task_tracker.close();
        this.cancellation_token.cancel();

        // Optionally wait for cancelled tasks to finish
        if *this.awaiting_completion {
            match this.wait.poll(cx) {
                Poll::Ready(()) => (),
                // Still waiting for clean-up to finish
                Poll::Pending => return Poll::Pending,
            }
        }

        if let Some(err) = this.task_err.take() {
            match err {
                TryTaskError::Cancelled => Poll::Ready(None),
                TryTaskError::Stream(err) => Poll::Ready(Some(Err(err))),
                TryTaskError::Future(err) => Poll::Ready(Some(Err(err))),
                TryTaskError::Join(err) => {
                    if let Ok(panic) = err.try_into_panic() {
                        panic::resume_unwind(panic)
                    } else {
                        // Assume cancelled; end stream
                        Poll::Ready(None)
                    }
                }
            }
        } else {
            Poll::Ready(None)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.buffer
            .as_ref()
            .map(Stream::size_hint)
            .unwrap_or_default()
    }
}

#[pin_project::pinned_drop]
impl<St, FutBuffer> PinnedDrop for TryParallelBuffer<St, FutBuffer>
where
    St: TryStream,
    St::Ok: TryFuture + Send + 'static,
{
    fn drop(self: Pin<&mut Self>) {
        self.task_tracker.close();
        self.cancellation_token.cancel();
    }
}
