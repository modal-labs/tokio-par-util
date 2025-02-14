use futures_util::stream::{TryBufferUnordered, TryBuffered};
use futures_util::{TryFuture, TryStream, TryStreamExt};

pub trait TryFutureBuffer<St>: TryStream
where
    St: TryStream,
    St::Ok: TryFuture<Error = St::Error>,
{
    fn try_buffer(stream: St, limit: usize) -> Self;
}

impl<St> TryFutureBuffer<St> for TryBuffered<St>
where
    St: TryStream,
    St::Ok: TryFuture<Error = St::Error>,
{
    fn try_buffer(stream: St, limit: usize) -> Self {
        stream.try_buffered(limit)
    }
}

impl<St> TryFutureBuffer<St> for TryBufferUnordered<St>
where
    St: TryStream,
    St::Ok: TryFuture<Error = St::Error>,
{
    fn try_buffer(stream: St, limit: usize) -> Self {
        stream.try_buffer_unordered(limit)
    }
}
