use crate::{PendingSigner, WalletSigner, error::PrivateKeyError};
use alloy_primitives::{Address, B256, hex::FromHex};
use alloy_signer_ledger::HDPath as LedgerHDPath;
use alloy_signer_local::PrivateKeySigner;
use alloy_signer_trezor::HDPath as TrezorHDPath;
use eyre::{Context, Result};
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
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

/// Extracts the address without consuming a regular keystore file.
fn extract_keystore_address(path: &Path) -> Result<Address> {
    if !path.is_file() {
        eyre::bail!("Keystore path at {path:?} is not a regular file");
    }
    let mut file = fs::File::open(path)
        .wrap_err_with(|| format!("Failed to open keystore file at {path:?}"))?;
    // File descriptor paths can share their cursor, so restore it before later decryption.
    let position = file
        .stream_position()
        .wrap_err_with(|| format!("Keystore file at {path:?} is not seekable"))?;
    let mut content = String::new();
    let read_result = file.read_to_string(&mut content);
    file.seek(SeekFrom::Start(position))
        .wrap_err_with(|| format!("Failed to restore keystore file position at {path:?}"))?;
    read_result.wrap_err_with(|| format!("Failed to read keystore file at {path:?}"))?;
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
        let address = extract_keystore_address(path).ok();
        Ok((None, Some(PendingSigner::Keystore(path.clone(), address))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::address;
    #[cfg(unix)]
    use std::{
        os::fd::AsRawFd,
        process::{Command, Stdio},
    };

    const KEYSTORE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/test-data/keystore/UTC--2022-12-20T10-30-43.591916000Z--ec554aeafe75601aaab43bd4621a22284db566c2"
    );
    const KEYSTORE_PASSWORD: &str = "keystorepassword";
    const KEYSTORE_ADDRESS: Address = address!("0xeC554aeAFE75601AaAb43Bd4621A22284dB566C2");

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
        assert_eq!(extract_keystore_address(Path::new(KEYSTORE)).unwrap(), KEYSTORE_ADDRESS);
    }

    #[test]
    fn pending_keystore_signer_exposes_address_of_a_regular_file() {
        let path = PathBuf::from(KEYSTORE);

        let (signer, pending) = create_keystore_signer(&path, None, None).unwrap();

        assert!(signer.is_none());
        assert!(
            matches!(pending, Some(PendingSigner::Keystore(_, Some(address))) if address == KEYSTORE_ADDRESS)
        );
    }

    #[cfg(unix)]
    #[test]
    fn pending_keystore_signer_restores_a_regular_file_descriptor() {
        let file = fs::File::open(KEYSTORE).unwrap();
        let path = PathBuf::from(format!("/dev/fd/{}", file.as_raw_fd()));

        let (signer, pending) = create_keystore_signer(&path, None, None).unwrap();
        assert!(signer.is_none());
        assert!(matches!(
            pending,
            Some(PendingSigner::Keystore(_, Some(address))) if address == KEYSTORE_ADDRESS
        ));

        let unlocked = PrivateKeySigner::decrypt_keystore(&path, KEYSTORE_PASSWORD).unwrap();
        assert_eq!(unlocked.address(), KEYSTORE_ADDRESS);
    }

    #[cfg(unix)]
    #[test]
    fn pending_keystore_signer_leaves_a_streamed_keystore_readable() {
        let mut cat = Command::new("cat").arg(KEYSTORE).stdout(Stdio::piped()).spawn().unwrap();
        let stdout = cat.stdout.take().unwrap();
        let path = PathBuf::from(format!("/dev/fd/{}", stdout.as_raw_fd()));
        assert!(cat.wait().unwrap().success());

        let (signer, pending) = create_keystore_signer(&path, None, None).unwrap();
        assert!(signer.is_none());
        assert!(matches!(pending, Some(PendingSigner::Keystore(_, None))));

        let unlocked = PrivateKeySigner::decrypt_keystore(&path, KEYSTORE_PASSWORD).unwrap();
        assert_eq!(unlocked.address(), KEYSTORE_ADDRESS);
    }
}
