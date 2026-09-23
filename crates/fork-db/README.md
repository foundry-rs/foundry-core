# foundry-fork-db

Fork database used by [Foundry](https://github.com/foundry-rs/foundry).

> **Note:** This crate was previously maintained at [foundry-rs/foundry-fork-db](https://github.com/foundry-rs/foundry-fork-db).

Provides a shared, caching database layer backed by a remote RPC provider, allowing EVM execution against forked chain state. Implements the native [evm2](https://github.com/alloy-rs/evm2) database interface.

Account metadata and bytecode use evm2 types; this crate has no REVM dependency.
Block metadata remains generic over a serializable type, defaulting to `serde_json::Value`.
Execution writes belong in evm2's cache or state overlay above the shared RPC cache,
which no longer implements REVM's `DatabaseCommit`.

## Features

- `zstd`: Enables [zstd](https://github.com/gysber/zstd-rs) compression support.

## Remote account loading

For RPCs such as Tempo, where `eth_getBalance` returns a placeholder, use
`BlockchainDbMeta::with_account_fetch_policy(AccountFetchPolicy::RequireAccountInfo)`
to require authoritative account data without fallback. The policy is part of cache
identity, so incompatible caches are discarded even in offline-start mode.
