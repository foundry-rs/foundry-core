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
    lookup_signer_at(from, None)
}

/// Looks up a signer for the given address and chain in Tempo's `keys.toml`.
///
/// An exact chain match takes precedence over a legacy `chain_id = 0` entry.
/// Entries for other chains are ignored.
pub fn lookup_signer_for_chain(from: Address, chain_id: u64) -> Result<TempoLookup> {
    lookup_signer_at(from, Some(chain_id))
}

fn lookup_signer_at(from: Address, chain_id: Option<u64>) -> Result<TempoLookup> {
    let path = match keys_path() {
        Some(p) if p.is_file() => p,
        _ => return Ok(TempoLookup::NotFound),
    };

    let contents = std::fs::read_to_string(&path)?;
    let file: KeysFile = toml::from_str(&contents)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    lookup_signer_in(&file, from, chain_id, now)
}

fn build_lookup(entry: &KeyEntry, fallback_chain: Option<u64>) -> Result<TempoLookup> {
    let Some(key) = &entry.key else {
        return Ok(TempoLookup::NotFound);
    };

    let signer = utils::create_local_signer(key)?;
    let key_address = entry.key_address.unwrap_or(entry.wallet_address);
    eyre::ensure!(
        signer.address() == key_address,
        "Tempo key material resolves to {}, expected {key_address}",
        signer.address()
    );

    if key_address == entry.wallet_address {
        return Ok(TempoLookup::Direct(WalletSigner::Local(signer)));
    }

    let key_authorization = if let Some(chain_id) = fallback_chain {
        if entry.key_authorization.is_some() {
            warn!(
                wallet = %entry.wallet_address,
                chain_id,
                "ignoring chain-specific authorization from legacy chain_id = 0 Tempo key entry"
            );
        }
        None
    } else {
        entry.key_authorization.as_deref().map(decode_key_authorization).transpose()?
    };

    Ok(TempoLookup::Keychain(tempo_access_key_wallet(
        entry.wallet_address,
        signer,
        key_authorization,
    )))
}

fn lookup_signer_in(
    file: &KeysFile,
    from: Address,
    chain_id: Option<u64>,
    now: u64,
) -> Result<TempoLookup> {
    let mut fallback = None;
    for entry in &file.keys {
        if entry.wallet_address != from {
            continue;
        }
        if !matches!(entry.wallet_type, WalletType::Local)
            || !matches!(entry.key_type, KeyType::Secp256k1)
            || entry.expiry.is_some_and(|expiry| expiry <= now)
        {
            continue;
        }
        if entry.key.is_none() {
            continue;
        }

        let Some(chain_id) = chain_id else {
            return build_lookup(entry, None);
        };
        if entry.chain_id == chain_id {
            return build_lookup(entry, None);
        }
        if entry.chain_id == 0 && fallback.is_none() {
            fallback = Some(entry);
        }
    }

    match (fallback, chain_id) {
        (Some(entry), Some(chain_id)) => build_lookup(entry, Some(chain_id)),
        _ => Ok(TempoLookup::NotFound),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_signer::Signer;

    const PRIVATE_KEY: &str = "0x59c6995e998f97a5a004497e5da3b5d2b2b66a87f064d39c44da0b6d6e4f8ff0";

    fn entry(
        wallet_address: Address,
        wallet_type: WalletType,
        key_type: KeyType,
        key_address: Option<Address>,
        key: Option<&str>,
        expiry: Option<u64>,
    ) -> KeyEntry {
        KeyEntry {
            wallet_type,
            wallet_address,
            chain_id: 4217,
            key_type,
            key_address,
            key: key.map(ToOwned::to_owned),
            key_authorization: None,
            expiry,
            limits: vec![],
        }
    }

    #[test]
    fn lookup_skips_unsupported_and_expired_keys() {
        let signer = utils::create_local_signer(PRIVATE_KEY).unwrap();
        let account = signer.address();
        let file = KeysFile {
            keys: vec![
                entry(
                    account,
                    WalletType::Passkey,
                    KeyType::P256,
                    None,
                    Some("not-a-secp256k1-key"),
                    None,
                ),
                entry(
                    account,
                    WalletType::Local,
                    KeyType::Secp256k1,
                    None,
                    Some("expired-invalid-key"),
                    Some(99),
                ),
                entry(
                    account,
                    WalletType::Local,
                    KeyType::Secp256k1,
                    None,
                    Some(PRIVATE_KEY),
                    None,
                ),
            ],
        };

        let TempoLookup::Direct(found) = lookup_signer_in(&file, account, None, 100).unwrap()
        else {
            panic!("expected a direct signer");
        };
        assert_eq!(found.address(), account);
    }

    #[test]
    fn lookup_validates_implicit_direct_key_address() {
        let account = Address::repeat_byte(0x11);
        let file = KeysFile {
            keys: vec![entry(
                account,
                WalletType::Local,
                KeyType::Secp256k1,
                None,
                Some(PRIVATE_KEY),
                None,
            )],
        };

        let Err(error) = lookup_signer_in(&file, account, None, 0) else {
            panic!("expected mismatched key material to fail");
        };
        assert!(error.to_string().contains("Tempo key material resolves to"));
    }

    #[test]
    fn chain_lookup_prefers_exact_match_over_legacy_fallback() {
        const SECOND_PRIVATE_KEY: &str =
            "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";
        let account = Address::repeat_byte(0x11);
        let fallback_signer = utils::create_local_signer(PRIVATE_KEY).unwrap();
        let exact_signer = utils::create_local_signer(SECOND_PRIVATE_KEY).unwrap();
        let mut fallback = entry(
            account,
            WalletType::Local,
            KeyType::Secp256k1,
            Some(fallback_signer.address()),
            Some(PRIVATE_KEY),
            None,
        );
        fallback.chain_id = 0;
        let mut exact = entry(
            account,
            WalletType::Local,
            KeyType::Secp256k1,
            Some(exact_signer.address()),
            Some(SECOND_PRIVATE_KEY),
            None,
        );
        exact.chain_id = 4217;
        let file = KeysFile { keys: vec![fallback, exact] };

        let TempoLookup::Keychain(wallet) =
            lookup_signer_in(&file, account, Some(4217), 0).unwrap()
        else {
            panic!("expected an access-key wallet");
        };
        assert_eq!(wallet.key_id(), exact_signer.address());
    }

    #[test]
    fn chain_lookup_rejects_entries_for_other_chains() {
        let signer = utils::create_local_signer(PRIVATE_KEY).unwrap();
        let account = Address::repeat_byte(0x11);
        let mut key = entry(
            account,
            WalletType::Local,
            KeyType::Secp256k1,
            Some(signer.address()),
            Some(PRIVATE_KEY),
            None,
        );
        key.chain_id = 1;
        let file = KeysFile { keys: vec![key] };

        assert!(matches!(
            lookup_signer_in(&file, account, Some(4217), 0).unwrap(),
            TempoLookup::NotFound
        ));
    }
}
