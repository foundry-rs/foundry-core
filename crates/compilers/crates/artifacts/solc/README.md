# foundry-compilers-artifacts-solc

Rust bindings for [Solc](https://docs.soliditylang.org/) JSON artifacts.

Provides types for Solc compiler input, output, settings, errors, source maps, and contract artifacts.

## Features

- `async`: Async artifact loading via `tokio`.
- `checksum`: Content hash computation for artifacts.
- `walkdir`: Recursive directory traversal for artifact discovery.
- `rayon`: Parallel artifact processing.
