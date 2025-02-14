use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::stream::Buffered;
use futures_util::Stream;
use tokio_util::sync::CancellationToken;

use crate::stream::into_tasks::IntoTasks;
use crate::stream::parallel_buffer::ParallelBuffer;
#[cfg(doc)]
use crate::stream::StreamParExt;

/// Given a [`Stream`] where every item is a [`Future`], this stream buffers up
/// to a certain number of futures, runs them in parallel on separate tasks, and
/// returns the results of the futures in the order in which they appeared in
/// the input stream.
///
/// If any of the futures panics, all other futures and tasks are immediately
/// cancelled and the panic gets returned immediately.
///
/// This stream is **cancellation safe** if the inner stream and generated
/// futures are also cancellation safe.  This means that dropping this stream
/// will also cancel any outstanding tasks and drop the relevant
/// futures/streams.
///
/// You can use [`ParallelBuffered::awaiting_completion`] to control
/// whether we wait for all tasks to fully terminate before this stream is
/// considered to have ended.
#[must_use = "streams do nothing unless polled"]
#[pin_project::pin_project]
pub struct ParallelBuffered<St>(#[pin] ParallelBuffer<St, Buffered<IntoTasks<St>>>)
where
    St: Stream,
    St::Item: Future + Send + 'static,
    <St::Item as Future>::Output: Send;

impl<St> ParallelBuffered<St>
where
    St: Stream,
    St::Item: Future + Send + 'static,
    <St::Item as Future>::Output: Send,
{
    /// Whether to always await completion for the tasks from the
    /// [`StreamParExt::parallel_buffered`] or
    /// [`StreamParExt::parallel_buffered_with_token`] call, even when
    /// one of the futures has panicked or been cancelled.
    pub fn awaiting_completion(self, value: bool) -> Self {
        Self(self.0.awaiting_completion(value))
    }

    pub(crate) fn new(
        stream: St,
        cancellation_token: CancellationToken,
        limit: usize,
    ) -> ParallelBuffered<St> {
        Self(ParallelBuffer::new(stream, cancellation_token, limit))
    }
}

impl<St> Stream for ParallelBuffered<St>
where
    St: Stream,
    St::Item: Future + Send,
    <St::Item as Future>::Output: Send,
{
    type Item = <<St as Stream>::Item as Future>::Output;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().0.poll_next(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl<St> fmt::Debug for ParallelBuffered<St>
where
    St: fmt::Debug + Stream,
    St::Item: fmt::Debug + Future + Send,
    <St::Item as Future>::Output: Send,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ParallelBuffered").field(&self.0).finish()
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
