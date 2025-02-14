//! Extension methods for anything that implements the [`TryStream`] trait.
use futures_util::{TryFuture, TryStream};
use tokio_util::sync::CancellationToken;

mod try_attach_task_wiring;
mod try_future_buffer;
mod try_into_tasks;
mod try_parallel_buffer;
mod try_parallel_buffer_unordered;
mod try_parallel_buffered;

pub use try_parallel_buffer_unordered::TryParallelBufferUnordered;
pub use try_parallel_buffered::TryParallelBuffered;

/// Extension trait for [`TryStream`] to add additional methods to it.
pub trait TryStreamParExt: TryStream {
    /// Attempt to execute several futures from a stream concurrently
    /// (unordered).
    ///
    /// This stream's `Ok` type must be a [`TryFuture`] with an `Error` type
    /// that matches the stream's `Error` type.
    ///
    /// This adaptor will buffer up to `limit` futures, run them in parallel on
    /// separate tasks, and then return their outputs in the order in which
    /// they complete. If the underlying stream returns an error, it will be
    /// immediately propagated.
    ///
    /// The returned stream is **cancellation safe** if the inner stream and
    /// generated futures are also cancellation safe.  This means that dropping
    /// this stream will also cancel any outstanding tasks and drop the relevant
    /// futures/streams.
    ///
    /// The returned stream will be a stream of results, each containing either
    /// an error or a future's output. An error can be produced either by the
    /// underlying stream itself or by one of the futures it yielded.
    fn try_parallel_buffer_unordered(self, limit: usize) -> TryParallelBufferUnordered<Self>
    where
        Self: Sized,
        Self::Ok: TryFuture<Error = Self::Error> + Send + 'static,
        Self::Error: Send,
        <Self::Ok as TryFuture>::Ok: Send,
    {
        assert_try_stream::<<Self::Ok as TryFuture>::Ok, <Self::Ok as TryFuture>::Error, _>(
            TryParallelBufferUnordered::new(self, CancellationToken::new(), limit),
        )
    }

    /// Like [`TryStreamParExt::try_parallel_buffer_unordered`], but with a
    /// custom [`CancellationToken`] to gracefully shut down the stream. A
    /// stream that is shut down via its token will cancel any running tasks,
    /// stop yielding stream items, and report end-of-stream.
    fn try_parallel_buffer_unordered_with_token(
        self,
        limit: usize,
        cancellation_token: CancellationToken,
    ) -> TryParallelBufferUnordered<Self>
    where
        Self: Sized,
        Self::Ok: TryFuture<Error = Self::Error> + Send + 'static,
        Self::Error: Send,
        <Self::Ok as TryFuture>::Ok: Send,
    {
        assert_try_stream::<<Self::Ok as TryFuture>::Ok, <Self::Ok as TryFuture>::Error, _>(
            TryParallelBufferUnordered::new(self, cancellation_token, limit),
        )
    }

    /// Attempt to execute several futures from a stream concurrently.
    ///
    /// This stream's `Ok` type must be a [`TryFuture`] with an `Error` type
    /// that matches the stream's `Error` type.
    ///
    /// This adaptor will buffer up to `limit` futures, run them in parallel on
    /// separate tasks, and then return their outputs in the same order as the
    /// underlying stream. If the underlying stream returns an error, it will be
    /// immediately propagated.
    ///
    /// The returned stream is **cancellation safe** if the inner stream and
    /// generated futures are also cancellation safe.  This means that dropping
    /// this stream will also cancel any outstanding tasks and drop the relevant
    /// futures/streams.
    ///
    /// The returned stream will be a stream of results, each containing either
    /// an error or a future's output. An error can be produced either by the
    /// underlying stream itself or by one of the futures it yielded.
    fn try_parallel_buffered(self, limit: usize) -> TryParallelBuffered<Self>
    where
        Self: Sized,
        Self::Ok: TryFuture<Error = Self::Error> + Send + 'static,
        Self::Error: Send,
        <Self::Ok as TryFuture>::Ok: Send,
    {
        assert_try_stream::<<Self::Ok as TryFuture>::Ok, <Self::Ok as TryFuture>::Error, _>(
            TryParallelBuffered::new(self, CancellationToken::new(), limit),
        )
    }

    /// Like [`TryStreamParExt::try_parallel_buffered`], but with a custom
    /// [`CancellationToken`] to gracefully shut down the stream. A stream that
    /// is shut down via its token will cancel any running tasks, stop yielding
    /// stream items, and report end-of-stream.
    fn try_parallel_buffered_with_token(
        self,
        limit: usize,
        cancellation_token: CancellationToken,
    ) -> TryParallelBuffered<Self>
    where
        Self: Sized,
        Self::Ok: TryFuture<Error = Self::Error> + Send + 'static,
        Self::Error: Send,
        <Self::Ok as TryFuture>::Ok: Send,
    {
        assert_try_stream::<<Self::Ok as TryFuture>::Ok, <Self::Ok as TryFuture>::Error, _>(
            TryParallelBuffered::new(self, cancellation_token, limit),
        )
    }
}

impl<St> TryStreamParExt for St where St: TryStream {}

// Just a helper function to ensure the try-streams we're returning all have the
// right implementations.
pub(crate) fn assert_try_stream<A, E, S>(stream: S) -> S
where
    S: TryStream<Ok = A, Error = E>,
{
    stream
}
