use std::convert::Infallible;

use crate::{FallibleAsyncIterator, FromFallibleAsyncIterator, IntoFallibleAsyncIterator};

pub struct IteratorAdaptor<I> {
    iter: I,
}

impl<I: Iterator> FallibleAsyncIterator for IteratorAdaptor<I> {
    type Item = I::Item;
    type Error = Infallible;

    async fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        Ok(self.iter.next())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

pub trait IteratorExt {
    fn into_fallible_async<T>(self) -> IteratorAdaptor<Self>
    where Self: Iterator<Item = T> + Sized;
}

impl<I: Iterator> IteratorExt for I {
    fn into_fallible_async<T>(self) -> IteratorAdaptor<I>
    where Self: Iterator<Item = T> + Sized {
            IteratorAdaptor { iter: self }
    }
}

impl<A> FromFallibleAsyncIterator<A> for Vec<A> {
    async fn from_fallible_async_iter<I>(
        iter: I,
    ) -> Result<Self, crate::Interrupted<I::IntoFallibleAsyncIter, Self, I::Error>>
    where
        I: IntoFallibleAsyncIterator<Item = A>,
    {
        let iter = iter.into_fallible_async_iter();
        let init = Vec::with_capacity(iter.size_hint().1.unwrap_or_default());
        iter.fold(init, |mut out, item| {
            out.push(item);
            out
        })
        .await
    }
}
