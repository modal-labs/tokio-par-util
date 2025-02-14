# `tokio-par-util`

> Utilities for running computations in parallel on top of Tokio

This library adds utility methods and stream transformers to `Stream`s and
`TryStream`s, to make it easier to run many futures in parallel while
adhering to structured parallelism best-practices. Each stream transformer
propagates panics and cancellation correctly, and ensures that tasks aren't
leaked, that the program waits for `Drop` impls to run on other tasks before
continuing execution, etc.

## Usage

To use this library, simply call `parallel_*` methods on `Stream`s or
`TryStream`s by importing the extension traits, `StreamParExt` or
`TryStreamParExt`.

Consult the latest [API docs](https://docs.rs/tokio-par-util) for more information.
