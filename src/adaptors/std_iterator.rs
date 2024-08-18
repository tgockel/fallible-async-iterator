use std::{convert::Infallible, task::Poll};

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

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<Option<Self::Item>, Self::Error>> {
        Poll::Ready(Ok(self.iter.next()))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}
