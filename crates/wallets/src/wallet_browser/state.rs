use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use alloy_network::Network;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::wallet_browser::{
    queue::RequestQueue,
    types::{
        BrowserSignRequest, BrowserSignResponse, BrowserTransactionRequest,
        BrowserTransactionResponse, Connection,
    },
};

#[cfg(feature = "tempo")]
use crate::wallet_browser::types::{BrowserKeychainAuthRequest, BrowserKeychainAuthResponse};

/// Reason placed in synthesized error responses when in-flight requests are
/// failed because the wallet was disconnected.
pub(crate) const DISCONNECT_REASON: &str = "Wallet disconnected";

#[derive(Debug, Clone)]
pub(crate) struct BrowserWalletState<N: Network> {
    /// Current information about the wallet connection.
    connection: Arc<RwLock<Option<Connection>>>,
    /// Request/response queue for transactions.
    transactions:
        Arc<Mutex<RequestQueue<BrowserTransactionRequest<N>, BrowserTransactionResponse>>>,
    /// Request/response queue for signings.
    signings: Arc<Mutex<RequestQueue<BrowserSignRequest, BrowserSignResponse>>>,
    /// Request/response queue for Tempo `KeyAuthorization` signings.
    #[cfg(feature = "tempo")]
    keychain_auths:
        Arc<Mutex<RequestQueue<BrowserKeychainAuthRequest, BrowserKeychainAuthResponse>>>,
    /// Unique session token for the wallet browser instance.
    /// The CSP on the served page prevents this token from being loaded by other origins.
    session_token: String,
    /// If true, the server is running in development mode.
    /// This relaxes certain security restrictions for local development.
    ///
    /// **WARNING**: This should only be used in a development environment.
    development: bool,
    /// Whether the server is shutting down. Once flipped, the `/api/session`
    /// endpoint reports `alive: false` so the webapp can stop polling.
    shutting_down: Arc<AtomicBool>,
}

impl<N: Network> BrowserWalletState<N> {
    /// Create a new browser wallet state.
    pub fn new(session_token: String, development: bool) -> Self {
        Self {
            connection: Arc::new(RwLock::new(None)),
            transactions: Arc::new(Mutex::new(RequestQueue::new())),
            signings: Arc::new(Mutex::new(RequestQueue::new())),
            #[cfg(feature = "tempo")]
            keychain_auths: Arc::new(Mutex::new(RequestQueue::new())),
            session_token,
            development,
            shutting_down: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Get the session token.
    pub fn session_token(&self) -> &str {
        &self.session_token
    }

    /// Check if in development mode.
    /// This relaxes certain security restrictions for local development.
    ///
    /// **WARNING**: This should only be used in a development environment.
    pub const fn is_development(&self) -> bool {
        self.development
    }

    /// Mark the server as shutting down.
    pub fn set_shutting_down(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
    }

    /// Whether the server is shutting down.
    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::SeqCst)
    }

    /// Check if wallet is connected.
    pub async fn is_connected(&self) -> bool {
        self.connection.read().await.is_some()
    }

    /// Get current connection information.
    pub async fn get_connection(&self) -> Option<Connection> {
        *self.connection.read().await
    }

    /// Set connection information. When the new connection is `None`, all
    /// pending transaction and signing requests are failed with a synthetic
    /// `Wallet disconnected` error response so that in-flight callers
    /// (`request_transaction`, `request_signing`) return immediately rather
    /// than waiting for their per-request timeout.
    pub async fn set_connection(&self, connection: Option<Connection>) {
        *self.connection.write().await = connection;

        if connection.is_none() {
            self.fail_pending_with_disconnect().await;
        }
    }

    /// Fail all in-flight transaction and signing requests with a synthetic
    /// `Wallet disconnected` error response.
    async fn fail_pending_with_disconnect(&self) {
        {
            let mut txs = self.transactions.lock().await;
            for id in txs.drain_request_ids() {
                txs.add_response(
                    id,
                    BrowserTransactionResponse {
                        id,
                        hash: None,
                        error: Some(DISCONNECT_REASON.to_string()),
                    },
                );
            }
        }
        {
            let mut signs = self.signings.lock().await;
            for id in signs.drain_request_ids() {
                signs.add_response(
                    id,
                    BrowserSignResponse {
                        id,
                        signature: None,
                        error: Some(DISCONNECT_REASON.to_string()),
                    },
                );
            }
        }
    }

    /// Add a transaction request.
    pub async fn add_transaction_request(&self, request: BrowserTransactionRequest<N>) {
        self.transactions.lock().await.add_request(request);
    }

    /// Check if a transaction request exists.
    pub async fn has_transaction_request(&self, id: &Uuid) -> bool {
        self.transactions.lock().await.has_request(id)
    }

    /// Read the next transaction request.
    pub async fn read_next_transaction_request(&self) -> Option<BrowserTransactionRequest<N>> {
        self.transactions.lock().await.read_request().cloned()
    }

    // Remove a transaction request.
    pub async fn remove_transaction_request(&self, id: &Uuid) {
        self.transactions.lock().await.remove_request(id);
    }

    /// Add transaction response.
    pub async fn add_transaction_response(&self, response: BrowserTransactionResponse) {
        let id = response.id;
        let mut transactions = self.transactions.lock().await;
        transactions.add_response(id, response);
        transactions.remove_request(&id);
    }

    /// Get transaction response, removing it from the queue.
    pub async fn get_transaction_response(&self, id: &Uuid) -> Option<BrowserTransactionResponse> {
        self.transactions.lock().await.get_response(id)
    }

    /// Add a signing request.
    pub async fn add_signing_request(&self, request: BrowserSignRequest) {
        self.signings.lock().await.add_request(request);
    }

    /// Check if a signing request exists.
    pub async fn has_signing_request(&self, id: &Uuid) -> bool {
        self.signings.lock().await.has_request(id)
    }

    /// Read the next signing request.
    pub async fn read_next_signing_request(&self) -> Option<BrowserSignRequest> {
        self.signings.lock().await.read_request().cloned()
    }

    /// Remove a signing request.
    pub async fn remove_signing_request(&self, id: &Uuid) {
        self.signings.lock().await.remove_request(id);
    }

    /// Add signing response.
    pub async fn add_signing_response(&self, response: BrowserSignResponse) {
        let id = response.id;
        let mut signings = self.signings.lock().await;
        signings.add_response(id, response);
        signings.remove_request(&id);
    }

    /// Get signing response, removing it from the queue.
    pub async fn get_signing_response(&self, id: &Uuid) -> Option<BrowserSignResponse> {
        self.signings.lock().await.get_response(id)
    }

    // -- Tempo `KeyAuthorization` signings -----------------------------------

    /// Add a Tempo `KeyAuthorization` signing request.
    #[cfg(feature = "tempo")]
    pub async fn add_keychain_auth_request(&self, request: BrowserKeychainAuthRequest) {
        self.keychain_auths.lock().await.add_request(request);
    }

    /// Check if a Tempo `KeyAuthorization` signing request exists.
    #[cfg(feature = "tempo")]
    pub async fn has_keychain_auth_request(&self, id: &Uuid) -> bool {
        self.keychain_auths.lock().await.has_request(id)
    }

    /// Read the next Tempo `KeyAuthorization` signing request.
    #[cfg(feature = "tempo")]
    pub async fn read_next_keychain_auth_request(&self) -> Option<BrowserKeychainAuthRequest> {
        self.keychain_auths.lock().await.read_request().cloned()
    }

    /// Remove a Tempo `KeyAuthorization` signing request.
    #[cfg(feature = "tempo")]
    pub async fn remove_keychain_auth_request(&self, id: &Uuid) {
        self.keychain_auths.lock().await.remove_request(id);
    }

    /// Add a Tempo `KeyAuthorization` signing response.
    #[cfg(feature = "tempo")]
    pub async fn add_keychain_auth_response(&self, response: BrowserKeychainAuthResponse) {
        let id = response.id;
        let mut keychain_auths = self.keychain_auths.lock().await;
        keychain_auths.add_response(id, response);
        keychain_auths.remove_request(&id);
    }

    /// Get a Tempo `KeyAuthorization` signing response, removing it from the queue.
    #[cfg(feature = "tempo")]
    pub async fn get_keychain_auth_response(
        &self,
        id: &Uuid,
    ) -> Option<BrowserKeychainAuthResponse> {
        self.keychain_auths.lock().await.get_response(id)
    }
}
