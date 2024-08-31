use core::{
    future::Future,
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

/// The return type of the [`retry_async`][`FallibleAsyncIterator::retry_async`] operation.
#[derive(Clone, Copy, Debug)]
pub struct RetryAsync<I, H, F> {
    pub(crate) iter: I,
    pub(crate) handle: H,
    pub(crate) handling_future: Option<F>,
}

impl<I, H, F, E> FallibleAsyncIterator for RetryAsync<I, H, F>
where
    Self: Unpin,
    I: FallibleAsyncIterator,
    H: FnMut(I::Error) -> F,
    F: Future<Output = Result<(), E>>,
{
    type Item = I::Item;
    type Error = E;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<Option<Self::Item>, Self::Error>> {
        loop {
            // first, check if we were handling the future
            // safety: projection pin of field we own
            if let Some(handling_future) =
                unsafe { self.as_mut().map_unchecked_mut(|s| &mut s.handling_future) }.as_pin_mut()
            {
                let Poll::Ready(done) = handling_future.poll(cx) else {
                    return Poll::Pending;
                };
                self.as_mut().handling_future = None;
                if let Err(err) = done {
                    return Poll::Ready(Err(err));
                }
            }

            // try to iterate
            // safety: projection pin of field we own
            let iter = unsafe { self.as_mut().map_unchecked_mut(|s| &mut s.iter) };
            let Poll::Ready(intermediate) = iter.poll_next(cx) else {
                return Poll::Pending;
            };
            match intermediate {
                Ok(normal) => return Poll::Ready(Ok(normal)),
                Err(ex) => {
                    self.as_mut().handling_future = Some((self.handle)(ex));
                    continue;
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}
