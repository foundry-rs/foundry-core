//! # foundry-wallets
//!
//! Utilities for working with multiple signers.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[macro_use]
extern crate tracing;

pub mod error;
pub mod opts;
pub mod signer;
#[cfg(all(target_os = "macos", feature = "touch-id"))]
pub mod touch_id;
pub mod utils;
#[cfg(feature = "browser")]
pub mod wallet_browser;
pub mod wallet_multi;
pub mod wallet_raw;

pub use error::StoreError;
#[cfg(feature = "tempo")]
pub use opts::TempoWalletSpec;
pub use opts::{MaybeTempoWallet, WalletOpts};
pub use signer::{PendingSigner, WalletSigner};
#[cfg(feature = "browser")]
pub use wallet_browser::opts::BrowserWalletOpts;
pub use wallet_multi::MultiWalletOpts;
pub use wallet_raw::RawWalletOpts;

#[cfg(feature = "aws-kms")]
use aws_config as _;
