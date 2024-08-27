use core::{
    pin::Pin,
    task::{Context, Poll},
};

use crate::FallibleAsyncIterator;

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
