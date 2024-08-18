#![cfg_attr(feature = "nightly-async-iterator", feature(async_iterator))]
#![cfg_attr(feature = "nightly-extend-one", feature(extend_one))]

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
    fn count(self) -> Fold<Self, usize, fn(usize, Self::Item) -> usize>
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
    ///
    /// ## Note
    /// Unlike the Rust Standard Library's [`Iterator::collect`] function, there is no equivalent to the
    /// [`FromIterator`] trait for the target container to collect into. Instead, functionality is built on top of the
    /// [`Extend`] and [`Default`] traits. This is done for a few reasons:
    ///
    /// 1. There is not an obvious way to implement a `FromIterator` equivalent without having an `async` trait (which
    ///    is unstable and will be for the forseeable future).
    /// 2. This unifies behavior with the [`collect_into`][`FallibleAsyncIterator::collect_into`] functionality, which
    ///    only requires you can `extend` from items.
    /// 3. There are a [`Extend`] implementations for many types, but no implementations for a hypothetical
    ///    `FromFallibleAsyncIterator` trait, so it is easier to get support for a lot of types by just calling their
    ///    `Extend` implementation.
    ///
    /// If your target container is not default-constructible, construct it ahead-of-time and use
    /// [`collect_into`][`FallibleAsyncIterator::collect_into`] instead.
    ///
    /// A downside to this approach is that you do not get the reserve space for your container when the source iterator
    /// has a [`size_hint`][`FallibleAsyncIterator::size_hint`]. This can be enabled with the `nightly-extend-one`
    /// feature, which enables this functionality with [`Extend::extend_reserve`] (which is an unstable API as of Rust
    /// 1.80).
    fn collect<B>(self) -> Fold<Self, B, fn(B, Self::Item) -> B>
    where
        B: Extend<Self::Item> + Default,
        Self: Sized,
    {
        #[allow(unused_mut)]
        let mut collection: B = Default::default();
        #[cfg(feature = "nightly-extend-one")]
        if let Some(reserve) = self.size_hint().1 {
            collection.extend_reserve(reserve);
        }

        self.fold(collection, |mut collection, item| {
            #[cfg(feature = "nightly-extend-one")]
            collection.extend_one(item);
            #[cfg(not(feature = "nightly-extend-one"))]
            collection.extend(Some(item));
            collection
        })
    }

    /// Collect the items yielded from this iterator into the `collection`.
    ///
    /// This function is similar to [`collect`][`FallibleAsyncIterator::collect`], but operates on a collection you have
    /// created ahead-of-time. This is useful for appending to an already-existing container or when the target
    /// container does not have a [`Default`] implementation.
    fn collect_into<Target>(
        self,
        collection: &mut Target,
    ) -> Fold<Self, &mut Target, fn(&mut Target, Self::Item) -> &mut Target>
    where
        Target: Extend<Self::Item>,
        Self: Sized,
    {
        #[cfg(feature = "nightly-extend-one")]
        if let Some(reserve) = self.size_hint().1 {
            collection.extend_reserve(reserve);
        }

        self.fold(collection, |collection, item| {
            #[cfg(feature = "nightly-extend-one")]
            collection.extend_one(item);
            #[cfg(not(feature = "nightly-extend-one"))]
            collection.extend(Some(item));
            collection
        })
    }

    /// Perform an `action` on each item yielded from this iterator.
    ///
    /// ```
    /// # use fallible_async_iterator::*;
    /// # tokio_test::block_on(async {
    /// let mut output = Vec::new();
    /// [1, 2, 3]
    ///     .into_iter()
    ///     .into_fallible_async()
    ///     .for_each(|item| output.push(item))
    ///     .await
    ///     .unwrap();
    /// assert_eq!([1, 2, 3], *output);
    /// # })
    /// ```
    fn for_each<F>(self, action: F) -> FoldWith<Self, F, ()>
    where
        Self: Sized,
        F: FnMut(Self::Item),
    {
        FoldWith {
            iter: Some(self),
            context: action,
            building: Some(()),
            combine: |action, (), item| action(item),
        }
    }

    /// Takes a closure and returns an iterator which returns the `transform`ed elements.
    ///
    /// ```
    /// # use fallible_async_iterator::*;
    /// # tokio_test::block_on(async {
    /// let values: Vec<_> = [1, 2, 3]
    ///     .into_iter()
    ///     .into_fallible_async()
    ///     .map(|x| x * 2)
    ///     .collect()
    ///     .await
    ///     .unwrap();
    /// assert_eq!([2, 4, 6], *values);
    /// # })
    /// ```
    fn map<F>(self, transform: F) -> Map<Self, F>
    where
        Self: Sized,
    {
        Map { iter: self, transform }
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

    /// Extract the `Err` from item and expose it as the `Error` type.
    ///
    /// When this iterator's `Item` is a [`Result`] and the `Error` is [`Infallible`][`std::convert::Infallible`], this
    /// function lifts the `Err` results into the [`Error`][`FallibleAsyncIterator::Error`] and puts `Ok` results as the
    /// [`Item`][`FallibleAsyncIterator::Item`].
    ///
    /// This function is useful for converting from sources whose iterators give you a [`Result`] and the error
    /// condition aligns with this library's expectations of errors.
    ///
    /// ```
    /// # use fallible_async_iterator::*;
    /// # tokio_test::block_on(async {
    /// let count =
    ///     [Ok(1), Ok(2), Ok(3), Err("error"), Ok(4), Err("another"), Ok(5)]
    ///     .into_iter()
    ///     .into_fallible_async()
    ///     .transpose()                 // <- convert internal `Err` into iterator `Error`s
    ///     .retry(|_| Ok::<(), ()>(())) // <- fake error handler just discards
    ///     .count()
    ///     .await
    ///     .unwrap();
    /// assert_eq!(5, count);
    /// # })
    /// ```
    fn transpose(self) -> Transpose<Self>
    where
        Self: Sized,
    {
        Transpose { iter: self }
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
        // safety: `iter` will not be moved-from, as we have an exclusive borrow
        let iter = unsafe { self.map_unchecked_mut(|s| s.iter) };
        iter.poll_next(cx)
    }
}
