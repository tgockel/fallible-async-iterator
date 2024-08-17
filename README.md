`fallible-async-iterator`
=========================

Traits and implementations for an iterator which runs asynchronously and might fail.
It has the same motivations as [`async-iterator`][async-iterator] and [`fallible-iterator`][fallible-iterator], but
fused together.

### Why not `AsyncIterator<Item = Result<...>>`?

One could imagine a `FallibleAsyncIterator<Item = T, Error = E>` implemented as as yielding `Result`s instead via
`AsyncIterator<Item = Result<T, E>>`.
The meaning of an error in `next` is subtly different between these two types.

In this library, `next` returning an `Err` state means that it failed to reach the next item.
With an [`AsyncIterator`][async-iterator], `Some(Err(...))` means that the item itself is an error, but iteration
performed successfully.

### Why not `___-stream`?

There are a few libraries that are also meant for handling asynchronous streams like [`async-stream`][async-stream] and
[`tokio-stream`][tokio-stream].
These libraries are extremely useful and flexible, allowing you to do whatever you want by writing a `stream!` macro.
By contrast, this library focuses on an iterator-like pattern of building blocks, which solves the common problems of "I
want to collect items into a `Vec`" and such.
Some problems are better-suited for writing as a stream, while others are going to be easier to express as
transformations on an iterator.

A `FallibleAsyncIterator` can work with a stream library through the [`futures-core`][futures-core] library.
By enabling the [futures-core] feature and a `use fallible_async_iterator::FuturesCoreStreamExt`, you get access to
`into_fallible_async()` and `transpose_into_fallible_async()` functions, which allow you to consume a
[`futures_core::Stream`][futures-core-stream] (the common component the `___-stream` libraries build) as a
`FallibleAsyncIterator`.

```rust
use fallible_async_iterator::{FallibleAsyncIterator, FuturesCoreStreamExt};

#[tokio::main]
async fn main() {
    // create a demo stream
    let s = async_stream::stream! {
        for i in 1..=3 {
            // some normal stream operation
            yield i;
        }
    };
    // convert to this library's FallibleAsyncIterator
    let iter = s.into_fallible_async();
    // use the standard chaining commands
    let out: Vec<_> = iter.map(|x| x * 2).collect().await;
    println!("out = {out:?}");
}
```

[async-stream]: https://crates.io/crates/async-stream
[async-iterator]: https://crates.io/crates/async-iterator
[fallible-iterator]: https://crates.io/crates/fallible-iterator
[futures-core]: https://crates.io/crates/futures-core
[futures-core-stream]: https://docs.rs/futures-core/0.3.30/futures_core/stream/trait.Stream.html
[tokio-stream]: https://crates.io/crates/tokio-stream
