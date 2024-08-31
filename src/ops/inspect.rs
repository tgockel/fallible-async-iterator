use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use crate::FallibleAsyncIterator;

/// Return type of the [`inspect`][`FallibleAsyncIterator::inspect`] operation.
#[derive(Clone, Copy, Debug)]
pub struct Inspect<I, F> {
    pub(crate) iter: I,
    pub(crate) observe: F,
}

impl<I, F> FallibleAsyncIterator for Inspect<I, F>
where
    I: FallibleAsyncIterator,
    F: FnMut(&I::Item),
{
    type Item = I::Item;
    type Error = I::Error;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<Option<Self::Item>, Self::Error>> {
        // safety: projection pin of field we own
        let iter = unsafe { self.as_mut().map_unchecked_mut(|s| &mut s.iter) };
        iter.poll_next(cx).map_ok(|item| {
            if let Some(item) = item.as_ref() {
                // safety: we own `observe` and will not be moving out of it
                let observe = unsafe { &mut self.get_unchecked_mut().observe };
                observe(item);
            }

            item
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

/// Return type of the [`inspect_async`][`FallibleAsyncIterator::inspect_async`] operation.
#[derive(Clone, Copy, Debug)]
pub struct InspectAsync<I, F, Item, Fut> {
    pub(crate) iter: I,
    pub(crate) observe: F,
    pub(crate) item: Option<Item>,
    pub(crate) observing_future: Option<Fut>,
}

impl<I, F, Fut> FallibleAsyncIterator for InspectAsync<I, F, I::Item, Fut>
where
    I: FallibleAsyncIterator,
    F: FnMut(&I::Item) -> Fut,
    Fut: Future<Output = ()>,
{
    type Item = I::Item;
    type Error = I::Error;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<Option<Self::Item>, Self::Error>> {
        loop {
            if let Some(observing) =
                unsafe { self.as_mut().map_unchecked_mut(|s| &mut s.observing_future) }.as_pin_mut()
            {
                let Poll::Ready(_) = observing.poll(cx) else {
                    return Poll::Pending;
                };
                unsafe {
                    let me = self.get_unchecked_mut();
                    me.observing_future = None;
                    return Poll::Ready(Ok(me.item.take()));
                };
            }

            // safety: projection pin of field we own
            let iter = unsafe { self.as_mut().map_unchecked_mut(|s| &mut s.iter) };
            let Poll::Ready(intermediate) = iter.poll_next(cx) else {
                return Poll::Pending;
            };
            match intermediate {
                Ok(None) => return Poll::Ready(Ok(None)),
                Err(err) => return Poll::Ready(Err(err)),
                Ok(Some(item)) => unsafe {
                    let me = self.as_mut().get_unchecked_mut();
                    me.observing_future = Some((me.observe)(&item));
                    me.item = Some(item);
                    continue;
                },
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

#[cfg(test)]
mod tests {
    use core::{iter::repeat, time::Duration};

    use crate::{FallibleAsyncIterator, IteratorExt};

    #[tokio::test]
    async fn inspect_async_with_sleep() {
        repeat(10)
            .take(10)
            .into_fallible_async()
            .inspect_async(|val| {
                #[cfg(feature = "std")]
                println!("val = {val}");
                tokio::time::sleep(Duration::from_millis(*val))
            })
            .for_each(|_| ())
            .await
            .unwrap();
    }
}
