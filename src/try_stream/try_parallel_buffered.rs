use std::fmt;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::stream::TryBuffered;
use futures_util::{Stream, TryFuture, TryStream};
use tokio_util::sync::CancellationToken;

use crate::try_stream::try_into_tasks::TryIntoTasks;
use crate::try_stream::try_parallel_buffer::TryParallelBuffer;
#[cfg(doc)]
use crate::try_stream::TryStreamParExt;

/// Given a [`TryStream`] where every item is a [`TryFuture`], this stream
/// buffers up to a certain number of futures, runs them in parallel on separate
/// tasks, and returns the results of the futures in the same order as in the
/// input stream.
///
/// If any of the futures returns an error or panics, all other futures and
/// tasks are immediately cancelled and the error/panic gets returned
/// immediately.
///
/// This stream is **cancellation safe** if the inner stream and generated
/// futures are also cancellation safe.  This means that dropping this stream
/// will also cancel any outstanding tasks and drop the relevant
/// futures/streams.
///
/// You can use [`TryParallelBuffered::awaiting_completion`] to control whether
/// we wait for all tasks to fully terminate before this stream is considered to
/// have ended.
#[must_use = "streams do nothing unless polled"]
#[pin_project::pin_project]
pub struct TryParallelBuffered<St>(#[pin] TryParallelBuffer<St, TryBuffered<TryIntoTasks<St>>>)
where
    St: TryStream,
    St::Ok: TryFuture<Error = St::Error> + Send + 'static,
    St::Error: Send,
    <St::Ok as TryFuture>::Ok: Send,
    <St::Ok as TryFuture>::Error: Send;

impl<St> TryParallelBuffered<St>
where
    St: TryStream,
    St::Ok: TryFuture<Error = St::Error> + Send + 'static,
    St::Error: Send,
    <St::Ok as TryFuture>::Ok: Send,
    <St::Ok as TryFuture>::Error: Send,
{
    /// Whether to always await completion for the tasks from the
    /// [`TryStreamParExt::try_parallel_buffered`] or
    /// [`TryStreamParExt::try_parallel_buffered_with_token`] call, even when
    /// one of the futures has panicked or been cancelled.
    pub fn awaiting_completion(self, value: bool) -> Self {
        Self(self.0.awaiting_completion(value))
    }

    pub(crate) fn new(
        stream: St,
        cancellation_token: CancellationToken,
        limit: usize,
    ) -> TryParallelBuffered<St> {
        Self(TryParallelBuffer::new(stream, cancellation_token, limit))
    }
}

impl<St> Stream for TryParallelBuffered<St>
where
    St: TryStream,
    St::Ok: TryFuture<Error = St::Error> + Send + 'static,
    St::Error: Send,
    <St::Ok as TryFuture>::Ok: Send,
    <St::Ok as TryFuture>::Error: Send,
{
    type Item = Result<<<St as TryStream>::Ok as TryFuture>::Ok, St::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().0.try_poll_next(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl<St> fmt::Debug for TryParallelBuffered<St>
where
    St: TryStream + fmt::Debug,
    St::Ok: fmt::Debug + TryFuture<Error = St::Error> + Send,
    <St::Ok as TryFuture>::Ok: fmt::Debug + Send,
    <St::Ok as TryFuture>::Error: fmt::Debug + Send,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("TryParallelBuffered").field(&self.0).finish()
    }
}

#[cfg(test)]
mod tests {
    use std::future;
    use std::sync::Arc;

    use futures_util::{stream, StreamExt};
    use scopeguard::defer;
    use tokio::sync::Semaphore;
    use tokio::task;
    use tokio_util::sync::CancellationToken;

    use crate::stream::StreamParExt;

    #[tokio::test]
    async fn test_parallel_buffered() -> anyhow::Result<()> {
        let result_vec: Vec<u32> = stream::iter([1, 2, 3, 4])
            .map(move |elem| async move { elem + 1 })
            .parallel_buffered(4)
            .collect()
            .await;

        assert_eq!(result_vec, &[2, 3, 4, 5]);

        Ok(())
    }

    #[tokio::test]
    async fn test_parallel_buffered_await_cancellation() -> anyhow::Result<()> {
        let drop_set = Arc::new(dashmap::DashSet::new());
        let semaphore = Arc::new(Semaphore::new(0));

        let future = stream::iter([1, 2, 3, 4])
            .map({
                let drop_set = Arc::clone(&drop_set);
                let semaphore = Arc::clone(&semaphore);

                move |elem| {
                    let drop_set = Arc::clone(&drop_set);
                    let semaphore = Arc::clone(&semaphore);
                    async move {
                        defer! { drop_set.insert(elem); }
                        semaphore.add_permits(1);
                        // Block forever here
                        future::pending::<u32>().await;
                    }
                }
            })
            .parallel_buffered(4)
            .collect::<Vec<_>>();
        let task = task::spawn(future);

        // Ensure all futures have made progress past `defer!`
        drop(semaphore.acquire_many(4).await?);

        task.abort();

        if let Err(err) = task.await {
            assert!(err.is_cancelled());
        } else {
            panic!("expected task to be cancelled")
        }

        // Check that `defer!` scope guards ran
        assert!(drop_set.contains(&1));
        assert!(drop_set.contains(&2));
        assert!(drop_set.contains(&3));
        assert!(drop_set.contains(&4));

        Ok(())
    }

    #[tokio::test]
    async fn test_parallel_buffered_cancel_via_token() -> anyhow::Result<()> {
        let drop_set = Arc::new(dashmap::DashSet::new());
        let semaphore = Arc::new(Semaphore::new(0));
        let cancellation_token = CancellationToken::new();

        let future = stream::iter([1, 2, 3, 4])
            .map({
                let drop_set = Arc::clone(&drop_set);
                let semaphore = Arc::clone(&semaphore);

                move |elem| {
                    let drop_set = Arc::clone(&drop_set);
                    let semaphore = Arc::clone(&semaphore);
                    async move {
                        defer! { drop_set.insert(elem); }
                        semaphore.add_permits(1);
                        // Block forever here
                        future::pending::<u32>().await;
                    }
                }
            })
            .parallel_buffered_with_token(4, cancellation_token.clone())
            .collect::<Vec<_>>();
        let task = task::spawn(future);

        // Ensure all futures have made progress past `defer!`
        drop(semaphore.acquire_many(4).await?);

        cancellation_token.cancel();

        // The result from the spawned task is `Ok(Vec::new())`
        let returned_vec = task.await?;
        assert!(returned_vec.is_empty());

        // Check that `defer!` scope guards ran
        assert!(drop_set.contains(&1));
        assert!(drop_set.contains(&2));
        assert!(drop_set.contains(&3));
        assert!(drop_set.contains(&4));

        Ok(())
    }

    #[tokio::test]
    async fn test_parallel_buffered_panic() -> anyhow::Result<()> {
        let drop_set = Arc::new(dashmap::DashSet::new());
        let semaphore = Arc::new(Semaphore::new(0));

        let future = stream::iter([1, 2, 3, 4])
            .map({
                let drop_set = Arc::clone(&drop_set);
                let semaphore = Arc::clone(&semaphore);

                move |elem| {
                    let drop_set = Arc::clone(&drop_set);
                    let semaphore = Arc::clone(&semaphore);
                    async move {
                        defer! { drop_set.insert(elem); }
                        semaphore.add_permits(1);
                        if elem == 2 {
                            panic!("allergic to the number 2")
                        }
                        if elem > 2 {
                            // Block forever here
                            future::pending::<u32>().await;
                        }
                        elem + 1
                    }
                }
            })
            .parallel_buffered(4)
            .collect::<Vec<_>>();
        let task = task::spawn(future);

        // Ensure all futures have made progress past `defer!`
        drop(semaphore.acquire_many(4).await?);

        // Expect a panic to be caught here
        let res = task.await;

        // Check that `defer!` scope guards ran
        assert!(drop_set.contains(&1));
        assert!(drop_set.contains(&2));
        assert!(drop_set.contains(&3));
        assert!(drop_set.contains(&4));

        let err = res.err().unwrap();
        let panic_msg = *err.into_panic().downcast_ref::<&'static str>().unwrap();
        assert_eq!(panic_msg, "allergic to the number 2");

        Ok(())
    }
}
