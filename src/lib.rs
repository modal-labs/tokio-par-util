//! # `tokio-par-util`: Utilities for running computations in parallel on top of Tokio.
//!
//! This library adds utility methods and stream transformers to [`Stream`]s and
//! [`TryStream`]s, to make it easier to run many futures in parallel while
//! adhering to structured parallelism best-practices. Each stream transformer
//! propagates panics and cancellation correctly, and ensures that tasks aren't
//! leaked, that the program waits for `Drop` impls to run on other tasks before
//! continuing execution, etc.
//!
//! ## Usage
//!
//! To use this library, simply call `parallel_*` methods on [`Stream`]s or
//! [`TryStream`]s by importing the extension traits, [`StreamParExt`] or
//! [`TryStreamParExt`].
//!
//! ### Regular streams
//!
//! When dealing with a regular [`Stream`], when you want to process a stream of
//! values with some async computation that cannot fail, you'd use the
//! [`StreamParExt`] extension trait.
//!
//! There's the option to process input elements in parallel and emitting the
//! results in the same order as the inputs, in which case you'd use
//! [`StreamParExt::parallel_buffered`]. There's also the option to not preserve
//! input order, in which case you'd use
//! [`StreamParExt::parallel_buffer_unordered`].  It might usually be beneficial
//! to use the `_unordered` version when possible, since that prevents stalling
//! the output on a future that is slow to produce its result.
//!
//! ```rust
//! use futures_util::{stream, StreamExt as _};
//! use std::collections::HashSet;
//! use tokio_par_util::StreamParExt as _;
//!
//! # #[tokio::main]
//! # async fn main() {
//! // This is a stream of futures. In a real-world example, each future would do
//! // something more complex; here, we use a dummy stream just as an example.
//! let stream = stream::iter([1, 2, 3, 4]).map(|i| async move { 2 * i });
//!
//! // Consume a stream with up to 32 parallel workers
//! let ints: Vec<_> = stream.parallel_buffered(32).collect().await;
//! assert_eq!(&ints, &[2, 4, 6, 8]);
//!
//! // Consume a stream with up to 32 parallel workers, not preserving order
//! let stream = stream::iter([1, 2, 3, 4]).map(|i| async move { 2 * i });
//! let ints: HashSet<_> = stream.parallel_buffer_unordered(32).collect().await;
//! assert_eq!(ints, HashSet::from_iter([2, 4, 6, 8]));
//! # }
//! ```
//!
//! The semantics of the resulting streams are that futures will be scheduled in
//! parallel, and the library will try to schedule as many tasks as possible,
//! bounded by the specified limit (in the above example, `32`).  The stream
//! semantics are fully preserved, so it is possible to start processing the
//! result of the first future as it completes, even if we have not fully
//! consumed the input stream or managed to fill up all worker slots, or
//! similar.
//!
//! A future may panic, in which case the panic is immediately propagated to the
//! calling task:
//!
//! ```rust
//! use futures_util::{stream, StreamExt as _};
//! use tokio::task;
//! use tokio_par_util::StreamParExt as _;
//!
//! # #[tokio::main]
//! # async fn main() {
//! let stream = stream::iter([1, 2, 3, 4]).map(|i| async move {
//!     if i == 3 {
//!         panic!("I don't like the number 3")
//!     } else {
//!         2 * i
//!     }
//! });
//!
//! // Spawn a task so that we're able to catch the panic and inspect its payload.
//! let task = task::spawn(stream.parallel_buffered(32).collect::<Vec<_>>());
//!
//! // We expect the task to fail with a panic
//! let err = task.await.err().unwrap();
//! let panic_msg = *err.into_panic().downcast_ref::<&'static str>().unwrap();
//! assert_eq!(panic_msg, "I don't like the number 3");
//! # }
//! ```
//!
//! Some code is not cancellation-safe, in the sense that some more expensive
//! clean-up is needed in order to cancel a future. To support that use-case,
//! this library exposes the ability to cancel computation via a
//! [`CancellationToken`], which enables some apps to implement graceful
//! shutdown.
//!
//! The semantics exposed by this library are that a stream that gets canceled
//! via a token will drop its input stream, stop producing new output items, but
//! by default still wait for any spawned tasks to finish before reporting
//! end-of-stream.
//!
//! To learn more about graceful shutdown, consult the
//! [Tokio official docs](https://tokio.rs/tokio/topics/shutdown) on the subject.
//!
//! ```rust
//! use futures_util::{stream, StreamExt as _};
//! use std::collections::HashSet;
//! use tokio_par_util::StreamParExt as _;
//! use tokio_util::sync::CancellationToken;
//!
//! # #[tokio::main]
//! # async fn main() {
//! // A cancellation token that would normally be passed-in by some surrounding
//! // code that requires graceful shutdown
//! let cancellation_token = CancellationToken::new();
//!
//! let stream = stream::iter([1, 2, 3, 4]).map(|i| async move { 2 * i });
//! let ints: Vec<_> = stream.parallel_buffered_with_token(32, cancellation_token.clone()).collect().await;
//! assert_eq!(&ints, &[2, 4, 6, 8]);
//!
//! let stream = stream::iter([1, 2, 3, 4]).map(|i| async move { 2 * i });
//! let ints: HashSet<_> = stream.parallel_buffer_unordered_with_token(32, cancellation_token.clone()).collect().await;
//! assert_eq!(ints, HashSet::from_iter([2, 4, 6, 8]));
//! # }
//! ```
//!
//! ### Streams and computations that may fail
//!
//! If you need to model fallible operations in a stream, you will most likely
//! be using a [`TryStream`]. When dealing with such a stream, the API is very
//! similar to when using a normal [`Stream`], except that any error returned by
//! the stream will short-circuit the stream as quickly as possible.
//!
//! This crate also offers an [`StreamParExt::into_try_stream`] utility method
//! to turn a normal [`Stream`] into a [`TryStream`] if you then want to chain
//! on some fallible computation.
//!
//! ```rust
//! use futures_util::{stream, TryStreamExt as _};
//! use tokio_par_util::TryStreamParExt as _;
//!
//! # #[tokio::main]
//! # async fn main() {
//! // A stream that is successful:
//! let stream = stream::iter([Ok(1), Ok(2), Ok(3), Ok(4)]).map_ok(|i| async move { Ok(2 * i) });
//! let ints: Result<Vec<_>, String> = stream.try_parallel_buffered(32).try_collect().await;
//! assert_eq!(ints, Ok(vec![2, 4, 6, 8]));
//!
//! // A stream where the input stream contains an error:
//! let stream = stream::iter([Ok(1), Ok(2), Err("failed".to_owned()), Ok(4)]).map_ok(|i| async move { Ok(2 * i) });
//! let ints: Result<Vec<_>, String> = stream.try_parallel_buffered(32).try_collect().await;
//! assert_eq!(ints, Err("failed".to_owned()));
//!
//! // A stream where a future produces an error:
//! let stream = stream::iter([Ok(1), Ok(2), Ok(3), Ok(4)]).map_ok(|i| async move {
//!     if i == 3 {
//!         Err("failed".to_owned())
//!     } else {
//!         Ok(2 * i)
//!     }
//! });
//! let ints: Result<Vec<_>, String> = stream.try_parallel_buffered(32).try_collect().await;
//! assert_eq!(ints, Err("failed".to_owned()));
//! # }
//! ```
//!
//! The above example only used [`TryStreamParExt::try_parallel_buffered`], but
//! there is of course also [`TryStreamParExt::try_parallel_buffer_unordered`]
//! which behaves very similarly, while not preserving input stream order.
#![deny(clippy::all)]
#![deny(missing_docs)]

mod future;
pub mod stream;
mod task_wiring;
pub mod try_stream;

#[cfg(doc)]
use futures_util::Stream;
#[cfg(doc)]
use futures_util::TryStream;
pub use stream::StreamParExt;
#[cfg(doc)]
use tokio_util::sync::CancellationToken;
pub use try_stream::TryStreamParExt;
