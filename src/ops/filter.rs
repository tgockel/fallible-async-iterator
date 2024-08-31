use core::{
    pin::Pin,
    task::{Context, Poll},
};

use crate::FallibleAsyncIterator;

/// Return type of the [`filter`][`FallibleAsyncIterator::filter`] operation.
#[derive(Clone, Copy, Debug)]
pub struct Filter<I, P> {
    pub(crate) iter: I,
    pub(crate) predicate: P,
}

impl<I, P> FallibleAsyncIterator for Filter<I, P>
where
    I: FallibleAsyncIterator,
    P: FnMut(&I::Item) -> bool,
{
    type Item = I::Item;
    type Error = I::Error;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<Option<Self::Item>, Self::Error>> {
        loop {
            // safety: projection pin of a field we own
            let iter = unsafe { self.as_mut().map_unchecked_mut(|s| &mut s.iter) };
            let Poll::Ready(potential) = iter.poll_next(cx) else {
                return Poll::Pending;
            };
            if let Ok(Some(item)) = potential.as_ref() {
                // safety: we do not move out of the predicate, we just call it
                let predicate = unsafe { &mut self.as_mut().get_unchecked_mut().predicate };
                if !predicate(item) {
                    // predicate returned false, so skip it
                    continue;
                }
            }
            return Poll::Ready(potential);
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

/// Return type of the [`filter_map`][`FallibleAsyncIterator::filter_map`] operation.
#[derive(Clone, Copy, Debug)]
pub struct FilterMap<I, F> {
    pub(crate) iter: I,
    pub(crate) transform: F,
}

impl<I, F, U> FallibleAsyncIterator for FilterMap<I, F>
where
    I: FallibleAsyncIterator,
    F: FnMut(I::Item) -> Option<U>,
{
    type Item = U;
    type Error = I::Error;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<Option<Self::Item>, Self::Error>> {
        loop {
            // safety: projection pin of a field we own
            let iter = unsafe { self.as_mut().map_unchecked_mut(|s| &mut s.iter) };
            let Poll::Ready(potential) = iter.poll_next(cx) else {
                return Poll::Pending;
            };
            match potential {
                Ok(None) => return Poll::Ready(Ok(None)),
                Err(err) => return Poll::Ready(Err(err)),
                Ok(Some(item)) => {
                    // safety: we do not move out of the predicate, we just call it
                    let transform = unsafe { &mut self.as_mut().get_unchecked_mut().transform };
                    let Some(new_val) = transform(item) else {
                        // no match -- try again
                        continue;
                    };
                    return Poll::Ready(Ok(Some(new_val)));
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}
