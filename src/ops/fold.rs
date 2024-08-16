use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use crate::{FallibleAsyncIterator, Interrupted};

pub struct Fold<I, B, F> {
    pub(crate) iter: Option<I>,
    pub(crate) building: Option<B>,
    pub(crate) combine: F,
}

impl<I, B, F> Future for Fold<I, B, F>
where
    I: FallibleAsyncIterator,
    F: FnMut(B, I::Item) -> B,
{
    type Output = Result<B, Interrupted<I, B, I::Error>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        loop {
            let iter = unsafe {
                self.as_mut()
                    .map_unchecked_mut(|s| s.iter.as_mut().expect("underlying iterator has already completed"))
            };
            let Poll::Ready(intermediate) = iter.poll_next(cx) else {
                return Poll::Pending;
            };

            let this = unsafe { self.as_mut().get_unchecked_mut() };
            let building = this.building.take().expect("result has already been returned");
            match intermediate {
                Ok(None) => return Poll::Ready(Ok(building)),
                Ok(Some(val)) => {
                    this.building = Some((this.combine)(building, val));
                }
                Err(e) => {
                    return Poll::Ready(Err(Interrupted {
                        iter: this.iter.take().unwrap(),
                        have: building,
                        with: e,
                    }))
                }
            };
        }
    }
}
