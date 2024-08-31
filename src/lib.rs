//! The major motivator for this library is correctly dealing with asynchronous operations which might fail. Typical
//! fallible iteration in Rust uses `Iterator<Item = Result<T, E>>`. The issue with this is the `Err` case is
//! essentially different from the `Ok` case. The iterator did not actually yield an item; it yielded an error. This
//! can lead to bugs when trying to compose operations using the standard `Iterator` transformer functions.
//!
//! As an example, reading records from a remote database might encounter transient network issues. Consider the
//! sequence of events: you read 3 records, there is a minor network glitch, then you read 2 more records. Using a
//! standard `Iterator<Result<i32, &str>>`, you might want to count these things:
//!
//! ```
//! let count = [Ok(1), Ok(2), Ok(3), Err("transient"), Ok(4), Ok(5)]
//!     .into_iter()
//!     .count();
//! assert_eq!(6, count);
//! ```
//!
//! The problem is that you did not actually count 6 things, you only counted 5 things and experienced one error.
//!
//! This library's [`FallibleAsyncIterator`] approaches errors through explicitly handling them. This approach means it
//! is harder to make mistakes by omission. All of the fold-like operations on a `FallibleAsyncIterator` can error with
//! a special [`Interrupted`] value, which contains the reason for interruption, the partially-evaluated state, and an
//! iterator so the operation can be resumed if you wish to.
//!
//! For example, this is the example above using [`count`][`FallibleAsyncIterator::count`]:
//!
//! ```
//! # use fallible_async_iterator::*;
//! # tokio_test::block_on(async {
//! let iter = [Ok(1), Ok(2), Ok(3), Err("transient"), Ok(4), Ok(5)]
//!     .into_iter()
//!     .transpose_into_fallible_async();
//!
//! // Our attempt to count will end in an `Err`
//! let count = iter.count().await;
//! assert!(count.is_err());
//!
//! // The state can be investigated
//! let Interrupted { iter, partial, cause } = count.unwrap_err();
//! assert_eq!(cause, "transient");
//! assert_eq!(partial, 3);
//!
//! // The interrupted iterator can be resumed
//! let remaining = iter.count().await.unwrap();
//! assert_eq!(remaining, 2);
//! assert_eq!(partial + remaining, 5); // <- the correct answer
//! # })
//! ```
//!
//! The "resume on transient failure" pattern is extremely common. To handle these cases, you can use the
//! [`retry`][`FallibleAsyncIterator::retry`] function:
//!
//! ```
//! # use fallible_async_iterator::*;
//! # tokio_test::block_on(async {
//! let count = [Ok(1), Ok(2), Ok(3), Err("transient"), Ok(4), Ok(5)]
//!     .into_iter()
//!     .transpose_into_fallible_async()
//!     .retry(|msg| {
//!         // Make a decision about if we can handle it or not
//!         if msg == "transient" {
//!             Ok(())
//!         } else {
//!             Err(msg)
//!         }
//!     })
//!     .count()
//!     .await
//!     .unwrap();
//! assert_eq!(count, 5);
//! # })
//! ```
#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(feature = "nightly-async-iterator", feature(async_iterator))]
#![cfg_attr(feature = "nightly-extend-one", feature(extend_one))]
// To fix this, we need to create dedicated types for the FallibleAsyncIterator operations which return things like
// `Fold<Self, Blah, fn(Blah, Self::Item) -> Blah>`.
#![allow(clippy::type_complexity)]

mod adaptors;
pub use adaptors::*;
mod ops;
pub use ops::*;

use core::{
    fmt,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

/// A trait for dealing with asynchronous iterators which might fail to iterate.
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

    /// Only yield items which match the `predicate`.
    ///
    /// ```
    /// # use fallible_async_iterator::*;
    /// # tokio_test::block_on(async {
    /// let values: Vec<_> = [1, 2, 3, 4, 5, 6, 7]
    ///     .into_iter()
    ///     .into_fallible_async()
    ///     .filter(|x| x % 2 == 0)
    ///     .collect()
    ///     .await
    ///     .unwrap();
    /// assert_eq!([2, 4, 6], *values);
    /// # })
    /// ```
    fn filter<P>(self, predicate: P) -> Filter<Self, P>
    where
        Self: Sized,
        P: FnMut(&Self::Item) -> bool,
    {
        Filter { iter: self, predicate }
    }

    /// A combination of [`filter`][`FallibleAsyncIterator::filter`] and [`map`][`FallibleAsyncIterator::map`].
    fn filter_map<U, F>(self, transform: F) -> FilterMap<Self, F>
    where
        Self: Sized,
        F: FnMut(Self::Item) -> Option<U>,
    {
        FilterMap { iter: self, transform }
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

    /// Call `observe` on each element successfully yielded from this iterator.
    fn inspect<F>(self, observe: F) -> Inspect<Self, F>
    where
        Self: Sized,
        F: FnMut(&Self::Item),
    {
        Inspect { iter: self, observe }
    }

    /// Call `observe` on each element successfully yielded from this iterator, resuming after the observer future has
    /// completed.
    ///
    /// Because [`async` closures are unstable](https://github.com/rust-lang/rust/issues/62290), the easiest way to use
    /// this is to handle your inspection logic first, then return an awaitable at the end. This works for simple cases
    /// where the actual inspection happens in-band, but you might want to wait on something external later.
    ///
    /// ```
    /// # use fallible_async_iterator::*;
    /// # tokio_test::block_on(async {
    /// # [1, 2, 3]
    /// #     .into_iter()
    /// #     .into_fallible_async()
    ///     .inspect_async(|val| {
    ///         println!("val = {val:?}");
    ///         tokio::time::sleep(core::time::Duration::from_millis(5))
    ///     })
    /// #     .for_each(|_| ())
    /// #     .await
    /// #     .unwrap();
    /// # })
    /// ```
    ///
    /// If you need to use the value inside of the asynchronous operation, this has to be done manually (until Rust has
    /// stable syntax for this). Note that the item is passed to you by reference.
    ///
    /// ```
    /// # use fallible_async_iterator::*;
    /// # tokio_test::block_on(async {
    /// # [1, 2, 3]
    /// #     .into_iter()
    /// #     .into_fallible_async()
    ///     .inspect_async(|val| {
    ///         let val = *val; // <- copy what you need
    ///         async move {
    ///             println!("val = {val:?}"); // <- pretend this couldn't be done in-line
    ///             tokio::time::sleep(core::time::Duration::from_millis(5)).await
    ///         }
    ///     })
    /// #     .for_each(|_| ())
    /// #     .await
    /// #     .unwrap();
    /// # })
    /// ```
    fn inspect_async<F, Fut>(self, observe: F) -> InspectAsync<Self, F, Self::Item, Fut>
    where
        Self: Sized,
        F: FnMut(&Self::Item) -> Fut,
        Fut: Future,
    {
        InspectAsync {
            iter: self,
            observe,
            item: None,
            observing_future: None,
        }
    }

    /// Consumes the iterator, returning the last element.
    ///
    /// If the underlying iterator yields an `Err`, it is returned as an `Err`.
    ///
    /// ```
    /// # use fallible_async_iterator::*;
    /// # tokio_test::block_on(async {
    /// let last = [1, 2, 3]
    ///     .into_iter()
    ///     .into_fallible_async()
    ///     .last()
    ///     .await;
    /// assert!(last.is_ok());
    /// assert_eq!(last.unwrap(), Some(3));
    /// # })
    /// ```
    fn last(self) -> Fold<Self, Option<Self::Item>, fn(Option<Self::Item>, Self::Item) -> Option<Self::Item>>
    where
        Self: Sized,
    {
        self.fold(None, |_, item| Some(item))
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

    /// Takes an async closure and returns an iterator which returns the `transform`ed elements.
    fn map_async<T, F>(self, transform: T) -> MapAsync<Self, T, F>
    where
        Self: Sized,
        T: FnMut(Self::Item) -> F,
        F: Future,
    {
        MapAsync {
            iter: self,
            transform,
            in_progress: None,
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

    /// When the iterator returns an error case, automatically retry the call if it is `handle`d asynchronously.
    ///
    /// This function is similar to [`retry`][`FallibleAsyncIterator::retry`], except the `handle` function can be
    /// `async`. This is useful for implementing things like backoffs.
    ///
    /// ## `handle`: `FnMut(Self::Error) -> impl Future<Output = Result<(), UError>>`
    /// Attempt to handle the error, returning `Ok(())` if the error is handled and the iterator should retry. If it can
    /// not be handled, `Err(ex)` is returned and the `next` will return this error.
    fn retry_async<H, F>(self, handle: H) -> RetryAsync<Self, H, F>
    where
        Self: Sized,
        H: FnMut(Self::Error) -> F,
        F: Future,
    {
        RetryAsync {
            iter: self,
            handle,
            handling_future: None,
        }
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
    /// The suspended iterator. This
    pub iter: I,

    /// The partial results of the operation. The exact meaning varies per operation. For example, an interruption in
    /// [`FallibleAsyncIterator::count`] will contain the number of things counted so far.
    pub partial: T,

    /// The value of the `Err` the iterator was interrupted with.
    pub cause: E,
}

impl<I, T, E> From<Interrupted<I, T, E>> for Result<T, E> {
    fn from(value: Interrupted<I, T, E>) -> Self {
        Err(value.cause)
    }
}

impl<I, T, E> From<Interrupted<I, T, E>> for (I, T, E) {
    fn from(value: Interrupted<I, T, E>) -> Self {
        (value.iter, value.partial, value.cause)
    }
}

impl<I, T, E> From<Interrupted<I, T, E>> for (T, E) {
    fn from(value: Interrupted<I, T, E>) -> Self {
        (value.partial, value.cause)
    }
}

impl<I, T: fmt::Debug, E: fmt::Debug> fmt::Debug for Interrupted<I, T, E> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Interruped")
            .field("cause", &self.cause)
            .field("partial", &self.partial)
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
