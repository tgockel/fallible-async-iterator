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

[async-iterator]: https://crates.io/crates/async-iterator
[fallible-iterator]: https://crates.io/crates/fallible-iterator
