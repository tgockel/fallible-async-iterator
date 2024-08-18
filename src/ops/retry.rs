use core::{
    pin::Pin,
    task::{Context, Poll},
};

use crate::FallibleAsyncIterator;

/// The return type of the [`retry`][`FallibleAsyncIterator::retry`] operation.
#[derive(Clone, Copy, Debug)]
pub struct Retry<I, H> {
    pub(crate) iter: I,
    pub(crate) handle: H,
}

impl<I, H, E> FallibleAsyncIterator for Retry<I, H>
where
    I: FallibleAsyncIterator,
    H: Fn(I::Error) -> Result<(), E>,
{
    type Item = I::Item;
    type Error = E;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<Option<Self::Item>, Self::Error>> {
        loop {
            // safety: projection pin of field we own
            let iter = unsafe { self.as_mut().map_unchecked_mut(|s| &mut s.iter) };
            let Poll::Ready(intermediate) = iter.poll_next(cx) else {
                return Poll::Pending;
            };
            match intermediate {
                Ok(normal) => return Poll::Ready(Ok(normal)),
                Err(ex) => {
                    if let Err(handled) = (self.handle)(ex) {
                        return Poll::Ready(Err(handled));
                    }
                    // error was handled -- continue to loop
                    continue;
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}
