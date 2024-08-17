mod std_iterator;
pub use std_iterator::{IteratorAdaptor, IteratorExt};

#[cfg(feature = "futures-core")]
mod futures_core;
#[cfg(feature = "futures-core")]
pub use futures_core::{FuturesCoreStreamAdaptor, FuturesCoreStreamExt};
