#[cfg(feature = "tempo")]
use crate::TempoAccountsWallet;
use crate::{signer::WalletSigner, utils, wallet_raw::RawWalletOpts};
use alloy_primitives::Address;
use clap::Parser;
use eyre::Result;
use serde::Serialize;

/// When the `tempo` feature is enabled this is [`TempoAccountsWallet`];
/// otherwise it is `()` so the API shape stays the same.
#[cfg(feature = "tempo")]
pub type MaybeTempoWallet = TempoAccountsWallet;
#[cfg(not(feature = "tempo"))]
pub type MaybeTempoWallet = ();

/// The wallet options can either be:
/// 1. Raw (via private key / mnemonic file, see `RawWallet`)
/// 2. Keystore (via file path)
/// 3. Ledger
/// 4. Trezor
/// 5. AWS KMS
/// 6. Google Cloud KMS
/// 7. Turnkey
/// 8. Browser wallet
#[derive(Clone, Debug, Default, Serialize, Parser)]
#[command(next_help_heading = "Wallet options", about = None, long_about = None)]
pub struct WalletOpts {
    /// The sender account.
    #[arg(
        long,
        short,
        value_name = "ADDRESS",
        help_heading = "Wallet options - raw",
        env = "ETH_FROM"
    )]
    pub from: Option<Address>,

    #[command(flatten)]
    pub raw: RawWalletOpts,

    /// Use the keystore in the given folder or file.
    #[arg(
        long = "keystore",
        help_heading = "Wallet options - keystore",
        value_name = "PATH",
        env = "ETH_KEYSTORE"
    )]
    pub keystore_path: Option<String>,

    /// Use a keystore from the default keystores folder (~/.foundry/keystores) by its filename
    #[arg(
        long = "account",
        help_heading = "Wallet options - keystore",
        value_name = "ACCOUNT_NAME",
        env = "ETH_KEYSTORE_ACCOUNT",
        conflicts_with = "keystore_path"
    )]
    pub keystore_account_name: Option<String>,

    /// The keystore password.
    ///
    /// Used with --keystore.
    #[arg(
        long = "password",
        help_heading = "Wallet options - keystore",
        requires = "keystore_path",
        value_name = "PASSWORD"
    )]
    pub keystore_password: Option<String>,

    /// The keystore password file path.
    ///
    /// Used with --keystore.
    #[arg(
        long = "password-file",
        help_heading = "Wallet options - keystore",
        requires = "keystore_path",
        value_name = "PASSWORD_FILE",
        env = "ETH_PASSWORD"
    )]
    pub keystore_password_file: Option<String>,

    /// Use a Ledger hardware wallet.
    #[arg(long, short, help_heading = "Wallet options - hardware wallet")]
    pub ledger: bool,

    /// Use a Trezor hardware wallet.
    #[arg(long, short, help_heading = "Wallet options - hardware wallet")]
    pub trezor: bool,

    /// Use AWS Key Management Service.
    ///
    /// Ensure the AWS_KMS_KEY_ID environment variable is set.
    #[arg(long, help_heading = "Wallet options - remote", hide = !cfg!(feature = "aws-kms"))]
    pub aws: bool,

    /// Use Google Cloud Key Management Service.
    ///
    /// Ensure the following environment variables are set: GCP_PROJECT_ID, GCP_LOCATION,
    /// GCP_KEY_RING, GCP_KEY_NAME, GCP_KEY_VERSION.
    ///
    /// See: <https://cloud.google.com/kms/docs>
    #[arg(long, help_heading = "Wallet options - remote", hide = !cfg!(feature = "gcp-kms"))]
    pub gcp: bool,

    /// Use Turnkey.
    ///
    /// Ensure the following environment variables are set: TURNKEY_API_PRIVATE_KEY,
    /// TURNKEY_ORGANIZATION_ID, TURNKEY_ADDRESS.
    ///
    /// See: <https://docs.turnkey.com/getting-started/quickstart>
    #[arg(long, help_heading = "Wallet options - remote", hide = !cfg!(feature = "turnkey"))]
    pub turnkey: bool,

    /// Tempo access key private key.
    ///
    /// When set, the transaction is signed with this access key on behalf of
    /// `--tempo.root-account`.
    #[arg(
        long = "tempo.access-key",
        help_heading = "Wallet options - Tempo",
        value_name = "PRIVATE_KEY",
        env = "TEMPO_ACCESS_KEY",
        hide = !cfg!(feature = "tempo")
    )]
    pub tempo_access_key: Option<String>,

    /// Tempo root account address (the `from` address for keychain transactions).
    ///
    /// Required when `--tempo.access-key` is set.
    #[arg(
        long = "tempo.root-account",
        help_heading = "Wallet options - Tempo",
        value_name = "ADDRESS",
        requires = "tempo_access_key",
        env = "TEMPO_ROOT_ACCOUNT",
        hide = !cfg!(feature = "tempo")
    )]
    pub tempo_root_account: Option<Address>,
}

impl WalletOpts {
    /// Attempts to resolve a signer from the configured wallet options.
    ///
    /// Returns the signer and, when the `tempo` feature is enabled and Tempo Accounts mode is
    /// used, a [`TempoAccountsWallet`] owning the complete signing context.
    ///
    /// Returns `Ok((None, None))` if no wallet option was configured and no Tempo fallback
    /// matched.
    pub async fn maybe_signer(&self) -> Result<(Option<WalletSigner>, Option<MaybeTempoWallet>)> {
        self.maybe_signer_inner(None).await
    }

    /// Attempts to resolve a signer for a concrete chain.
    ///
    /// This is identical to [`Self::maybe_signer`], except the Tempo Accounts
    /// wallet is pinned to the selected chain.
    pub async fn maybe_signer_for_chain(
        &self,
        chain_id: u64,
    ) -> Result<(Option<WalletSigner>, Option<MaybeTempoWallet>)> {
        self.maybe_signer_inner(Some(chain_id)).await
    }

    async fn maybe_signer_inner(
        &self,
        chain_id: Option<u64>,
    ) -> Result<(Option<WalletSigner>, Option<MaybeTempoWallet>)> {
        trace!("start finding signer");
        #[cfg(not(feature = "tempo"))]
        let _ = chain_id;

        // If a Tempo access key is provided on the CLI, use it directly.
        #[cfg(feature = "tempo")]
        if let Some(ref access_key) = self.tempo_access_key {
            let root_account = self.tempo_root_account.ok_or_else(|| {
                eyre::eyre!("--tempo.root-account is required when --tempo.access-key is set")
            })?;
            let signer = utils::create_local_signer(access_key)?;
            let wallet = TempoAccountsWallet::from_secp256k1(root_account, signer, None);
            let wallet = match chain_id {
                Some(chain_id) => wallet.with_chain_id(chain_id),
                None => wallet,
            };
            return Ok((None, Some(wallet)));
        }

        let get_env = |key: &str| {
            std::env::var(key)
                .map_err(|_| eyre::eyre!("{key} environment variable is required for signer"))
        };

        let signer = if self.ledger {
            utils::create_ledger_signer(self.raw.hd_path.as_deref(), self.raw.mnemonic_index)
                .await?
        } else if self.trezor {
            utils::create_trezor_signer(self.raw.hd_path.as_deref(), self.raw.mnemonic_index)
                .await?
        } else if self.aws {
            let key_id = get_env("AWS_KMS_KEY_ID")?;
            WalletSigner::from_aws(key_id).await?
        } else if self.gcp {
            let project_id = get_env("GCP_PROJECT_ID")?;
            let location = get_env("GCP_LOCATION")?;
            let keyring = get_env("GCP_KEY_RING")?;
            let key_name = get_env("GCP_KEY_NAME")?;
            let key_version = get_env("GCP_KEY_VERSION")?
                .parse()
                .map_err(|_| eyre::eyre!("GCP_KEY_VERSION could not be parsed into u64"))?;
            WalletSigner::from_gcp(project_id, location, keyring, key_name, key_version).await?
        } else if self.turnkey {
            let api_private_key = get_env("TURNKEY_API_PRIVATE_KEY")?;
            let organization_id = get_env("TURNKEY_ORGANIZATION_ID")?;
            let address_str = get_env("TURNKEY_ADDRESS")?;
            let address = address_str.parse().map_err(|_| {
                eyre::eyre!("TURNKEY_ADDRESS could not be parsed as an Ethereum address")
            })?;
            WalletSigner::from_turnkey(api_private_key, organization_id, address)?
        } else if let Some(raw_wallet) = self.raw.signer()? {
            raw_wallet
        } else if let Some(path) = utils::maybe_get_keystore_path(
            self.keystore_path.as_deref(),
            self.keystore_account_name.as_deref(),
        )? {
            let (maybe_signer, maybe_pending) = utils::create_keystore_signer(
                &path,
                self.keystore_password.as_deref(),
                self.keystore_password_file.as_deref(),
            )?;
            if let Some(pending) = maybe_pending {
                pending.unlock()?
            } else if let Some(signer) = maybe_signer {
                signer
            } else {
                unreachable!()
            }
        } else {
            // No explicit wallet option was provided. Try the Tempo Accounts store
            // if `--from` selects one of its accounts.
            #[cfg(feature = "tempo")]
            if let Some(from) = self.from
                && let Some(wallet) = TempoAccountsWallet::try_from_default_store()?
                && wallet.has_account(from)?
            {
                let wallet = match chain_id {
                    Some(chain_id) => wallet.with_chain_id(chain_id),
                    None => wallet,
                };
                return Ok((None, Some(wallet)));
            }

            return Ok((None, None));
        };

        Ok((Some(signer), None))
    }

    pub async fn signer(&self) -> Result<WalletSigner> {
        self.maybe_signer().await?.0.ok_or_else(|| {
            eyre::eyre!(
                "\
Error accessing local wallet. Did you pass a keystore, hardware wallet, private key or mnemonic?

Run the command with --help flag for more information or use the corresponding CLI
flag to set your key via:

--keystore
--interactive
--private-key
--mnemonic-path
--aws
--gcp
--turnkey
--trezor
--ledger
--browser

Alternatively, when using the `cast send` or `cast mktx` commands with a local node
or RPC that has unlocked accounts, the --unlocked or --ethsign flags can be used,
respectively. The sender address can be specified by setting the `ETH_FROM` environment
variable to the desired unlocked account address, or by providing the address directly
using the --from flag."
            )
        })
    }
}

impl From<RawWalletOpts> for WalletOpts {
    fn from(options: RawWalletOpts) -> Self {
        Self { raw: options, ..Default::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(redundant_imports)]
    use alloy_signer::Signer;
    use std::{path::Path, str::FromStr};

    #[tokio::test]
    async fn find_keystore() {
        let keystore = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/test-data/keystore"));
        let keystore_file = keystore
            .join("UTC--2022-12-20T10-30-43.591916000Z--ec554aeafe75601aaab43bd4621a22284db566c2");
        let password_file = keystore.join("password-ec554");
        let wallet: WalletOpts = WalletOpts::parse_from([
            "foundry-cli",
            "--from",
            "560d246fcddc9ea98a8b032c9a2f474efb493c28",
            "--keystore",
            keystore_file.to_str().unwrap(),
            "--password-file",
            password_file.to_str().unwrap(),
        ]);
        let signer = wallet.signer().await.unwrap();
        assert_eq!(
            signer.address(),
            Address::from_str("ec554aeafe75601aaab43bd4621a22284db566c2").unwrap()
        );
    }

    #[tokio::test]
    async fn illformed_private_key_generates_user_friendly_error() {
        let wallet = WalletOpts {
            raw: RawWalletOpts {
                interactive: false,
                private_key: Some("123".to_string()),
                mnemonic: None,
                mnemonic_passphrase: None,
                hd_path: None,
                mnemonic_index: 0,
            },
            from: None,
            keystore_path: None,
            keystore_account_name: None,
            keystore_password: None,
            keystore_password_file: None,
            ledger: false,
            trezor: false,
            aws: false,
            gcp: false,
            turnkey: false,
            tempo_access_key: None,
            tempo_root_account: None,
        };
        match wallet.signer().await {
            Ok(_) => {
                panic!("illformed private key shouldn't decode")
            }
            Err(x) => {
                assert!(
                    x.to_string().contains("Failed to decode private key"),
                    "Error message is not user-friendly"
                );
            }
        }
    }

    #[cfg(feature = "tempo")]
    #[tokio::test]
    async fn tempo_access_key_resolves_to_one_wallet() {
        let account = Address::repeat_byte(0x11);
        let wallet = WalletOpts {
            tempo_access_key: Some(
                "0x59c6995e998f97a5a004497e5da3b5d2b2b66a87f064d39c44da0b6d6e4f8ff0".to_owned(),
            ),
            tempo_root_account: Some(account),
            ..Default::default()
        };

        let (signer, tempo_wallet) = wallet.maybe_signer().await.unwrap();
        assert!(signer.is_none());
        assert_eq!(tempo_wallet.unwrap().account(), account);
    }
}
