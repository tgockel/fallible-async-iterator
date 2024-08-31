use core::{
    convert::Infallible,
    pin::Pin,
    task::{Context, Poll},
};

use futures_core::Stream;

use crate::FallibleAsyncIterator;

/// Extension methods for `futures_core::Stream`.
pub trait FuturesCoreStreamExt {
    fn into_fallible_async<T>(self) -> FuturesCoreStreamAdaptor<Self>
    where
        Self: Stream<Item = T> + Sized;

    /// Change a [`Stream<Item = Result<T, E>>`][`futures_core::Stream`] to a
    /// [`FallibleAsyncIterator<Item = T, Error = E>`][`FallibleAsyncIterator`].
    fn transpose_into_fallible_async<T, E>(self) -> crate::Transpose<FuturesCoreStreamAdaptor<Self>>
    where
        Self: Stream<Item = Result<T, E>> + Sized,
    {
        crate::Transpose {
            iter: self.into_fallible_async(),
        }
    }
}

impl<S: Stream> FuturesCoreStreamExt for S {
    fn into_fallible_async<T>(self) -> FuturesCoreStreamAdaptor<Self>
    where
        Self: Stream<Item = T> + Sized,
    {
        FuturesCoreStreamAdaptor { stream: self }
    }
}

/// Adapts a `futures_core::Stream` into a [`FallibleAsyncIterator`].
pub struct FuturesCoreStreamAdaptor<S> {
    stream: S,
}

impl<S> FallibleAsyncIterator for FuturesCoreStreamAdaptor<S>
where
    S: Stream,
{
    type Item = S::Item;
    type Error = Infallible;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<Option<Self::Item>, Self::Error>> {
        let stream = unsafe { self.map_unchecked_mut(|s| &mut s.stream) };
        stream.poll_next(cx).map(Ok)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.stream.size_hint()
    }
}

#[cfg(test)]
mod tests {
    #![allow(unused)]

    use core::time::Duration;

    use crate::{FallibleAsyncIterator, FuturesCoreStreamExt};

    #[cfg(feature = "std")]
    #[tokio::test]
    async fn stream_into_vec() {
        let s = async_stream::stream! {
            for i in 1..=3 {
                tokio::time::sleep(Duration::from_millis(5)).await;
                yield i;
            }
        };
        let iter = s.into_fallible_async();
        let out: Vec<_> = iter.collect().await.unwrap();
        assert_eq!([1, 2, 3], *out);
    }
}
