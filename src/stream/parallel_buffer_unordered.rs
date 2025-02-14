use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::stream::BufferUnordered;
use futures_util::Stream;
use tokio_util::sync::CancellationToken;

use crate::stream::into_tasks::IntoTasks;
use crate::stream::parallel_buffer::ParallelBuffer;
#[cfg(doc)]
use crate::stream::StreamParExt;

/// Given a [`Stream`] where every item is a [`Future`], this stream buffers up
/// to a certain number of futures, runs them in parallel on separate tasks, and
/// returns the results of the futures in completion order.
///
/// If any of the futures panics, all other futures and tasks are immediately
/// cancelled and the panic gets returned immediately.
///
/// This stream is **cancellation safe** if the inner stream and generated
/// futures are also cancellation safe.  This means that dropping this stream
/// will also cancel any outstanding tasks and drop the relevant
/// futures/streams.
///
/// You can use [`ParallelBufferUnordered::awaiting_completion`] to control
/// whether we wait for all tasks to fully terminate before this stream is
/// considered to have ended.
#[must_use = "streams do nothing unless polled"]
#[pin_project::pin_project]
pub struct ParallelBufferUnordered<St>(#[pin] ParallelBuffer<St, BufferUnordered<IntoTasks<St>>>)
where
    St: Stream,
    St::Item: Future + Send + 'static,
    <St::Item as Future>::Output: Send;

impl<St> ParallelBufferUnordered<St>
where
    St: Stream,
    St::Item: Future + Send + 'static,
    <St::Item as Future>::Output: Send,
{
    /// Whether to always await completion for the tasks from the
    /// [`StreamParExt::parallel_buffer_unordered`] or
    /// [`StreamParExt::parallel_buffer_unordered_with_token`] call, even when
    /// one of the futures has panicked or been cancelled.
    pub fn awaiting_completion(self, value: bool) -> Self {
        Self(self.0.awaiting_completion(value))
    }

    pub(crate) fn new(
        stream: St,
        cancellation_token: CancellationToken,
        limit: usize,
    ) -> ParallelBufferUnordered<St> {
        Self(ParallelBuffer::new(stream, cancellation_token, limit))
    }
}

impl<St> Stream for ParallelBufferUnordered<St>
where
    St: Stream,
    St::Item: Future + Send + 'static,
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

impl<St> fmt::Debug for ParallelBufferUnordered<St>
where
    St: Stream + fmt::Debug,
    St::Item: fmt::Debug + Future + Send,
    <St::Item as Future>::Output: fmt::Debug + Send,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ParallelBufferUnordered")
            .field(&self.0)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::future;
    use std::sync::Arc;

    use futures_util::{stream, StreamExt};
    use scopeguard::defer;
    use tokio::sync::Semaphore;
    use tokio::task;
    use tokio_util::sync::CancellationToken;

    use crate::stream::StreamParExt;

    #[tokio::test]
    async fn test_parallel_buffer_unordered() -> anyhow::Result<()> {
        let result_set: HashSet<u32> = stream::iter([1, 2, 3, 4])
            .map(move |elem| async move { elem + 1 })
            .parallel_buffer_unordered(4)
            .collect()
            .await;

        assert!(result_set.contains(&2));
        assert!(result_set.contains(&3));
        assert!(result_set.contains(&4));
        assert!(result_set.contains(&5));

        Ok(())
    }

    #[tokio::test]
    async fn test_parallel_buffer_unordered_await_cancellation() -> anyhow::Result<()> {
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
            .parallel_buffer_unordered(4)
            .collect::<HashSet<_>>();
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
    async fn test_parallel_buffer_unordered_cancel_via_token() -> anyhow::Result<()> {
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
            .parallel_buffer_unordered_with_token(4, cancellation_token.clone())
            .collect::<HashSet<_>>();
        let task = task::spawn(future);

        // Ensure all futures have made progress past `defer!`
        drop(semaphore.acquire_many(4).await?);

        cancellation_token.cancel();

        // The result from the spawned task is `Ok(HashSet::new())`
        let returned_set = task.await?;
        assert!(returned_set.is_empty());

        // Check that `defer!` scope guards ran
        assert!(drop_set.contains(&1));
        assert!(drop_set.contains(&2));
        assert!(drop_set.contains(&3));
        assert!(drop_set.contains(&4));

        Ok(())
    }

    #[tokio::test]
    async fn test_parallel_buffer_unordered_panic() -> anyhow::Result<()> {
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
                        // Block forever here
                        future::pending::<u32>().await;
                    }
                }
            })
            .parallel_buffer_unordered(4)
            .collect::<HashSet<_>>();
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
