use std::{
    async_iter::AsyncIterator,
    convert::Infallible,
    pin::Pin,
    task::{Context, Poll},
};

use crate::FallibleAsyncIterator;

/// Extension methods for [`std::async_iter::AsyncIterator`].
pub trait AsyncIteratorExt {
    fn into_fallible_async<T>(self) -> AsyncIteratorAdaptor<Self>
    where
        Self: Iterator<Item = T> + Sized;
}

impl<I: AsyncIterator> AsyncIteratorExt for I {
    fn into_fallible_async<T>(self) -> AsyncIteratorAdaptor<Self>
    where
        Self: Iterator<Item = T> + Sized,
    {
        AsyncIteratorAdaptor { iter: self }
    }
}

/// Adapts an [`std::async_iter::AsyncIterator`] into a [`FallibleAsyncIterator`].
pub struct AsyncIteratorAdaptor<I> {
    iter: I,
}

impl<I> FallibleAsyncIterator for AsyncIteratorAdaptor<I>
where
    I: AsyncIterator,
{
    type Item = I::Item;
    type Error = Infallible;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<Option<Self::Item>, Self::Error>> {
        let iter = unsafe { self.map_unchecked_mut(|s| &mut s.iter) };
        iter.poll_next(cx).map(Ok)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}
