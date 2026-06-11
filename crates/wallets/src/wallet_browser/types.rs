use alloy_dyn_abi::TypedData;
use alloy_network::Network;
use alloy_primitives::{Address, Bytes, ChainId, TxHash};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[cfg(feature = "tempo")]
use {alloy_primitives::B256, tempo_primitives::transaction::KeyAuthorization};

/// Response format for API endpoints.
/// - `Ok(T)` serializes as: {"status":"ok","data": ...}
/// - `Ok(())` serializes as: {"status":"ok"}  (no data key)
/// - `Error { message }` as: {"status":"error","message":"..."}
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "status", content = "data", rename_all = "lowercase")]
pub(crate) enum BrowserApiResponse<T = ()> {
    Ok(T),
    Error { message: String },
}

impl BrowserApiResponse<()> {
    /// Create a successful response with no data.
    pub const fn ok() -> Self {
        Self::Ok(())
    }
}

impl<T> BrowserApiResponse<T> {
    /// Create a successful response with the given data.
    pub const fn with_data(data: T) -> Self {
        Self::Ok(data)
    }

    /// Create an error response with the given message.
    pub fn error(msg: impl Into<String>) -> Self {
        Self::Error { message: msg.into() }
    }
}

/// Represents a transaction request sent to the browser wallet.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserTransactionRequest<N: Network> {
    /// The unique identifier for the transaction.
    pub id: Uuid,
    /// The transaction request details.
    pub request: N::TransactionRequest,
}

/// Represents a transaction response sent from the browser wallet.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BrowserTransactionResponse {
    /// The unique identifier for the transaction, must match the request ID sent earlier.
    pub id: Uuid,
    /// The transaction hash if the transaction was successful.
    pub hash: Option<TxHash>,
    /// The error message if the transaction failed.
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SignType {
    /// Standard personal sign: `eth_sign` / `personal_sign`
    PersonalSign,
    /// EIP-712 typed data sign: `eth_signTypedData_v4`
    SignTypedDataV4,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignRequest {
    /// The message to be signed.
    pub message: String,
    /// The address that should sign the message.
    pub address: Address,
}

/// Represents a signing request sent to the browser wallet.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BrowserSignRequest {
    /// The unique identifier for the signing request.
    pub id: Uuid,
    /// The type of signing operation.
    pub sign_type: SignType,
    /// The sign request details.
    pub request: SignRequest,
}

/// Represents a typed data signing request sent to the browser wallet.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BrowserSignTypedDataRequest {
    /// The unique identifier for the signing request.
    pub id: Uuid,
    /// The address that should sign the typed data.
    pub address: Address,
    /// The typed data to be signed.
    pub typed_data: TypedData,
}

/// Represents a signing response sent from the browser wallet.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BrowserSignResponse {
    /// The unique identifier for the signing request, must match the request ID sent earlier.
    pub id: Uuid,
    /// The signature if the signing was successful.
    pub signature: Option<Bytes>,
    /// The error message if the signing failed.
    pub error: Option<String>,
}

/// Tempo `KeyAuthorization` signing request sent to the browser wallet.
///
/// The browser UI should display the human-readable contents of
/// [`Self::key_authorization`] (key id, expiry, limits, and allowed calls),
/// drive the WebAuthn / P256 / Secp256k1 ceremony for the precomputed
/// [`Self::digest`], and POST back the resulting RLP-encoded
/// `SignedKeyAuthorization` as a `0x`-prefixed hex string. T5
/// `KeyAuthorization` fields are rejected server-side until the bundled
/// browser wallet can display and forward them.
#[cfg(feature = "tempo")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BrowserKeyAuthorizationRequest {
    /// Unique id correlating request and response.
    pub id: Uuid,
    /// Root account that must sign the authorization. The wallet UI must
    /// cross-check this against the connected wallet address before signing.
    pub root_account: Address,
    /// The full unsigned `KeyAuthorization` payload. Sent so the UI can render a human-readable
    /// approval card.
    pub key_authorization: KeyAuthorization,
    /// keccak256 of `RLP(key_authorization)` — equal to
    /// `key_authorization.signature_hash()`. Foundry pre-computes it so the
    /// frontend doesn't need to import RLP.
    pub digest: B256,
}

/// Tempo `KeyAuthorization` signing response sent back from the browser
/// wallet. Exactly one of `signed_hex` and `error` must be set.
#[cfg(feature = "tempo")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BrowserKeyAuthorizationResponse {
    /// Must match the request id.
    pub id: Uuid,
    /// `0x`-prefixed RLP-encoded `SignedKeyAuthorization` produced by the
    /// wallet. Decoded server-side via the existing
    /// `tempo_primitives::transaction::SignedKeyAuthorization::decode` impl.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed_hex: Option<String>,
    /// Error message if signing was rejected or failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Represents an active connection to a browser wallet.
///
/// The wire format uses camelCase (`chainId`) to match the rest of the
/// browser-facing API (`BrowserSignRequest`, `SessionInfo`, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    /// The address of the connected wallet.
    pub address: Address,
    /// The chain ID of the connected wallet.
    pub chain_id: ChainId,
}

impl Connection {
    /// Create a new connection instance.
    pub const fn new(address: Address, chain_id: ChainId) -> Self {
        Self { address, chain_id }
    }
}

/// Information about the current browser wallet session, returned by
/// `GET /api/session`. Allows the webapp to detect when the underlying
/// `BrowserSigner` has been dropped (e.g. the script has finished) and
/// stop polling cleanly.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    /// Whether the server is still accepting new requests.
    /// Becomes `false` immediately before the server stops.
    pub alive: bool,
    /// Whether a wallet is currently connected to the server.
    pub connected: bool,
}
