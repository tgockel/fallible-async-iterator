mod std_iterator;
pub use std_iterator::{IteratorAdaptor, IteratorExt};

#[cfg(feature = "nightly-async-iterator")]
mod std_async_iterator;
#[cfg(feature = "nightly-async-iterator")]
pub use std_async_iterator::{AsyncIteratorAdaptor, AsyncIteratorExt};

#[cfg(feature = "futures-core")]
mod futures_core;
#[cfg(feature = "futures-core")]
pub use futures_core::{FuturesCoreStreamAdaptor, FuturesCoreStreamExt};
