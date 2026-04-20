# foundry-compilers-artifacts

Rust bindings for compiler JSON artifacts.

This is the umbrella crate that re-exports [`foundry-compilers-artifacts-solc`](https://github.com/foundry-rs/foundry-core/tree/main/crates/compilers/crates/artifacts/solc) and [`foundry-compilers-artifacts-vyper`](https://github.com/foundry-rs/foundry-core/tree/main/crates/compilers/crates/artifacts/vyper).

## Features

- `async`: Async artifact loading.
- `checksum`: Content hash computation for artifacts.
- `walkdir`: Recursive directory traversal for artifact discovery.
- `rayon`: Parallel artifact processing.
