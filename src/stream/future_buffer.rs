/// Internal utilities for keeping track of a collection of futures.
use std::future::Future;

use futures_util::stream::{BufferUnordered, Buffered};
use futures_util::{Stream, StreamExt};

pub trait FutureBuffer<St>: Stream<Item = <St::Item as Future>::Output>
where
    St: Stream,
    St::Item: Future,
{
    fn buffer(stream: St, limit: usize) -> Self;
}

impl<St> FutureBuffer<St> for Buffered<St>
where
    St: Stream,
    St::Item: Future,
{
    fn buffer(stream: St, limit: usize) -> Self {
        stream.buffered(limit)
    }
}

impl<St> FutureBuffer<St> for BufferUnordered<St>
where
    St: Stream,
    St::Item: Future,
{
    fn buffer(stream: St, limit: usize) -> Self {
        stream.buffer_unordered(limit)
    }
}
