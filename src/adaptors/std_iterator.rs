use core::{
    convert::Infallible,
    pin::Pin,
    task::{Context, Poll},
};

use crate::FallibleAsyncIterator;

/// Extension methods for `std::iter::Iterator`.
pub trait IteratorExt {
    fn into_fallible_async<T>(self) -> IteratorAdaptor<Self>
    where
        Self: Iterator<Item = T> + Sized;
}

impl<I: Iterator> IteratorExt for I {
    fn into_fallible_async<T>(self) -> IteratorAdaptor<I>
    where
        Self: Iterator<Item = T> + Sized,
    {
        IteratorAdaptor { iter: self }
    }
}

/// Adapts a `std::iter::Iterator` into a [`FallibleAsyncIterator`].
pub struct IteratorAdaptor<I> {
    iter: I,
}

impl<I: Iterator + Unpin> FallibleAsyncIterator for IteratorAdaptor<I> {
    type Item = I::Item;
    type Error = Infallible;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<Option<Self::Item>, Self::Error>> {
        Poll::Ready(Ok(self.iter.next()))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}
