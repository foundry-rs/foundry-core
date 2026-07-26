//! # foundry-wallets
//!
//! Utilities for working with multiple signers.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[macro_use]
extern crate tracing;

// The RLP traits are consumed by the browser module only when both features
// are enabled; retain the dependency for Tempo-only builds as well.
#[cfg(feature = "tempo")]
use alloy_rlp as _;

pub mod error;
pub mod opts;
pub mod signer;
pub mod utils;
#[cfg(feature = "browser")]
pub mod wallet_browser;
pub mod wallet_multi;
pub mod wallet_raw;

pub use error::StoreError;
pub use opts::{MaybeTempoWallet, WalletOpts};
pub use signer::{PendingSigner, WalletSigner};
#[cfg(feature = "tempo")]
pub use tempo_alloy::accounts::TempoAccountsWallet;
#[cfg(feature = "browser")]
pub use wallet_browser::opts::BrowserWalletOpts;
pub use wallet_multi::MultiWalletOpts;
pub use wallet_raw::RawWalletOpts;

#[cfg(feature = "aws-kms")]
use aws_config as _;
#[cfg(feature = "aws-kms")]
use aws_smithy_time_compat as _;
