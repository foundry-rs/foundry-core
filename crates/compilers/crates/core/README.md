# foundry-compilers-core

Core utilities for [`foundry-compilers`](https://github.com/foundry-rs/foundry-core/tree/main/crates/compilers) crates.

Provides shared error types, path utilities, and optional helpers used across the compiler pipeline.

## Features

- `async`: Adds async file I/O via `tokio`.
- `hasher`: Enables `xxhash` based hashing.
- `regex`: Enables regex-based utilities.
- `svm-solc`: Enables [svm](https://github.com/alloy-rs/svm-rs) for managing `solc` installations.
- `walkdir`: Enables recursive directory walking.
- `project-util`: Utilities for creating and testing project workspaces.
- `test-utils`: Test helpers (tempdir creation, etc.).
