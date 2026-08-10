use crate::error::WalletSignerError;
use alloy_consensus::SignableTransaction;
use alloy_dyn_abi::TypedData;
use alloy_network::TxSigner;
use alloy_primitives::{Address, B256, ChainId, Signature, hex};
use alloy_signer::Signer;
use alloy_signer_ledger::{HDPath as LedgerHDPath, LedgerSigner};
use alloy_signer_local::{MnemonicBuilder, PrivateKeySigner, coins_bip39::English};
use alloy_signer_trezor::{HDPath as TrezorHDPath, TrezorSigner};
use alloy_sol_types::{Eip712Domain, SolStruct};
use async_trait::async_trait;
use std::{collections::HashSet, path::PathBuf};
use tracing::warn;

#[cfg(feature = "aws-kms")]
use alloy_signer_aws::{AwsSigner, aws_config::BehaviorVersion, aws_sdk_kms::Client as AwsClient};

#[cfg(feature = "gcp-kms")]
use alloy_signer_gcp::{
    GcpKeyRingRef, GcpSigner, GcpSignerError, KeySpecifier,
    gcloud_sdk::{
        GoogleApi,
        google::cloud::kms::v1::key_management_service_client::KeyManagementServiceClient,
    },
};

#[cfg(feature = "turnkey")]
use alloy_signer_turnkey::TurnkeySigner;

pub type Result<T> = std::result::Result<T, WalletSignerError>;

const BIP32_HARDEN: u32 = 0x8000_0000;

/// Wrapper enum around different signers.
#[derive(Debug)]
pub enum WalletSigner {
    /// Wrapper around local wallet. e.g. private key, mnemonic
    Local(PrivateKeySigner),
    /// Wrapper around Ledger signer.
    Ledger(LedgerSigner),
    /// Wrapper around Trezor signer.
    Trezor(TrezorSigner),
    /// Wrapper around AWS KMS signer.
    #[cfg(feature = "aws-kms")]
    Aws(AwsSigner),
    /// Wrapper around Google Cloud KMS signer.
    #[cfg(feature = "gcp-kms")]
    Gcp(GcpSigner),
    /// Wrapper around Turnkey signer.
    #[cfg(feature = "turnkey")]
    Turnkey(TurnkeySigner),
}

/// Probes HID availability without triggering `coins-ledger`'s panicking
/// `HidApi::new().expect(..)`, which would abort the process under `panic = "abort"`
/// (foundry-rs/foundry#14491). Cached because `hidapi-rusb` allows only one live `HidApi`; a later
/// probe would see the lock held and fail.
#[cfg(not(target_family = "wasm"))]
fn hid_api_available() -> bool {
    static HID_API_AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *HID_API_AVAILABLE.get_or_init(|| hidapi_rusb::HidApi::new().is_ok())
}

/// Probes libusb availability without triggering `rusb::GlobalContext`, which `panic!`s (inside a
/// `Once`) when `libusb_init` fails and would abort the process under `panic = "abort"`. `trezor-
/// client` uses that global context, so we probe a throwaway `rusb::Context` (which returns a
/// `Result`) first. Cached so we probe at most once per process.
#[cfg(not(target_family = "wasm"))]
fn usb_available() -> bool {
    static USB_AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *USB_AVAILABLE.get_or_init(|| rusb::Context::new().is_ok())
}

impl WalletSigner {
    pub async fn from_ledger_path(path: LedgerHDPath) -> Result<Self> {
        #[cfg(not(target_family = "wasm"))]
        if !hid_api_available() {
            return Err(WalletSignerError::LedgerHidInit);
        }

        let ledger = LedgerSigner::new(path, None).await?;
        Ok(Self::Ledger(ledger))
    }

    pub async fn from_trezor_path(path: TrezorHDPath) -> Result<Self> {
        #[cfg(not(target_family = "wasm"))]
        if !usb_available() {
            return Err(WalletSignerError::TrezorUsbInit);
        }

        let trezor = TrezorSigner::new(path, None).await?;
        Ok(Self::Trezor(trezor))
    }

    pub async fn from_aws(key_id: String) -> Result<Self> {
        #[cfg(feature = "aws-kms")]
        {
            let config =
                alloy_signer_aws::aws_config::load_defaults(BehaviorVersion::latest()).await;
            let client = AwsClient::new(&config);

            Ok(Self::Aws(
                AwsSigner::new(client, key_id, None)
                    .await
                    .map_err(|e| WalletSignerError::Aws(Box::new(e)))?,
            ))
        }

        #[cfg(not(feature = "aws-kms"))]
        {
            let _ = key_id;
            Err(WalletSignerError::aws_unsupported())
        }
    }

    pub async fn from_gcp(
        project_id: String,
        location: String,
        keyring: String,
        key_name: String,
        key_version: u64,
    ) -> Result<Self> {
        #[cfg(feature = "gcp-kms")]
        {
            let keyring = GcpKeyRingRef::new(&project_id, &location, &keyring);
            let client = match GoogleApi::from_function(
                KeyManagementServiceClient::new,
                "https://cloudkms.googleapis.com",
                None,
            )
            .await
            {
                Ok(c) => c,
                Err(e) => {
                    return Err(WalletSignerError::Gcp(Box::new(GcpSignerError::GoogleKmsError(
                        e,
                    ))));
                }
            };

            let specifier = KeySpecifier::new(keyring, &key_name, key_version);

            Ok(Self::Gcp(
                GcpSigner::new(client, specifier, None)
                    .await
                    .map_err(|e| WalletSignerError::Gcp(Box::new(e)))?,
            ))
        }

        #[cfg(not(feature = "gcp-kms"))]
        {
            let _ = project_id;
            let _ = location;
            let _ = keyring;
            let _ = key_name;
            let _ = key_version;
            Err(WalletSignerError::gcp_unsupported())
        }
    }

    pub fn from_turnkey(
        api_private_key: String,
        organization_id: String,
        address: Address,
    ) -> Result<Self> {
        #[cfg(feature = "turnkey")]
        {
            Ok(Self::Turnkey(TurnkeySigner::from_api_key(
                &api_private_key,
                organization_id,
                address,
                None,
            )?))
        }

        #[cfg(not(feature = "turnkey"))]
        {
            let _ = api_private_key;
            let _ = organization_id;
            let _ = address;
            Err(WalletSignerError::turnkey_unsupported())
        }
    }

    pub fn from_private_key(private_key: &B256) -> Result<Self> {
        Ok(Self::Local(PrivateKeySigner::from_bytes(private_key)?))
    }

    /// Returns a list of addresses available to use with current signer
    ///
    /// - for Ledger and Trezor signers the number of addresses to retrieve is specified as argument
    /// - the result for Ledger signers includes addresses available for both LedgerLive and Legacy
    ///   derivation paths
    /// - for Local and AWS signers the result contains a single address
    /// - errors when retrieving addresses are logged but do not prevent returning available
    ///   addresses
    pub async fn available_senders(&self, max: usize) -> Result<Vec<Address>> {
        let mut senders = HashSet::new();

        match self {
            Self::Local(local) => {
                senders.insert(local.address());
            }
            Self::Ledger(ledger) => {
                // Try LedgerLive derivation path
                for i in 0..max {
                    match ledger.get_address_with_path(&LedgerHDPath::LedgerLive(i)).await {
                        Ok(address) => {
                            senders.insert(address);
                        }
                        Err(e) => {
                            warn!("Failed to get Ledger address at index {i} (LedgerLive): {e}");
                        }
                    }
                }
                // Try Legacy derivation path
                for i in 0..max {
                    match ledger.get_address_with_path(&LedgerHDPath::Legacy(i)).await {
                        Ok(address) => {
                            senders.insert(address);
                        }
                        Err(e) => {
                            warn!("Failed to get Ledger address at index {i} (Legacy): {e}");
                        }
                    }
                }
            }
            Self::Trezor(trezor) => {
                for i in 0..max {
                    match trezor.get_address_with_path(&TrezorHDPath::TrezorLive(i)).await {
                        Ok(address) => {
                            senders.insert(address);
                        }
                        Err(e) => {
                            warn!("Failed to get Trezor address at index {i} (TrezorLive): {e}",);
                        }
                    }
                }
            }
            #[cfg(feature = "aws-kms")]
            Self::Aws(aws) => {
                senders.insert(alloy_signer::Signer::address(aws));
            }
            #[cfg(feature = "gcp-kms")]
            Self::Gcp(gcp) => {
                senders.insert(alloy_signer::Signer::address(gcp));
            }
            #[cfg(feature = "turnkey")]
            Self::Turnkey(turnkey) => {
                senders.insert(alloy_signer::Signer::address(turnkey));
            }
        }
        Ok(senders.into_iter().collect())
    }

    pub fn from_mnemonic(
        mnemonic: &str,
        passphrase: Option<&str>,
        derivation_path: Option<&str>,
        index: u32,
    ) -> Result<Self> {
        let mut builder = MnemonicBuilder::<English>::default().phrase(mnemonic);

        if let Some(passphrase) = passphrase {
            builder = builder.password(passphrase)
        }

        builder = if let Some(hd_path) = derivation_path {
            validate_bip32_path(hd_path)?;
            builder.derivation_path(hd_path)?
        } else {
            if index >= BIP32_HARDEN {
                return Err(WalletSignerError::InvalidBip32Index(index));
            }
            builder.index(index)?
        };

        Ok(Self::Local(builder.build()?))
    }
}

macro_rules! delegate {
    ($s:ident, $inner:ident => $e:expr) => {
        match $s {
            Self::Local($inner) => $e,
            Self::Ledger($inner) => $e,
            Self::Trezor($inner) => $e,
            #[cfg(feature = "aws-kms")]
            Self::Aws($inner) => $e,
            #[cfg(feature = "gcp-kms")]
            Self::Gcp($inner) => $e,
            #[cfg(feature = "turnkey")]
            Self::Turnkey($inner) => $e,
        }
    };
}

#[async_trait]
impl Signer for WalletSigner {
    /// Signs the given hash.
    async fn sign_hash(&self, hash: &B256) -> alloy_signer::Result<Signature> {
        delegate!(self, inner => inner.sign_hash(hash)).await
    }

    async fn sign_message(&self, message: &[u8]) -> alloy_signer::Result<Signature> {
        delegate!(self, inner => inner.sign_message(message)).await
    }

    fn address(&self) -> Address {
        delegate!(self, inner => alloy_signer::Signer::address(inner))
    }

    fn chain_id(&self) -> Option<ChainId> {
        delegate!(self, inner => inner.chain_id())
    }

    fn set_chain_id(&mut self, chain_id: Option<ChainId>) {
        delegate!(self, inner => inner.set_chain_id(chain_id))
    }

    async fn sign_typed_data<T: SolStruct + Send + Sync>(
        &self,
        payload: &T,
        domain: &Eip712Domain,
    ) -> alloy_signer::Result<Signature>
    where
        Self: Sized,
    {
        delegate!(self, inner => inner.sign_typed_data(payload, domain)).await
    }

    async fn sign_dynamic_typed_data(
        &self,
        payload: &TypedData,
    ) -> alloy_signer::Result<Signature> {
        delegate!(self, inner => inner.sign_dynamic_typed_data(payload)).await
    }
}

#[async_trait]
impl TxSigner<Signature> for WalletSigner {
    fn address(&self) -> Address {
        Signer::address(self)
    }

    async fn sign_transaction(
        &self,
        tx: &mut dyn SignableTransaction<Signature>,
    ) -> alloy_signer::Result<Signature> {
        delegate!(self, inner => TxSigner::sign_transaction(inner, tx)).await
    }
}

/// Signers that require user action to be obtained.
#[derive(Debug, Clone)]
pub enum PendingSigner {
    Keystore(PathBuf, Option<Address>),
    Interactive,
}

impl PendingSigner {
    pub fn unlock(self) -> Result<WalletSigner> {
        match self {
            Self::Keystore(path, expected_address) => {
                // Falling back to the password prompt is reserved for recoverable
                // environmental failures and is always announced on stderr.
                // Cancellation, corruption, invalidation, and a password mismatch
                // require an explicit re-enroll/remove decision.
                #[cfg(all(target_os = "macos", feature = "touch-id"))]
                if crate::touch_id::is_enrolled(&path) {
                    match crate::touch_id::unwrap_password(&path) {
                        Ok(password) => {
                            match PrivateKeySigner::decrypt_keystore(&path, password.as_str()) {
                                Ok(signer) => {
                                    return checked_keystore_signer(signer, expected_address);
                                }
                                Err(alloy_signer_local::LocalSignerError::EthKeystoreError(
                                    eth_keystore::KeystoreError::MacMismatch,
                                )) => {
                                    return Err(
                                        crate::touch_id::TouchIdError::PasswordMismatch.into()
                                    );
                                }
                                Err(e) => return Err(WalletSignerError::Local(e)),
                            }
                        }
                        Err(e) if e.is_recoverable() => {
                            eprintln!("Warning: {e}; falling back to the password prompt.")
                        }
                        Err(e) => return Err(e.into()),
                    }
                }
                let password = zeroize::Zeroizing::new(rpassword::prompt_password(
                    "Enter keystore password:",
                )?);
                match PrivateKeySigner::decrypt_keystore(path, password.as_str()) {
                    Ok(signer) => checked_keystore_signer(signer, expected_address),
                    Err(e) => match e {
                        // Catch the `MacMismatch` error, which indicates an incorrect password and
                        // return a more user-friendly `IncorrectKeystorePassword`.
                        alloy_signer_local::LocalSignerError::EthKeystoreError(
                            eth_keystore::KeystoreError::MacMismatch,
                        ) => Err(WalletSignerError::IncorrectKeystorePassword),
                        _ => Err(WalletSignerError::Local(e)),
                    },
                }
            }
            Self::Interactive => {
                let private_key = rpassword::prompt_password("Enter private key:")?;
                Ok(WalletSigner::from_private_key(&hex::FromHex::from_hex(private_key)?)?)
            }
        }
    }
}

fn validate_bip32_path(path: &str) -> Result<()> {
    for component in path.split('/') {
        let index = component
            .strip_suffix('\'')
            .or_else(|| component.strip_suffix('h'))
            .unwrap_or(component);
        if let Ok(index) = index.parse::<u32>()
            && index >= BIP32_HARDEN
        {
            return Err(WalletSignerError::InvalidBip32Index(index));
        }
    }
    Ok(())
}

fn checked_keystore_signer(
    signer: PrivateKeySigner,
    expected_address: Option<Address>,
) -> Result<WalletSigner> {
    let actual = signer.address();
    if let Some(expected) = expected_address
        && actual != expected
    {
        return Err(WalletSignerError::KeystoreAddressMismatch { expected, actual });
    }
    Ok(WalletSigner::Local(signer))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_MNEMONIC: &str = "test test test test test test test test test test test junk";

    #[test]
    fn rejects_mnemonic_indices_that_set_harden_bit() {
        assert!(matches!(
            WalletSigner::from_mnemonic(TEST_MNEMONIC, None, None, BIP32_HARDEN),
            Err(WalletSignerError::InvalidBip32Index(BIP32_HARDEN))
        ));

        for path in [
            "m/2147483648",
            "m/2147483648'",
            "m/2147483648h",
            "m/+2147483648'",
            "m/02147483648'",
            "m/4294967295",
        ] {
            assert!(matches!(
                WalletSigner::from_mnemonic(TEST_MNEMONIC, None, Some(path), 0),
                Err(WalletSignerError::InvalidBip32Index(_))
            ));
        }
    }

    #[test]
    fn accepts_largest_unhardened_mnemonic_index() {
        assert!(WalletSigner::from_mnemonic(TEST_MNEMONIC, None, None, BIP32_HARDEN - 1).is_ok());
        assert!(
            WalletSigner::from_mnemonic(TEST_MNEMONIC, None, Some("m/2147483647'"), 0,).is_ok()
        );
    }

    #[test]
    fn rejects_keystore_signer_with_unexpected_address() {
        let signer = PrivateKeySigner::from_bytes(&B256::from([1_u8; 32])).unwrap();
        let actual = signer.address();

        assert!(matches!(
            checked_keystore_signer(signer, Some(Address::ZERO)),
            Err(WalletSignerError::KeystoreAddressMismatch {
                expected: Address::ZERO,
                actual: returned,
            }) if returned == actual
        ));
    }
}
