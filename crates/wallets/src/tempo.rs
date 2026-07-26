use alloy_primitives::{Address, hex};
use alloy_rlp::Decodable;
use alloy_signer_local::PrivateKeySigner;
use eyre::Result;
use std::path::PathBuf;
use tempo_alloy::accounts::TempoKeychainWallet;
use tempo_primitives::transaction::SignedKeyAuthorization;

use crate::{WalletSigner, utils};

/// Wallet type: how this wallet was created.
#[derive(Clone, Copy, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum WalletType {
    #[default]
    Local,
    Passkey,
}

/// Cryptographic key type.
#[derive(Clone, Copy, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum KeyType {
    #[default]
    Secp256k1,
    P256,
    WebAuthn,
}

/// A single entry from Tempo's `keys.toml`.
#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct KeyEntry {
    #[serde(default)]
    wallet_type: WalletType,
    #[serde(default)]
    wallet_address: Address,
    #[serde(default)]
    chain_id: u64,
    #[serde(default)]
    key_type: KeyType,
    #[serde(default)]
    key_address: Option<Address>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    key_authorization: Option<String>,
    #[serde(default)]
    expiry: Option<u64>,
    #[serde(default)]
    limits: Vec<StoredTokenLimit>,
}

/// Per-token spending limit stored in `keys.toml`.
#[derive(serde::Deserialize)]
struct StoredTokenLimit {
    #[allow(dead_code)]
    currency: Address,
    #[allow(dead_code)]
    limit: String,
}

/// The top-level structure of `~/.tempo/wallet/keys.toml`.
#[derive(serde::Deserialize)]
struct KeysFile {
    #[serde(default)]
    keys: Vec<KeyEntry>,
}

/// A Foundry signer carrying its complete Tempo access-key context.
pub type TempoAccessKeyWallet = TempoKeychainWallet<PrivateKeySigner>;

/// Build a Tempo access-key wallet from a local signer.
pub fn tempo_access_key_wallet(
    account: Address,
    signer: PrivateKeySigner,
    key_authorization: Option<SignedKeyAuthorization>,
) -> TempoAccessKeyWallet {
    let wallet = TempoKeychainWallet::new(account, signer);
    match key_authorization {
        Some(key_authorization) => wallet.with_key_authorization(key_authorization),
        None => wallet,
    }
}

/// Result of looking up an address in Tempo's key store.
pub enum TempoLookup {
    /// A direct (EOA) signer was found — `wallet_address == key_address`.
    Direct(WalletSigner),
    /// A keychain (access key) signer was found — `wallet_address != key_address`.
    Keychain(TempoAccessKeyWallet),
    /// No matching entry was found.
    NotFound,
}

/// Returns the path to Tempo's keys file.
///
/// Respects `TEMPO_HOME` env var, defaulting to `~/.tempo`.
fn keys_path() -> Option<PathBuf> {
    let base = std::env::var_os("TEMPO_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".tempo")))?;
    Some(base.join("wallet").join("keys.toml"))
}

/// Decodes a hex-encoded, RLP-encoded [`SignedKeyAuthorization`].
fn decode_key_authorization(hex_str: &str) -> Result<SignedKeyAuthorization> {
    let bytes = hex::decode(hex_str)?;
    let auth = SignedKeyAuthorization::decode(&mut bytes.as_slice())?;
    Ok(auth)
}

/// Looks up a signer for the given address in Tempo's `keys.toml`.
///
/// Returns [`TempoLookup::Direct`] if an EOA key is found,
/// [`TempoLookup::Keychain`] if an account access key is found,
/// or [`TempoLookup::NotFound`] if no entry matches.
pub fn lookup_signer(from: Address) -> Result<TempoLookup> {
    let path = match keys_path() {
        Some(p) if p.is_file() => p,
        _ => return Ok(TempoLookup::NotFound),
    };

    let contents = std::fs::read_to_string(&path)?;
    let file: KeysFile = toml::from_str(&contents)?;

    for entry in &file.keys {
        if entry.wallet_address != from {
            continue;
        }

        let Some(key) = &entry.key else {
            continue;
        };

        // An EOA signs for itself when wallet_address == key_address (or key_address is absent).
        let is_direct =
            entry.key_address.is_none() || entry.key_address == Some(entry.wallet_address);

        let signer = utils::create_local_signer(key)?;
        if let Some(key_address) = entry.key_address {
            eyre::ensure!(
                signer.address() == key_address,
                "Tempo key material resolves to {}, expected {key_address}",
                signer.address()
            );
        }

        if is_direct {
            return Ok(TempoLookup::Direct(WalletSigner::Local(signer)));
        }

        // Otherwise the key signs on behalf of wallet_address.
        let key_authorization =
            entry.key_authorization.as_deref().map(decode_key_authorization).transpose()?;

        return Ok(TempoLookup::Keychain(tempo_access_key_wallet(
            entry.wallet_address,
            signer,
            key_authorization,
        )));
    }

    Ok(TempoLookup::NotFound)
}
