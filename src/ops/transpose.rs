use std::{
    convert::Infallible,
    pin::Pin,
    task::{Context, Poll},
};

use crate::FallibleAsyncIterator;

/// The return type of the [`transpose`][`FallibleAsyncIterator::transpose`] operation.
#[derive(Clone, Copy, Debug)]
pub struct Transpose<I> {
    pub(crate) iter: I,
}

impl<I, T, E> FallibleAsyncIterator for Transpose<I>
where
    I: FallibleAsyncIterator<Item = Result<T, E>, Error = Infallible>,
{
    type Item = T;
    type Error = E;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<Option<Self::Item>, Self::Error>> {
        // safety: projection pin of field we own
        let iter = unsafe { self.map_unchecked_mut(|s| &mut s.iter) };

        iter.poll_next(cx).map(|x| x.unwrap().transpose())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}
