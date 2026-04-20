# foundry-compilers

Compiler abstraction and Foundry project implementation.

Originally part of [`ethers-rs`](https://github.com/gakonst/ethers-rs) as `ethers-solc`, this is the compilation backend for [Foundry](https://github.com/foundry-rs/foundry).

## Features

- `full`: Enables `async` + `svm-solc`.
- `async`: Adds async methods using `tokio`.
- `svm-solc`: Enables [svm](https://github.com/alloy-rs/svm-rs) to auto-detect and manage `solc` builds.
- `project-util`: Utilities for creating and testing project workspaces.
- `rustls`: Uses `rustls` for TLS (enabled by default).
