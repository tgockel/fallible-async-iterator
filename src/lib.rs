#![allow(async_fn_in_trait)]

mod adaptors;
pub use adaptors::*;
mod ops;
pub use ops::*;

use std::{
    fmt,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

#[must_use = "iterators are lazy and do nothing unless consumed"]
pub trait FallibleAsyncIterator {
    /// The type this iterator yields on successful iteration.
    type Item;

    /// The error condition yielded on unsuccessful iteration.
    type Error;

    /// Attempt to poll the next value of this iterator. When using this iterator, you most likely want to use the
    /// [`next`][`FallibleAsyncIterator::next`] function instead.
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<Option<Self::Item>, Self::Error>>;

    /// Get the next value of the iterator.
    fn next(&mut self) -> impl Future<Output = Result<Option<Self::Item>, Self::Error>> {
        NextFuture { iter: self }
    }

    /// Returns the bounds on the remaining length of the iterator.
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, None)
    }

    /// Fold every element into an accumulator by applying a `combine` operation, returning the final result. If the
    /// underlying iterator returns an `Err`, the intermediate result is returned in the `Err` of this function.
    ///
    /// ## Note
    /// Other reduce methods on this trait are implemented in terms of this `fold` operation.
    fn fold<B, F: FnMut(B, Self::Item) -> B>(self, init: B, combine: F) -> Fold<Self, B, F>
    where
        Self: Sized,
    {
        Fold {
            iter: Some(self),
            building: Some(init),
            combine,
        }
    }

    /// Count the elements in the iterator. On interruption, the number of items counted so far is returned.
    ///
    /// ```
    /// # use fallible_async_iterator::*;
    /// # tokio_test::block_on(async {
    /// let counted = std::iter::repeat("hello")
    ///     .take(10)
    ///     .into_fallible_async()
    ///     .count()
    ///     .await
    ///     .unwrap();
    /// assert_eq!(10, counted);
    /// # })
    /// ```
    fn count(self) -> impl Future<Output = Result<usize, Interrupted<Self, usize, Self::Error>>>
    where
        Self: Sized,
    {
        self.fold(0, |c, _| c + 1)
    }

    /// ```
    /// # use fallible_async_iterator::*;
    /// # tokio_test::block_on(async {
    /// let values: Vec<_> = std::iter::repeat(5)
    ///     .take(4)
    ///     .into_fallible_async()
    ///     .collect()
    ///     .await
    ///     .unwrap();
    /// assert_eq!([5, 5, 5, 5], *values);
    /// # })
    /// ```
    async fn collect<B>(self) -> Result<B, Interrupted<Self, B, Self::Error>>
    where
        B: FromFallibleAsyncIterator<Self::Item>,
        Self: Sized,
    {
        B::from_fallible_async_iter(self).await
    }

    async fn collect_into<Target>(
        self,
        collection: &mut Target,
    ) -> Result<&mut Target, Interrupted<Self, &mut Target, Self::Error>>
    where
        Target: Extend<Self::Item>,
        Self: Sized,
    {
        #[cfg(feature = "nightly-extend-one")]
        if let Some(reserve) = self.size_hint().1 {
            collection.extend_reserve(reserve);
        }

        match self.fold((), |(), item| collection.extend(Some(item))).await {
            Ok(()) => Ok(collection),
            Err(partial) => Err(Interrupted {
                iter: partial.iter,
                have: collection,
                with: partial.with,
            }),
        }
    }

    /// When the iterator returns an error case, automatically retry the call if it is `handle`d.
    ///
    /// ## `handle`: `FnMut(Self::Error) -> Result<(), UError>`
    /// Attempt to handle the error, returning `Ok(())` if the error is handled and the iterator should retry. If it can
    /// not be handled, `Err(ex)` is returned and the `next` will return this error.
    fn retry<H>(self, handle: H) -> Retry<Self, H>
    where
        Self: Sized,
    {
        Retry { iter: self, handle }
    }
}

/// Trait for types which can convert into a [`FallibleAsyncIterator`]. This is equivalent to the standard library's
/// `IntoIterator` trait.
pub trait IntoFallibleAsyncIterator {
    type Item;
    type Error;
    type IntoFallibleAsyncIter: FallibleAsyncIterator<Item = Self::Item, Error = Self::Error>;

    fn into_fallible_async_iter(self) -> Self::IntoFallibleAsyncIter;
}

impl<I: FallibleAsyncIterator> IntoFallibleAsyncIterator for I {
    type Item = I::Item;
    type Error = I::Error;
    type IntoFallibleAsyncIter = I;

    fn into_fallible_async_iter(self) -> Self::IntoFallibleAsyncIter {
        self
    }
}

pub trait FromFallibleAsyncIterator<A>: Sized {
    async fn from_fallible_async_iter<I>(
        iter: I,
    ) -> Result<Self, Interrupted<I::IntoFallibleAsyncIter, Self, I::Error>>
    where
        I: IntoFallibleAsyncIterator<Item = A>;
}

/// Represents the error state when an iterator was interrupted while performing a folding operation. It contains the
/// error which occurred, the iterator as it was suspended, and the partial results.
pub struct Interrupted<I, T, E> {
    iter: I,
    have: T,
    with: E,
}

impl<I, T, E> Interrupted<I, T, E> {
    /// The partial results of the operation. The exact meaning varies per operation. For example, an interruption in
    /// [`FallibleAsyncIterator::count`] will contain the number of things counted so far.
    pub fn partial(&self) -> &T {
        &self.have
    }
}

impl<I, T, E> From<Interrupted<I, T, E>> for Result<T, E> {
    fn from(value: Interrupted<I, T, E>) -> Self {
        Err(value.with)
    }
}

impl<I, T, E> From<Interrupted<I, T, E>> for (I, T, E) {
    fn from(value: Interrupted<I, T, E>) -> Self {
        (value.iter, value.have, value.with)
    }
}

impl<I, T, E> From<Interrupted<I, T, E>> for (T, E) {
    fn from(value: Interrupted<I, T, E>) -> Self {
        (value.have, value.with)
    }
}

impl<I, T: fmt::Debug, E: fmt::Debug> fmt::Debug for Interrupted<I, T, E> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Interruped")
            .field("with", &self.with)
            .field("partial", &self.have)
            .finish()
    }
}

/// See [`FallibleAsyncIterator::next`].
struct NextFuture<'a, I: ?Sized> {
    iter: &'a mut I,
}

impl<'a, I: FallibleAsyncIterator + ?Sized> Future for NextFuture<'a, I> {
    type Output = Result<Option<I::Item>, I::Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let iter = unsafe { self.map_unchecked_mut(|s| s.iter) };
        iter.poll_next(cx)
    }
}
