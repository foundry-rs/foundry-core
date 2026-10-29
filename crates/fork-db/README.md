# foundry-fork-db

Fork database used by [Foundry](https://github.com/foundry-rs/foundry).

> **Note:** This crate was previously maintained at [foundry-rs/foundry-fork-db](https://github.com/foundry-rs/foundry-fork-db).

Provides a shared, caching database layer backed by a remote RPC provider, allowing EVM execution against forked chain state. Implements the native [evm2](https://github.com/alloy-rs/evm2) database interface.

Account metadata and bytecode use evm2 types; this crate has no REVM dependency.
Block metadata remains generic over a serializable type, defaulting to `serde_json::Value`.
Execution writes belong in evm2's cache or state overlay above the shared RPC cache,
which no longer implements REVM's `DatabaseCommit`.

`SharedBackend` implements `evm2::evm::Database`. Shared reads are also available as
`account(address)`, `storage_at(address, slot)`, and `block_hash(number)`.
Account/code, storage, and block-hash enumeration and bulk insertion remain available
for corpus discovery and prefetching. RPC account reads include bytecode; lookup by
code hash alone returns `DatabaseError::MissingCode`. Empty RPC accounts remain
`Some(AccountInfo)` rather than claiming proven absence.

## Features

- `zstd`: Enables [zstd](https://github.com/gysber/zstd-rs) compression support.

## Remote account loading

For RPCs such as Tempo, where `eth_getBalance` returns a placeholder, use
`BlockchainDbMeta::with_account_fetch_policy(AccountFetchPolicy::RequireAccountInfo)`
to require authoritative account data without fallback. The policy is part of cache
identity, so incompatible caches are discarded even in offline-start mode.

## Persisted caches

Existing JSON caches (and zstd caches with the feature enabled) remain readable,
including REVM `LegacyAnalyzed` and `Eip7702` bytecode. evm2's serde implementation
recovers the original code bytes and rebuilds jump analysis; new writes store code
as hex bytes. Block metadata, fork/source identity, and account-fetch policy keep
their existing format and validation. Cache migration does not require REVM types
or a separate cache schema.
