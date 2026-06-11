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
        SessionInfo, SignRequest, SignType,
    },
};

#[cfg(feature = "tempo")]
use {
    crate::wallet_browser::types::BrowserKeyAuthorizationRequest,
    alloy_primitives::hex,
    alloy_rlp::Decodable,
    tempo_primitives::transaction::{KeyAuthorization, SignedKeyAuthorization},
};

/// Browser wallet server.
#[derive(Debug, Clone)]
pub struct BrowserWalletServer<N: Network> {
    port: u16,
    state: Arc<BrowserWalletState<N>>,
    shutdown_tx: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    open_browser: bool,
    timeout: Duration,
}

impl<N: Network> BrowserWalletServer<N> {
    /// Create a new browser wallet server.
    pub fn new(port: u16, open_browser: bool, timeout: Duration, development: bool) -> Self {
        Self {
            port,
            state: Arc::new(BrowserWalletState::new(Uuid::new_v4().to_string(), development)),
            shutdown_tx: Arc::new(Mutex::new(None)),
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
        *self.shutdown_tx.lock().await = Some(shutdown_tx);

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

    /// Stop the server. Marks the session as shutting down (so the next
    /// `/api/session` poll from the webapp surfaces `alive: false`) and
    /// triggers the axum graceful-shutdown signal.
    pub async fn stop(&self) -> Result<(), BrowserWalletError> {
        self.state.set_shutting_down();
        if let Some(tx) = self.shutdown_tx.lock().await.take() {
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

    /// Get the current session info (alive + connected).
    pub async fn session_info(&self) -> SessionInfo {
        SessionInfo {
            alive: !self.state.is_shutting_down(),
            connected: self.state.is_connected().await,
        }
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
    /// - The signed authorization payload must match the original request (the wallet must not
    ///   mutate security-relevant fields).
    /// - The signature must recover to `root_account` for every supported signature scheme.
    #[cfg(feature = "tempo")]
    pub async fn request_key_authorization(
        &self,
        key_authorization: KeyAuthorization,
        root_account: Address,
    ) -> Result<SignedKeyAuthorization, BrowserWalletError> {
        if !self.is_connected().await {
            return Err(BrowserWalletError::NotConnected);
        }
        reject_unsupported_key_authorization_fields(&key_authorization)?;

        let id = Uuid::new_v4();
        let digest = key_authorization.signature_hash();
        let request = BrowserKeyAuthorizationRequest {
            id,
            root_account,
            key_authorization: key_authorization.clone(),
            digest,
        };

        self.state.add_key_authorization_request(request).await;

        let start = Instant::now();

        loop {
            if let Some(response) = self.state.get_key_authorization_response(&id).await {
                if let Some(hex_str) = response.signed_hex {
                    let bytes = hex::decode(hex_str.trim_start_matches("0x")).map_err(|e| {
                        BrowserWalletError::ServerError(format!(
                            "invalid hex in key authorization response: {e}"
                        ))
                    })?;
                    let signed =
                        SignedKeyAuthorization::decode(&mut bytes.as_slice()).map_err(|e| {
                            BrowserWalletError::ServerError(format!(
                                "invalid SignedKeyAuthorization RLP from wallet: {e}"
                            ))
                        })?;

                    if signed.authorization != key_authorization {
                        return Err(BrowserWalletError::ServerError(format!(
                            "wallet returned a mutated KeyAuthorization payload: signed digest {} \
                             but requested {}",
                            signed.authorization.signature_hash(),
                            key_authorization.signature_hash(),
                        )));
                    }

                    match signed.recover_signer() {
                        Ok(recovered) if recovered == root_account => {}
                        Ok(recovered) => {
                            return Err(BrowserWalletError::ServerError(format!(
                                "wallet returned a SignedKeyAuthorization signed by \
                                 {recovered} but the connected root account is {root_account}"
                            )));
                        }
                        Err(e) => {
                            return Err(BrowserWalletError::ServerError(format!(
                                "wallet returned an unrecoverable SignedKeyAuthorization \
                                 signature: {e}"
                            )));
                        }
                    }

                    return Ok(signed);
                } else if let Some(error) = response.error {
                    return Err(BrowserWalletError::Rejected {
                        operation: "KeyAuthorization",
                        reason: error,
                    });
                }
                return Err(BrowserWalletError::ServerError(
                    "Key authorization response missing both signed_hex and error".to_string(),
                ));
            }

            if start.elapsed() > self.timeout {
                self.state.remove_key_authorization_request(&id).await;
                return Err(BrowserWalletError::Timeout { operation: "KeyAuthorization" });
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

#[cfg(feature = "tempo")]
fn reject_unsupported_key_authorization_fields(
    key_authorization: &KeyAuthorization,
) -> Result<(), BrowserWalletError> {
    let mut unsupported_fields = Vec::new();
    if key_authorization.witness.is_some() {
        unsupported_fields.push("witness");
    }
    if key_authorization.account.is_some() {
        unsupported_fields.push("account");
    }
    if key_authorization.is_admin {
        unsupported_fields.push("is_admin");
    }

    if !unsupported_fields.is_empty() {
        return Err(BrowserWalletError::ServerError(format!(
            "browser key authorization signing does not support T5 fields yet: {}",
            unsupported_fields.join(", ")
        )));
    }

    Ok(())
}
