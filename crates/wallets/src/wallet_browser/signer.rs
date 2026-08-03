use std::{
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use alloy_dyn_abi::TypedData;
use alloy_network::{Network, TransactionBuilder};
use alloy_primitives::{Address, B256, ChainId, Signature, hex};
use alloy_signer::Result;
use uuid::Uuid;

use crate::wallet_browser::{
    server::BrowserWalletServer,
    types::{BrowserSignRequest, BrowserTransactionRequest, Connection, SignRequest, SignType},
};

#[cfg(feature = "tempo")]
use tempo_alloy::primitives::transaction::{KeyAuthorization, SignedKeyAuthorization};

#[derive(Clone, Debug)]
pub struct BrowserSigner<N: Network> {
    server: Arc<BrowserWalletServer<N>>,
    address: Address,
    chain_id: Arc<RwLock<ChainId>>,
}

impl<N: Network> BrowserSigner<N> {
    pub async fn new(
        port: u16,
        open_browser: bool,
        timeout: Duration,
        development: bool,
    ) -> Result<Self> {
        let mut server = BrowserWalletServer::new(port, open_browser, timeout, development);

        server.start().await.map_err(alloy_signer::Error::other)?;

        // TODO: use sh_* macros once extracted from foundry-common
        eprintln!("Warning: Browser wallet is still in early development. Use with caution!");
        eprintln!("Opening browser for wallet connection...");
        eprintln!("Waiting for wallet connection...");

        let start = Instant::now();

        loop {
            if let Some(Connection { address, chain_id }) = server.get_connection().await {
                eprintln!("Wallet connected: {address}");
                eprintln!("Chain ID: {chain_id}");

                return Ok(Self {
                    server: Arc::new(server),
                    address,
                    chain_id: Arc::new(RwLock::new(chain_id)),
                });
            }

            if start.elapsed() > timeout {
                return Err(alloy_signer::Error::other("Wallet connection timeout"));
            }

            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    /// Send a transaction through the browser wallet.
    pub async fn send_transaction_via_browser(
        &self,
        tx_request: N::TransactionRequest,
    ) -> Result<B256> {
        if let Some(from) = tx_request.from()
            && from != self.address
        {
            return Err(alloy_signer::Error::other(
                "Transaction `from` address does not match connected wallet address",
            ));
        }

        if let Some(chain_id) = tx_request.chain_id()
            && chain_id != self.chain_id()
        {
            return Err(alloy_signer::Error::other(
                "Transaction `chainId` does not match connected wallet chain ID",
            ));
        }

        let request = BrowserTransactionRequest { id: Uuid::new_v4(), request: tx_request };

        let tx_hash =
            self.server.request_transaction(request).await.map_err(alloy_signer::Error::other)?;

        Ok(tx_hash)
    }

    /// Ask the connected browser wallet to sign a personal message.
    pub async fn sign_message(&self, message: &[u8]) -> Result<Signature> {
        let request = BrowserSignRequest {
            id: Uuid::new_v4(),
            sign_type: SignType::PersonalSign,
            request: SignRequest {
                message: format!("0x{}", hex::encode(message)),
                address: self.address,
            },
        };
        let signature =
            self.server.request_signing(request).await.map_err(alloy_signer::Error::other)?;
        Signature::from_raw(&signature).map_err(alloy_signer::Error::other)
    }

    /// Ask the connected browser wallet to sign dynamic EIP-712 typed data.
    pub async fn sign_dynamic_typed_data(&self, typed_data: &TypedData) -> Result<Signature> {
        let signature = self
            .server
            .request_typed_data_signing(self.address, typed_data.clone())
            .await
            .map_err(alloy_signer::Error::other)?;
        Signature::from_raw(&signature).map_err(alloy_signer::Error::other)
    }

    #[cfg(test)]
    pub(super) fn from_server(server: BrowserWalletServer<N>, connection: Connection) -> Self {
        Self {
            server: Arc::new(server),
            address: connection.address,
            chain_id: Arc::new(RwLock::new(connection.chain_id)),
        }
    }

    pub const fn address(&self) -> Address {
        self.address
    }

    pub fn chain_id(&self) -> ChainId {
        *self.chain_id.read().expect("chain ID lock poisoned")
    }

    /// Ask the connected browser wallet to switch chains.
    pub async fn switch_chain(&self, chain_id: ChainId) -> Result<()> {
        if chain_id == self.chain_id() {
            return Ok(());
        }

        let switched_chain_id =
            self.server.request_chain_switch(chain_id).await.map_err(alloy_signer::Error::other)?;
        if switched_chain_id != chain_id {
            return Err(alloy_signer::Error::other(format!(
                "Wallet switched to chain ID {switched_chain_id}, expected {chain_id}",
            )));
        }

        *self.chain_id.write().expect("chain ID lock poisoned") = switched_chain_id;

        Ok(())
    }

    /// Ask the connected browser wallet to sign a Tempo `KeyAuthorization`.
    ///
    /// Cross-checks that the connected wallet is the root account named by
    /// `key_authorization.chain_id` (when non-zero) and that this signer's
    /// address matches what we'll send as `root_account`.
    #[cfg(feature = "tempo")]
    pub async fn sign_key_authorization(
        &self,
        key_authorization: KeyAuthorization,
    ) -> Result<SignedKeyAuthorization> {
        let chain_id = self.chain_id();
        if key_authorization.chain_id != 0 && key_authorization.chain_id != chain_id {
            return Err(alloy_signer::Error::other(format!(
                "KeyAuthorization chainId {} does not match connected wallet chain ID {}",
                key_authorization.chain_id, chain_id,
            )));
        }

        self.server
            .request_key_authorization(key_authorization, self.address)
            .await
            .map_err(alloy_signer::Error::other)
    }
}

impl<N: Network> Drop for BrowserSigner<N> {
    fn drop(&mut self) {
        let server = self.server.clone();

        tokio::spawn(async move {
            let _ = server.stop().await;
        });
    }
}
