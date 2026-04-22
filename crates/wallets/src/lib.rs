//! # foundry-wallets
//!
//! Utilities for working with multiple signers.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[macro_use]
extern crate tracing;

#[cfg(feature = "tempo")]
pub mod channel_db;
pub mod error;
pub mod opts;
pub mod signer;
#[cfg(feature = "tempo")]
pub mod tempo;
pub mod utils;
#[cfg(feature = "browser")]
pub mod wallet_browser;
pub mod wallet_multi;
pub mod wallet_raw;

#[cfg(feature = "tempo")]
pub use channel_db::{Channel, ChannelDb};
pub use error::StoreError;
pub use opts::{MaybeTempoConfig, WalletOpts};
pub use signer::{PendingSigner, WalletSigner};
#[cfg(feature = "tempo")]
pub use tempo::TempoAccessKeyConfig;
#[cfg(feature = "browser")]
pub use wallet_browser::opts::BrowserWalletOpts;
pub use wallet_multi::MultiWalletOpts;
pub use wallet_raw::RawWalletOpts;

#[cfg(feature = "aws-kms")]
use aws_config as _;
