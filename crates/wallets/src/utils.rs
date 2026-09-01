use crate::{PendingSigner, WalletSigner, error::PrivateKeyError};
use alloy_primitives::{Address, B256, hex::FromHex};
use alloy_signer_ledger::HDPath as LedgerHDPath;
use alloy_signer_local::PrivateKeySigner;
use alloy_signer_trezor::HDPath as TrezorHDPath;
use eyre::{Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn ensure_pk_not_env(pk: &str) -> Result<()> {
    if !pk.starts_with("0x") && std::env::var(pk).is_ok() {
        return Err(PrivateKeyError::ExistsAsEnvVar(pk.to_string()).into());
    }
    Ok(())
}

/// Validates and sanitizes user inputs, returning configured [WalletSigner].
pub fn create_private_key_signer(private_key_str: &str) -> Result<WalletSigner> {
    create_local_signer(private_key_str).map(WalletSigner::Local)
}

/// Validates and sanitizes user inputs, returning a local private-key signer.
pub fn create_local_signer(private_key_str: &str) -> Result<PrivateKeySigner> {
    let Ok(private_key) = B256::from_hex(private_key_str) else {
        ensure_pk_not_env(private_key_str)?;
        eyre::bail!("Failed to decode private key");
    };
    match PrivateKeySigner::from_bytes(&private_key) {
        Ok(pk) => Ok(pk),
        Err(err) => {
            ensure_pk_not_env(private_key_str)?;
            eyre::bail!("Failed to create wallet from private key: {err}");
        }
    }
}

/// Creates [WalletSigner] instance from given mnemonic parameters.
///
/// Mnemonic can be either a file path or a mnemonic phrase.
pub fn create_mnemonic_signer(
    mnemonic: &str,
    passphrase: Option<&str>,
    hd_path: Option<&str>,
    index: u32,
) -> Result<WalletSigner> {
    let mnemonic = if Path::new(mnemonic).is_file() {
        fs::read_to_string(mnemonic)?
    } else {
        mnemonic.to_owned()
    };
    let mnemonic = mnemonic.split_whitespace().collect::<Vec<_>>().join(" ");

    Ok(WalletSigner::from_mnemonic(&mnemonic, passphrase, hd_path, index)?)
}

/// Creates [WalletSigner] instance from given Ledger parameters.
pub async fn create_ledger_signer(
    hd_path: Option<&str>,
    mnemonic_index: u32,
) -> Result<WalletSigner> {
    let derivation = if let Some(hd_path) = hd_path {
        LedgerHDPath::Other(hd_path.to_owned())
    } else {
        LedgerHDPath::LedgerLive(mnemonic_index as usize)
    };

    WalletSigner::from_ledger_path(derivation).await.wrap_err_with(|| {
        "\
Could not connect to Ledger device.
Make sure it's connected and unlocked, with no other desktop wallet apps open."
    })
}

/// Creates [WalletSigner] instance from given Trezor parameters.
pub async fn create_trezor_signer(
    hd_path: Option<&str>,
    mnemonic_index: u32,
) -> Result<WalletSigner> {
    let derivation = if let Some(hd_path) = hd_path {
        TrezorHDPath::Other(hd_path.to_owned())
    } else {
        TrezorHDPath::TrezorLive(mnemonic_index as usize)
    };

    WalletSigner::from_trezor_path(derivation).await.wrap_err_with(|| {
        "\
Could not connect to Trezor device.
Make sure it's connected and unlocked, with no other conflicting desktop wallet apps open."
    })
}

pub fn maybe_get_keystore_path(
    maybe_path: Option<&str>,
    maybe_name: Option<&str>,
) -> Result<Option<PathBuf>> {
    // TODO: temporary replacement for `Config::foundry_keystores_dir` to not depend on
    // `foundry-config` crate
    let default_keystore_dir = dirs::home_dir()
        .map(|p| p.join(".foundry").join("keystores"))
        .ok_or_else(|| eyre::eyre!("Could not find the default keystore directory."))?;
    Ok(maybe_path
        .map(PathBuf::from)
        .or_else(|| maybe_name.map(|name| default_keystore_dir.join(name))))
}

/// Whether `path` can be read more than once.
///
/// Keystores are commonly streamed in rather than named, e.g. `--keystore /dev/stdin` with the
/// JSON piped on stdin, or `--keystore <(...)`. Those paths resolve to a stream that the first
/// reader drains, so only a regular file survives being read twice.
fn is_rereadable(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
}

/// Extracts the address from a keystore JSON file without decrypting it.
fn extract_keystore_address(path: &Path) -> Result<Address> {
    let content = fs::read_to_string(path)
        .wrap_err_with(|| format!("Failed to read keystore file at {path:?}"))?;
    let json: serde_json::Value = serde_json::from_str(&content)
        .wrap_err_with(|| format!("Failed to parse keystore JSON at {path:?}"))?;
    let address = json
        .get("address")
        .and_then(|value| value.as_str())
        .ok_or_else(|| eyre::eyre!("Keystore JSON does not contain an `address` field"))?;

    address
        .parse()
        .wrap_err_with(|| format!("Failed to parse address `{address}` from keystore JSON"))
}

/// Creates keystore signer from given parameters.
///
/// If correct password or password file is provided, the keystore is decrypted and a [WalletSigner]
/// is returned.
///
/// Otherwise, a [PendingSigner] is returned, which can be used to unlock the keystore later,
/// prompting user for password.
pub fn create_keystore_signer(
    path: &PathBuf,
    maybe_password: Option<&str>,
    maybe_password_file: Option<&str>,
) -> Result<(Option<WalletSigner>, Option<PendingSigner>)> {
    if !path.exists() {
        eyre::bail!("Keystore file `{path:?}` does not exist");
    }

    if path.is_dir() {
        eyre::bail!(
            "Keystore path `{path:?}` is a directory. Please specify the keystore file directly."
        );
    }

    let password = match (maybe_password, maybe_password_file) {
        (Some(password), _) => Ok(Some(password.to_string())),
        (_, Some(password_file)) => {
            let password_file = Path::new(password_file);
            if password_file.is_file() {
                Ok(Some(
                    fs::read_to_string(password_file)
                        .wrap_err_with(|| {
                            format!("Failed to read keystore password file at {password_file:?}")
                        })?
                        .trim_end()
                        .to_string(),
                ))
            } else {
                Err(eyre::eyre!("Keystore password file `{password_file:?}` does not exist"))
            }
        }
        (None, None) => Ok(None),
    }?;

    if let Some(password) = password {
        let wallet = PrivateKeySigner::decrypt_keystore(path, password)
            .wrap_err_with(|| format!("Failed to decrypt keystore {path:?}"))?;
        Ok((Some(WalletSigner::Local(wallet)), None))
    } else {
        // `PendingSigner::unlock` reopens the keystore to decrypt it, so reading it here as well
        // is only safe when the path can be read twice.
        let address = is_rereadable(path).then(|| extract_keystore_address(path).ok()).flatten();
        Ok((None, Some(PendingSigner::Keystore(path.clone(), address))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::address;

    /// Test vector from the Web3 Secret Storage Definition, with the `address` field that
    /// `extract_keystore_address` looks for. Unlocks to `KEYSTORE_ADDRESS` with
    /// `KEYSTORE_PASSWORD`.
    const KEYSTORE: &str = r#"{
      "address": "008aeeda4d805471df9b2a5b0f38a0c3bcba786b",
      "crypto": {
        "cipher": "aes-128-ctr",
        "cipherparams": { "iv": "6087dab2f9fdbbfaddc31a909735c1e6" },
        "ciphertext": "5318b4d5bcd28de64ee5559e671353e16f075ecae9f99c7a79a38af5f869aa46",
        "kdf": "pbkdf2",
        "kdfparams": {
          "c": 262144,
          "dklen": 32,
          "prf": "hmac-sha256",
          "salt": "ae3cd4e7013836a3df6bd7241b12db061dbe2c6785853cce422d148a624ce0bd"
        },
        "mac": "517ead924a9d0dc3124507e3393d175ce3ff7c1e96529c6c555ce9e51205e9b2"
      },
      "id": "3198bc9c-6672-5ab3-d995-4942343ae5b6",
      "version": 3
    }"#;
    const KEYSTORE_PASSWORD: &str = "testpassword";
    const KEYSTORE_ADDRESS: Address = address!("0x008AeEda4D805471dF9b2A5B0f38A0C3bCBA786b");

    fn keystore_json(address: Address) -> String {
        format!(r#"{{"address":"{}","version":3}}"#, alloy_primitives::hex::encode(address))
    }

    #[test]
    fn parse_private_key_signer() {
        let pk = B256::random();
        let pk_str = pk.to_string();
        assert!(create_private_key_signer(&pk_str).is_ok());
        assert!(create_local_signer(&pk_str).is_ok());
        // skip 0x
        assert!(create_private_key_signer(&pk_str[2..]).is_ok());
        assert!(create_local_signer(&pk_str[2..]).is_ok());
    }

    #[test]
    fn extracts_keystore_address() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keystore.json");
        let address = Address::random();
        fs::write(&path, keystore_json(address)).unwrap();

        assert_eq!(extract_keystore_address(&path).unwrap(), address);
    }

    #[test]
    fn pending_keystore_signer_exposes_address_of_a_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keystore.json");
        let address = Address::random();
        fs::write(&path, keystore_json(address)).unwrap();

        let (signer, pending) = create_keystore_signer(&path, None, None).unwrap();

        assert!(signer.is_none());
        assert!(
            matches!(pending, Some(PendingSigner::Keystore(_, Some(found))) if found == address)
        );
    }

    /// `cat keystore.json | cast wallet address --keystore /dev/stdin`: the keystore arrives on a
    /// pipe that only survives a single read, and that read belongs to `PendingSigner::unlock`.
    #[cfg(unix)]
    #[test]
    fn pending_keystore_signer_leaves_a_streamed_keystore_readable() {
        use std::{os::fd::AsRawFd, process::Stdio};

        let dir = tempfile::tempdir().unwrap();
        let keystore = dir.path().join("keystore.json");
        fs::write(&keystore, KEYSTORE).unwrap();

        let mut cat = std::process::Command::new("cat")
            .arg(&keystore)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = cat.stdout.take().unwrap();
        // What `/dev/stdin` resolves to for the process reading the pipe. The payload fits in the
        // pipe buffer, so `cat` is already gone and only the buffered copy is left to read.
        let path = PathBuf::from(format!("/dev/fd/{}", stdout.as_raw_fd()));
        assert!(cat.wait().unwrap().success());

        let (signer, pending) = create_keystore_signer(&path, None, None).unwrap();
        assert!(signer.is_none());
        assert!(matches!(pending, Some(PendingSigner::Keystore(..))));

        // The read `PendingSigner::unlock` performs once the password is entered.
        let unlocked = PrivateKeySigner::decrypt_keystore(&path, KEYSTORE_PASSWORD).unwrap();
        assert_eq!(unlocked.address(), KEYSTORE_ADDRESS);
    }
}
