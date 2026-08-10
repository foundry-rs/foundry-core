use alloy_primitives::{Address, hex::FromHexError};
use alloy_signer::k256::ecdsa;
use alloy_signer_ledger::LedgerError;
use alloy_signer_local::LocalSignerError;
use alloy_signer_trezor::TrezorError;

#[cfg(feature = "aws-kms")]
use alloy_signer_aws::AwsSignerError;

#[cfg(feature = "gcp-kms")]
use alloy_signer_gcp::GcpSignerError;

#[cfg(feature = "turnkey")]
use alloy_signer_turnkey::TurnkeySignerError;

#[cfg(feature = "browser")]
use crate::wallet_browser::error::BrowserWalletError;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("Store error: {0}")]
    Internal(String),
}

#[derive(Debug, thiserror::Error)]
pub enum PrivateKeyError {
    #[error("Failed to create wallet from private key. Private key is invalid hex: {0}")]
    InvalidHex(#[from] FromHexError),
    #[error(
        "Failed to create wallet from private key. Invalid private key. But env var {0} exists. Is the `$` anchor missing?"
    )]
    ExistsAsEnvVar(String),
}

#[derive(Debug, thiserror::Error)]
pub enum WalletSignerError {
    #[error(transparent)]
    Local(#[from] LocalSignerError),
    #[error("BIP-32 index {0} must be less than 2147483648")]
    InvalidBip32Index(u32),
    #[error("Failed to decrypt keystore: incorrect password")]
    IncorrectKeystorePassword,
    #[error("decrypted keystore address mismatch: expected {expected}, got {actual}")]
    KeystoreAddressMismatch { expected: Address, actual: Address },
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(
        "failed to initialize the HID API for the Ledger device; \
         this can happen in headless or containerized environments without USB/HID access"
    )]
    LedgerHidInit,
    #[error(transparent)]
    Trezor(#[from] TrezorError),
    #[error(
        "failed to initialize libusb for the Trezor device; \
         this can happen in headless or containerized environments without USB access"
    )]
    TrezorUsbInit,
    #[error(transparent)]
    #[cfg(feature = "aws-kms")]
    Aws(#[from] Box<AwsSignerError>),
    #[error(transparent)]
    #[cfg(feature = "gcp-kms")]
    Gcp(#[from] Box<GcpSignerError>),
    #[error(transparent)]
    #[cfg(feature = "turnkey")]
    Turnkey(#[from] TurnkeySignerError),
    #[error(transparent)]
    #[cfg(feature = "browser")]
    Browser(#[from] BrowserWalletError),
    #[error(transparent)]
    #[cfg(all(target_os = "macos", feature = "touch-id"))]
    TouchId(#[from] crate::touch_id::TouchIdError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    InvalidHex(#[from] FromHexError),
    #[error(transparent)]
    Ecdsa(#[from] ecdsa::Error),
    #[error("foundry was not built with support for {0} signer")]
    UnsupportedSigner(&'static str),
}

impl WalletSignerError {
    pub const fn aws_unsupported() -> Self {
        Self::UnsupportedSigner("AWS KMS")
    }

    pub const fn gcp_unsupported() -> Self {
        Self::UnsupportedSigner("Google Cloud KMS")
    }

    pub const fn turnkey_unsupported() -> Self {
        Self::UnsupportedSigner("Turnkey")
    }

    pub const fn browser_unsupported() -> Self {
        Self::UnsupportedSigner("Browser Wallet")
    }
}
