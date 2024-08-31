use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use crate::FallibleAsyncIterator;

/// Return type of the [`map`][`FallibleAsyncIterator::map`] operation.
#[derive(Clone, Copy, Debug)]
pub struct Map<I, F> {
    pub(crate) iter: I,
    pub(crate) transform: F,
}

impl<I, F, R> FallibleAsyncIterator for Map<I, F>
where
    I: FallibleAsyncIterator,
    F: FnMut(I::Item) -> R,
{
    type Item = R;
    type Error = I::Error;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<Option<Self::Item>, Self::Error>> {
        // safety: projection pin of field we own
        let iter = unsafe { self.as_mut().map_unchecked_mut(|s| &mut s.iter) };
        iter.poll_next(cx).map_ok(|item| {
            item.map(|item| {
                // safety: we own transform and will not be moving out of it
                let transform = unsafe { &mut self.get_unchecked_mut().transform };
                transform(item)
            })
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

/// Return type of the [`map`][`FallibleAsyncIterator::map_async`] operation.
#[derive(Clone, Copy, Debug)]
pub struct MapAsync<I, T, F> {
    pub(crate) iter: I,
    pub(crate) transform: T,
    pub(crate) in_progress: Option<F>,
}

impl<I, T, U, F> FallibleAsyncIterator for MapAsync<I, T, F>
where
    I: FallibleAsyncIterator,
    T: FnMut(I::Item) -> F,
    F: Future<Output = U>,
{
    type Item = U;
    type Error = I::Error;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<Option<Self::Item>, Self::Error>> {
        loop {
            if let Some(in_progress) = unsafe { self.as_mut().map_unchecked_mut(|s| &mut s.in_progress).as_pin_mut() } {
                let Poll::Ready(item) = in_progress.poll(cx) else {
                    return Poll::Pending;
                };
                unsafe {
                    self.get_unchecked_mut().in_progress = None;
                }
                return Poll::Ready(Ok(Some(item)));
            }

            let iter = unsafe { self.as_mut().map_unchecked_mut(|s| &mut s.iter) };
            let Poll::Ready(item) = iter.poll_next(cx) else {
                return Poll::Pending;
            };
            match item {
                Ok(None) => return Poll::Ready(Ok(None)),
                Err(err) => return Poll::Ready(Err(err)),
                Ok(Some(item)) => unsafe {
                    let me = self.as_mut().get_unchecked_mut();
                    me.in_progress = Some((me.transform)(item));
                    // loop back to the top to do the polling logic
                    continue;
                },
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}
