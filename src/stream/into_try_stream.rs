use std::convert::Infallible;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::Stream;
#[cfg(doc)]
use futures_util::TryStream;

/// Transforms the provided stream by wrapping every element in `Ok(...)`, so
/// that the resulting stream implements [`TryStream`]. The provided type `E` is
/// used to determine the error type for the [`TryStream`].
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
#[pin_project::pin_project]
pub struct IntoTryStream<St, E = Infallible> {
    #[pin]
    stream: St,
    phantom: PhantomData<E>,
}

impl<St, E> IntoTryStream<St, E> {
    pub(crate) fn new(stream: St) -> Self {
        let phantom = PhantomData;
        Self { stream, phantom }
    }
}

impl<St, E> Stream for IntoTryStream<St, E>
where
    St: Stream,
{
    type Item = Result<St::Item, E>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();
        this.stream.poll_next(cx).map(|o| o.map(Ok))
    }
}
