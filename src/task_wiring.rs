use std::future::{Future, Ready};
use std::sync::Arc;

use futures_util::{TryFuture, TryFutureExt};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::future::RunUntilCancelled;

/// Task wiring context that can be used to spawn a future on a dedicated task.
///
/// Use [`TaskWiring::spawn_task`] or [`TaskWiring::try_spawn_task`] as
/// an argument to some transformer (e.g. `.map(...)`) to actually transform a
/// stream/iterator of [`TaskWiring`]s into spawned tasks.
#[derive(Debug)]
pub struct TaskWiring<Fut> {
    cancellation_token: CancellationToken,
    task_tracker: Arc<TaskTracker>,
    future: Fut,
}

/// Convenience type alias to coerce [`TaskWiring::spawn_task`] into a `fn()`
/// raw function pointer type.
pub type SpawnTaskFn<Fut> = fn(TaskWiring<Fut>) -> JoinHandle<Option<<Fut as Future>::Output>>;

// Static assert that the types are compatible
const _: SpawnTaskFn<Ready<()>> = TaskWiring::spawn_task;

/// Convenience type alias to coerce [`TaskWiring::try_spawn_task`] into a
/// `fn()` raw function pointer type.
pub type TrySpawnTaskFn<Fut> =
    fn(
        TaskWiring<Fut>,
    ) -> JoinHandle<Option<Result<<Fut as TryFuture>::Ok, <Fut as TryFuture>::Error>>>;

// Static assert that the types are compatible
const _: TrySpawnTaskFn<Ready<Result<(), ()>>> = TaskWiring::try_spawn_task;

impl<Fut> TaskWiring<Fut> {
    pub fn new(
        cancellation_token: CancellationToken,
        task_tracker: Arc<TaskTracker>,
        future: Fut,
    ) -> Self {
        Self {
            cancellation_token,
            task_tracker,
            future,
        }
    }
}

impl<Fut> TaskWiring<Fut>
where
    Fut: Future + Send + 'static,
    Fut::Output: Send,
{
    pub fn spawn_task(
        TaskWiring {
            cancellation_token,
            task_tracker,
            future,
        }: TaskWiring<Fut>,
    ) -> JoinHandle<Option<Fut::Output>> {
        let future = RunUntilCancelled::new(cancellation_token, future);
        #[cfg(feature = "tracing")]
        let future = tracing::Instrument::in_current_span(future);
        task_tracker.spawn(future)
    }
}

impl<Fut> TaskWiring<Fut>
where
    Fut: TryFuture + Send + 'static,
    Fut::Ok: Send,
    Fut::Error: Send,
{
    pub fn try_spawn_task(
        TaskWiring {
            cancellation_token,
            task_tracker,
            future,
        }: TaskWiring<Fut>,
    ) -> JoinHandle<Option<Result<Fut::Ok, Fut::Error>>> {
        let future = RunUntilCancelled::new(cancellation_token, future.into_future());
        #[cfg(feature = "tracing")]
        let future = tracing::Instrument::in_current_span(future);
        task_tracker.spawn(future)
    }
}
