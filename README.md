# foundry-core

Core libraries extracted from [Foundry](https://github.com/foundry-rs/foundry), published as standalone crates.

## Crates

- [`foundry-compilers`] - Compiler abstraction and Foundry project implementation
  - [`foundry-compilers-artifacts`] - Rust bindings for compiler JSON artifacts
  - [`foundry-compilers-artifacts-solc`] - Rust bindings for Solc JSON artifacts
  - [`foundry-compilers-artifacts-vyper`] - Rust bindings for Vyper JSON artifacts
  - [`foundry-compilers-core`] - Core utilities for foundry-compilers crates
- [`foundry-block-explorers`] - Bindings for Etherscan and other block explorer APIs
- [`foundry-blob-explorers`] - Bindings for Blobscan and other blob explorer APIs
- [`foundry-fork-db`] - Fork database used by Foundry
- [`foundry-wallets`] - Wallet management and signing support

[`foundry-compilers`]: https://github.com/foundry-rs/foundry-core/tree/main/crates/compilers
[`foundry-compilers-artifacts`]: https://github.com/foundry-rs/foundry-core/tree/main/crates/compilers/crates/artifacts/artifacts
[`foundry-compilers-artifacts-solc`]: https://github.com/foundry-rs/foundry-core/tree/main/crates/compilers/crates/artifacts/solc
[`foundry-compilers-artifacts-vyper`]: https://github.com/foundry-rs/foundry-core/tree/main/crates/compilers/crates/artifacts/vyper
[`foundry-compilers-core`]: https://github.com/foundry-rs/foundry-core/tree/main/crates/compilers/crates/core
[`foundry-block-explorers`]: https://github.com/foundry-rs/foundry-core/tree/main/crates/explorers/block-explorers
[`foundry-blob-explorers`]: https://github.com/foundry-rs/foundry-core/tree/main/crates/explorers/blob-explorers
[`foundry-fork-db`]: https://github.com/foundry-rs/foundry-core/tree/main/crates/fork-db
[`foundry-wallets`]: https://github.com/foundry-rs/foundry-core/tree/main/crates/wallets

## Supported Rust Versions (MSRV)

The current MSRV (minimum supported rust version) is **1.93**.

Note that the MSRV is not increased automatically, and only as part of a minor release.

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md) for guidelines.

Pull requests will not be merged unless CI passes, so please ensure that your
contribution follows the linting rules and passes clippy.

#### License

<sup>
Licensed under either of <a href="LICENSE-APACHE">Apache License, Version
2.0</a> or <a href="LICENSE-MIT">MIT license</a> at your option.
</sup>

<br>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in these crates by you, as defined in the Apache-2.0 license,
shall be dual licensed as above, without any additional terms or conditions.
</sub>
