# foundry-fork-db

Fork database used by [Foundry](https://github.com/foundry-rs/foundry).

> **Note:** This crate was previously maintained at [foundry-rs/foundry-fork-db](https://github.com/foundry-rs/foundry-fork-db).

Provides a shared, caching database layer backed by a remote RPC provider, allowing EVM execution against forked chain state.

The crate depends on no EVM implementation. It fetches and caches accounts, storage, and block
hashes with alloy types: `SharedBackend::account(address)`, `storage_at(address, slot)`, and
`block_hash(number)`. Accounts are `AccountInfo { balance, nonce, code_hash, code }`, with code as
raw bytes. Execution engines implement their own database traits over these reads, convert raw
code into their bytecode type, and keep execution writes in their own state overlay. RPCs cannot
tell absent accounts from empty ones, so both are reported as `Some(AccountInfo)`; the engine
decides how an empty account loads.

Block metadata is generic over a serializable type, defaulting to `serde_json::Value`.

## Features

- `zstd`: Enables [zstd](https://github.com/gysber/zstd-rs) compression support.

## Remote account loading

For RPCs such as Tempo, where `eth_getBalance` returns a placeholder, use
`BlockchainDbMeta::with_account_fetch_policy(AccountFetchPolicy::RequireAccountInfo)`
to require authoritative account data without fallback. The policy is part of cache
identity, so incompatible caches are discarded even in offline-start mode.

## Persisted caches

Existing JSON caches (and zstd caches with the feature enabled) remain readable,
including REVM `LegacyAnalyzed` and `Eip7702` bytecode, which load as their original code
bytes. New writes store code as hex bytes. Block metadata, fork/source identity, and
account-fetch policy keep their existing format and validation.
