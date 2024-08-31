use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use crate::{FallibleAsyncIterator, Interrupted};

/// The return type of [`fold`][`FallibleAsyncIterator::fold`] operations.
///
/// This can not be used directly. This structure hold the common elements for performing eduction operations such as
/// [`fold`][`FallibleAsyncIterator::fold`], [`count`][`FallibleAsyncIterator::count`],
/// [`collect`][`FallibleAsyncIterator::collect`], etc. Anything that consumes an iterator and returns a single value
/// through a [`Future`] can be implemented through a fold.
///
/// This structure exists so derivable traits like [`Clone`] and [`Copy`] and auto traits like [`Send`] and [`Sync`] can
/// be derived correctly. In contrast, having [`FallibleAsyncIterator`] operations return an `impl Future<Output = ...>`
/// provides no mechanism to pick these traits up. There is no Rust syntax for "make the return type `Clone` if you can,
/// but still return a `!Clone` if that is not possible."
#[derive(Clone, Copy, Debug)]
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
                        partial: building,
                        cause: e,
                    }))
                }
            };
        }
    }
}

/// The return type of fold-like operations which need additional context.
///
/// This can not be used directly. It is used by functions like [`for_each`][`FallibleAsyncIterator::for_each`] to
/// perform a reduce operation, but provides an explicit context function instead of embedding the closure in the `F`
/// parameter.
#[derive(Clone, Copy, Debug)]
pub struct FoldWith<I: FallibleAsyncIterator, C, B> {
    pub(crate) iter: Option<I>,
    pub(crate) context: C,
    pub(crate) building: Option<B>,
    pub(crate) combine: fn(&mut C, B, I::Item) -> B,
}

impl<I, C, B> Future for FoldWith<I, C, B>
where
    I: FallibleAsyncIterator,
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
                    this.building = Some((this.combine)(&mut this.context, building, val));
                }
                Err(e) => {
                    return Poll::Ready(Err(Interrupted {
                        iter: this.iter.take().unwrap(),
                        partial: building,
                        cause: e,
                    }))
                }
            };
        }
    }
}
