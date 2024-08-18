use crate::FallibleAsyncIterator;

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

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context,
    ) -> std::task::Poll<Result<Option<Self::Item>, Self::Error>> {
        let iter = unsafe { self.as_mut().map_unchecked_mut(|s| &mut s.iter) };
        iter.poll_next(cx).map_ok(|item| {
            item.map(|item| {
                let transform = unsafe { &mut self.get_unchecked_mut().transform };
                transform(item)
            })
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}
