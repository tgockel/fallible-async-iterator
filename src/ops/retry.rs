use crate::FallibleAsyncIterator;

#[derive(Clone)]
pub struct Retry<I, H> {
    pub(crate) iter: I,
    pub(crate) handle: H,
}

impl<I, H, E> FallibleAsyncIterator for Retry<I, H>
where
    I: FallibleAsyncIterator,
    H: FnMut(I::Error) -> Result<(), E>,
{
    type Item = I::Item;
    type Error = E;

    async fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        loop {
            match self.iter.next().await {
                Ok(x) => break Ok(x),
                Err(e) => {
                    // try to handle the error
                    let Err(e) = (self.handle)(e) else {
                        continue;
                    };
                    break Err(e);
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}
