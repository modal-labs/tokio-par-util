use std::future::Future;
use std::marker::PhantomData;
use std::panic;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::Stream;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::future::Wait;
use crate::stream::assert_stream;
use crate::stream::future_buffer::FutureBuffer;
use crate::stream::into_tasks::{IntoTasks, Task, TaskError};

#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
#[pin_project::pin_project(PinnedDrop)]
pub struct ParallelBuffer<St, FutBuffer> {
    awaiting_completion: bool,
    cancellation_token: CancellationToken,
    task_tracker: Arc<TaskTracker>,
    task_err: Option<TaskError>,
    #[pin]
    buffer: Option<FutBuffer>,
    #[pin]
    wait: Wait,
    phantom: PhantomData<St>,
}

impl<St, FutBuffer> ParallelBuffer<St, FutBuffer> {
    pub fn awaiting_completion(mut self, value: bool) -> Self {
        self.awaiting_completion = value;
        self
    }
}

impl<St, FutBuffer> ParallelBuffer<St, FutBuffer>
where
    St: Stream,
    St::Item: Future + Send + 'static,
    <St::Item as Future>::Output: Send,
    FutBuffer: FutureBuffer<IntoTasks<St>>,
    FutBuffer: Stream<Item = Result<Option<<St::Item as Future>::Output>, TaskError>>,
{
    pub(crate) fn new(stream: St, cancellation_token: CancellationToken, limit: usize) -> Self {
        let awaiting_completion = true;
        let task_tracker = Arc::new(TaskTracker::new());
        let task_err = None;

        let stream = assert_stream::<Task<St>, _>(IntoTasks::new(
            cancellation_token.clone(),
            Arc::clone(&task_tracker),
            stream,
        ));

        let buffer = assert_stream::<Result<Option<<St::Item as Future>::Output>, TaskError>, _>(
            FutBuffer::buffer(stream, limit),
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

impl<St, FutBuffer> Stream for ParallelBuffer<St, FutBuffer>
where
    St: Stream,
    St::Item: Future + Send + 'static,
    <St::Item as Future>::Output: Send,
    FutBuffer: FutureBuffer<IntoTasks<St>>,
    FutBuffer: Stream<Item = Result<Option<<St::Item as Future>::Output>, TaskError>>,
{
    type Item = <<St as Stream>::Item as Future>::Output;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        let mut buffer_cell = this.buffer.as_mut();
        if let Some(stream) = buffer_cell.as_mut().as_pin_mut() {
            match stream.poll_next(cx) {
                Poll::Ready(Some(Ok(Some(item)))) => {
                    // Happy case: next item is available
                    return Poll::Ready(Some(item));
                }
                Poll::Ready(Some(Err(err))) => {
                    // A task panicked or got aborted
                    buffer_cell.set(None);
                    *this.task_err = Some(err);
                }
                Poll::Ready(None) | Poll::Ready(Some(Ok(None))) => {
                    // The stream ended or got cancelled through cancellation token
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
            if let Ok(panic) = err.try_into_panic() {
                panic::resume_unwind(panic)
            }
        }

        // Final end-of-stream
        Poll::Ready(None)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.buffer
            .as_ref()
            .map(Stream::size_hint)
            .unwrap_or_default()
    }
}

#[pin_project::pinned_drop]
impl<St, FutBuffer> PinnedDrop for ParallelBuffer<St, FutBuffer> {
    fn drop(self: Pin<&mut Self>) {
        self.task_tracker.close();
        self.cancellation_token.cancel();
    }
}
