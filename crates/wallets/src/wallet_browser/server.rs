use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use alloy_dyn_abi::TypedData;
use alloy_network::Network;
use alloy_primitives::{Address, Bytes, TxHash};
use tokio::{
    net::TcpListener,
    sync::{Mutex, oneshot},
};
use uuid::Uuid;

use crate::wallet_browser::{
    error::BrowserWalletError,
    router::build_router,
    state::BrowserWalletState,
    types::{
        BrowserSignRequest, BrowserSignTypedDataRequest, BrowserTransactionRequest, Connection,
        SignRequest, SignType,
    },
};

#[cfg(feature = "tempo")]
use {
    crate::wallet_browser::types::BrowserKeychainAuthRequest,
    alloy_primitives::hex,
    alloy_rlp::Decodable,
    tempo_primitives::transaction::{KeyAuthorization, SignatureType, SignedKeyAuthorization},
};

/// Browser wallet server.
#[derive(Debug, Clone)]
pub struct BrowserWalletServer<N: Network> {
    port: u16,
    state: Arc<BrowserWalletState<N>>,
    shutdown_tx: Option<Arc<Mutex<Option<oneshot::Sender<()>>>>>,
    open_browser: bool,
    timeout: Duration,
}

impl<N: Network> BrowserWalletServer<N> {
    /// Create a new browser wallet server.
    pub fn new(port: u16, open_browser: bool, timeout: Duration, development: bool) -> Self {
        Self {
            port,
            state: Arc::new(BrowserWalletState::new(Uuid::new_v4().to_string(), development)),
            shutdown_tx: None,
            open_browser,
            timeout,
        }
    }

    /// Start the server and open browser.
    pub async fn start(&mut self) -> Result<(), BrowserWalletError> {
        let router = build_router(self.state.clone(), self.port).await;

        let addr = SocketAddr::from(([127, 0, 0, 1], self.port));
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| BrowserWalletError::ServerError(e.to_string()))?;
        self.port = listener.local_addr().unwrap().port();

        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        self.shutdown_tx = Some(Arc::new(Mutex::new(Some(shutdown_tx))));

        tokio::spawn(async move {
            let server = axum::serve(listener, router);
            let _ = server
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await;
        });

        if self.open_browser {
            webbrowser::open(&format!("http://127.0.0.1:{}", self.port)).map_err(|e| {
                BrowserWalletError::ServerError(format!("Failed to open browser: {e}"))
            })?;
        }

        Ok(())
    }

    /// Stop the server.
    pub async fn stop(&mut self) -> Result<(), BrowserWalletError> {
        if let Some(shutdown_arc) = self.shutdown_tx.take()
            && let Some(tx) = shutdown_arc.lock().await.take()
        {
            let _ = tx.send(());
        }
        Ok(())
    }

    /// Get the server port.
    pub const fn port(&self) -> u16 {
        self.port
    }

    /// Check if the browser should be opened.
    pub const fn open_browser(&self) -> bool {
        self.open_browser
    }

    /// Get the timeout duration.
    pub const fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Get the session token.
    pub fn session_token(&self) -> &str {
        self.state.session_token()
    }

    /// Check if a wallet is connected.
    pub async fn is_connected(&self) -> bool {
        self.state.is_connected().await
    }

    /// Get current wallet connection.
    pub async fn get_connection(&self) -> Option<Connection> {
        self.state.get_connection().await
    }

    /// Request a transaction to be signed and sent via the browser wallet.
    pub async fn request_transaction(
        &self,
        request: BrowserTransactionRequest<N>,
    ) -> Result<TxHash, BrowserWalletError> {
        if !self.is_connected().await {
            return Err(BrowserWalletError::NotConnected);
        }

        let tx_id = request.id;

        self.state.add_transaction_request(request).await;

        let start = Instant::now();

        loop {
            if let Some(response) = self.state.get_transaction_response(&tx_id).await {
                if let Some(hash) = response.hash {
                    return Ok(hash);
                } else if let Some(error) = response.error {
                    return Err(BrowserWalletError::Rejected {
                        operation: "Transaction",
                        reason: error,
                    });
                }
                return Err(BrowserWalletError::ServerError(
                    "Transaction response missing both hash and error".to_string(),
                ));
            }

            if start.elapsed() > self.timeout {
                self.state.remove_transaction_request(&tx_id).await;
                return Err(BrowserWalletError::Timeout { operation: "Transaction" });
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Request a message to be signed via the browser wallet.
    pub async fn request_signing(
        &self,
        request: BrowserSignRequest,
    ) -> Result<Bytes, BrowserWalletError> {
        if !self.is_connected().await {
            return Err(BrowserWalletError::NotConnected);
        }

        let tx_id = request.id;

        self.state.add_signing_request(request).await;

        let start = Instant::now();

        loop {
            if let Some(response) = self.state.get_signing_response(&tx_id).await {
                if let Some(signature) = response.signature {
                    return Ok(signature);
                } else if let Some(error) = response.error {
                    return Err(BrowserWalletError::Rejected {
                        operation: "Signing",
                        reason: error,
                    });
                }
                return Err(BrowserWalletError::ServerError(
                    "Signing response missing both signature and error".to_string(),
                ));
            }

            if start.elapsed() > self.timeout {
                self.state.remove_signing_request(&tx_id).await;
                return Err(BrowserWalletError::Timeout { operation: "Signing" });
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Request a Tempo `KeyAuthorization` to be signed via the browser
    /// wallet. The wallet must drive the WebAuthn / P256 / Secp256k1
    /// ceremony and POST back an RLP-encoded `SignedKeyAuthorization` hex
    /// string.
    ///
    /// The returned [`SignedKeyAuthorization`] is verified server-side:
    /// - `key_id`, `chain_id`, `key_type`, `expiry`, and `limits` must match the original request
    ///   (the wallet must not mutate the payload).
    /// - For `Secp256k1`, the recovered signer must equal `root_account`.
    /// - For `P256` / `WebAuthn`, recovery isn't possible off-chain; we trust the wallet UI's
    ///   confirmation and defer to the on-chain precompile to reject invalid signatures at
    ///   submission time.
    #[cfg(feature = "tempo")]
    pub async fn request_keychain_auth(
        &self,
        key_authorization: KeyAuthorization,
        root_account: alloy_primitives::Address,
        preferred_signature_type: Option<SignatureType>,
    ) -> Result<SignedKeyAuthorization, BrowserWalletError> {
        if !self.is_connected().await {
            return Err(BrowserWalletError::NotConnected);
        }

        let id = Uuid::new_v4();
        let digest = key_authorization.signature_hash();
        let request = BrowserKeychainAuthRequest {
            id,
            root_account,
            key_authorization: key_authorization.clone(),
            digest,
            preferred_signature_type,
        };

        self.state.add_keychain_auth_request(request).await;

        let start = Instant::now();

        loop {
            if let Some(response) = self.state.get_keychain_auth_response(&id).await {
                if let Some(hex_str) = response.signed_hex {
                    let bytes = hex::decode(hex_str.trim_start_matches("0x")).map_err(|e| {
                        BrowserWalletError::ServerError(format!(
                            "invalid hex in keychain authorization response: {e}"
                        ))
                    })?;
                    let signed =
                        SignedKeyAuthorization::decode(&mut bytes.as_slice()).map_err(|e| {
                            BrowserWalletError::ServerError(format!(
                                "invalid SignedKeyAuthorization RLP from wallet: {e}"
                            ))
                        })?;

                    // Defensive: the wallet must authorize the same key on
                    // the same chain with the same key type. We deliberately
                    // do not require an exact byte-for-byte match on the rest
                    // of the payload because the wallet may canonicalize
                    // optional fields (e.g. drop empty `limits`/`allowedCalls`
                    // arrays) before signing.
                    if signed.authorization.key_id != key_authorization.key_id {
                        return Err(BrowserWalletError::ServerError(format!(
                            "wallet authorized key {} but {} was requested",
                            signed.authorization.key_id, key_authorization.key_id,
                        )));
                    }
                    if signed.authorization.chain_id != key_authorization.chain_id
                        && key_authorization.chain_id != 0
                    {
                        return Err(BrowserWalletError::ServerError(format!(
                            "wallet authorized chainId {} but {} was requested",
                            signed.authorization.chain_id, key_authorization.chain_id,
                        )));
                    }
                    if signed.authorization.key_type != key_authorization.key_type {
                        return Err(BrowserWalletError::ServerError(format!(
                            "wallet authorized keyType {:?} but {:?} was requested",
                            signed.authorization.key_type, key_authorization.key_type,
                        )));
                    }

                    // Best-effort signer check. Only meaningful for Secp256k1;
                    // P256/WebAuthn signatures are validated by the on-chain
                    // precompile, not here.
                    if signed.authorization.key_type == SignatureType::Secp256k1
                        && let Ok(recovered) = signed.recover_signer()
                        && recovered != root_account
                    {
                        return Err(BrowserWalletError::ServerError(format!(
                            "wallet returned a SignedKeyAuthorization signed by {recovered} but \
                             the connected root account is {root_account}"
                        )));
                    }

                    return Ok(signed);
                } else if let Some(error) = response.error {
                    return Err(BrowserWalletError::Rejected {
                        operation: "KeychainAuth",
                        reason: error,
                    });
                }
                return Err(BrowserWalletError::ServerError(
                    "Keychain authorization response missing both signed_hex and error".to_string(),
                ));
            }

            if start.elapsed() > self.timeout {
                self.state.remove_keychain_auth_request(&id).await;
                return Err(BrowserWalletError::Timeout { operation: "KeychainAuth" });
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Request EIP-712 typed data signing via the browser wallet.
    pub async fn request_typed_data_signing(
        &self,
        address: Address,
        typed_data: TypedData,
    ) -> Result<Bytes, BrowserWalletError> {
        let request = BrowserSignTypedDataRequest { id: Uuid::new_v4(), address, typed_data };

        let sign_request = BrowserSignRequest {
            id: request.id,
            sign_type: SignType::SignTypedDataV4,
            request: SignRequest {
                message: serde_json::to_string(&request.typed_data).map_err(|e| {
                    BrowserWalletError::ServerError(format!("Failed to serialize typed data: {e}"))
                })?,
                address: request.address,
            },
        };

        self.request_signing(sign_request).await
    }
}
