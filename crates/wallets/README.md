# foundry-wallets

Wallet management and signing support for [Foundry](https://github.com/foundry-rs/foundry).

Supports multiple signer backends:

- [Private key](https://docs.rs/alloy-signer-local) (plaintext, keystore, mnemonic)
- [Ledger](https://docs.rs/alloy-signer-ledger) hardware wallet
- [Trezor](https://docs.rs/alloy-signer-trezor) hardware wallet
- Browser wallet (local HTTP callback server)
- Tempo access key signing

## Features

- `browser`: Browser wallet support via a local HTTP callback server.
- `tempo`: Tempo access key signing support.
- `aws-kms`: [AWS KMS](https://aws.amazon.com/kms/) signer support via [`alloy-signer-aws`](https://docs.rs/alloy-signer-aws).
- `gcp-kms`: [GCP KMS](https://cloud.google.com/kms) signer support via [`alloy-signer-gcp`](https://docs.rs/alloy-signer-gcp).
- `turnkey`: [Turnkey](https://www.turnkey.com) signer support via [`alloy-signer-turnkey`](https://docs.rs/alloy-signer-turnkey).
- `touch-id`: Touch ID (Secure Enclave) keystore unlock on macOS. Compiles a Swift shim at build time; no effect on other targets. Building requires Xcode or the Command Line Tools 15.0.1 or newer (Swift 5.9+) and a macOS 11 or newer deployment target; Intel builds must set `MACOSX_DEPLOYMENT_TARGET=11.0` or newer. At runtime the feature needs macOS 11 or newer on a Mac with a Secure Enclave.
