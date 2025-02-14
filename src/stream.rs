//! Extension methods for anything that implements the [`Stream`] trait.
use std::future::Future;

use futures_util::Stream;
#[cfg(doc)]
use futures_util::StreamExt;
use tokio_util::sync::CancellationToken;

mod attach_task_wiring;
mod future_buffer;
mod into_tasks;
mod into_try_stream;
mod parallel_buffer;
mod parallel_buffer_unordered;
mod parallel_buffered;

pub use into_try_stream::IntoTryStream;
pub use parallel_buffer_unordered::ParallelBufferUnordered;
pub use parallel_buffered::ParallelBuffered;

/// Extension trait for [`Stream`] to add additional methods to it.
pub trait StreamParExt: Stream {
    /// An adaptor for creating a buffered list of pending futures (unordered).
    ///
    /// If this stream's item can be converted into a future, then this adaptor
    /// will buffer up to `limit` futures, run them in parallel on separate
    /// tasks, and then return the outputs in the order in which they
    /// complete. No more than `limit` futures will be buffered at any point
    /// in time, and less than `limit` may also be buffered depending on the
    /// state of each future.
    ///
    /// The returned stream is **cancellation safe** if the inner stream and
    /// generated futures are also cancellation safe.  This means that dropping
    /// this stream will also cancel any outstanding tasks and drop the relevant
    /// futures/streams.
    ///
    /// The returned stream will be a stream of each future's output.
    fn parallel_buffer_unordered(self, limit: usize) -> ParallelBufferUnordered<Self>
    where
        Self: Sized,
        Self::Item: Future + Send + 'static,
        <Self::Item as Future>::Output: Send,
    {
        ParallelBufferUnordered::new(self, CancellationToken::new(), limit)
    }

    /// Like [`StreamParExt::parallel_buffer_unordered`], but with a custom
    /// [`CancellationToken`] to gracefully shut down the stream. A stream that
    /// is shut down via its token will cancel any running tasks, stop yielding
    /// stream items, and report end-of-stream.
    fn parallel_buffer_unordered_with_token(
        self,
        limit: usize,
        cancellation_token: CancellationToken,
    ) -> ParallelBufferUnordered<Self>
    where
        Self: Sized,
        Self::Item: Future + Send + 'static,
        <Self::Item as Future>::Output: Send,
    {
        ParallelBufferUnordered::new(self, cancellation_token, limit)
    }

    /// An adaptor for creating a buffered list of pending futures.
    ///
    /// If this stream's item can be converted into a future, then this adaptor
    /// will buffer up to at most `limit` futures, run them in parallel on
    /// separate tasks, and then return the outputs in the same order as the
    /// underlying stream. No more than `limit` futures will be buffered at any
    /// point in time, and less than `limit` may also be buffered depending on
    /// the state of each future.
    ///
    /// The returned stream is **cancellation safe** if the inner stream and
    /// generated futures are also cancellation safe.  This means that dropping
    /// this stream will also cancel any outstanding tasks and drop the relevant
    /// futures/streams.
    ///
    /// The returned stream will be a stream of each future's output.
    fn parallel_buffered(self, limit: usize) -> ParallelBuffered<Self>
    where
        Self: Sized,
        Self::Item: Future + Send + 'static,
        <Self::Item as Future>::Output: Send,
    {
        ParallelBuffered::new(self, CancellationToken::new(), limit)
    }

    /// Like [`StreamParExt::parallel_buffered`], but with a custom
    /// [`CancellationToken`] to gracefully shut down the stream. A stream that
    /// is shut down via its token will cancel any running tasks, stop yielding
    /// stream items, and report end-of-stream.
    fn parallel_buffered_with_token(
        self,
        limit: usize,
        cancellation_token: CancellationToken,
    ) -> ParallelBuffered<Self>
    where
        Self: Sized,
        Self::Item: Future + Send + 'static,
        <Self::Item as Future>::Output: Send,
    {
        ParallelBuffered::new(self, cancellation_token, limit)
    }

    /// Wraps every element in this stream in `Ok(...)`.
    ///
    /// This is useful for turning any stream into something that implements
    /// `TryStream`.
    fn into_try_stream<E>(self) -> IntoTryStream<Self, E>
    where
        Self: Sized,
    {
        IntoTryStream::new(self)
    }
}

impl<St> StreamParExt for St where St: Stream {}

// Just a helper function to ensure the streams we're returning all have the
// right implementations.
pub(crate) fn assert_stream<A, S>(stream: S) -> S
where
    S: Stream<Item = A>,
{
    stream
}
