//! State reads pinned to a resolved fork block.

use alloy_eips::{BlockId, BlockNumHash};
use alloy_primitives::{Address, Bytes, U64};
use alloy_provider::{
    Network, Provider,
    network::{BlockResponse, primitives::HeaderResponse},
};
use alloy_transport::TransportError;
use parking_lot::RwLock;
use serde::{Serialize, ser::SerializeStruct};
use std::{
    hash::{Hash, Hasher},
    sync::Arc,
};

/// Shared state access for a resolved fork, including reads made before backend construction.
///
/// Compatible reads try the hash first. If the RPC rejects the parameters, they retry the same
/// operation at the resolved number, checking its hash before and after. This detects observed
/// reorgs but cannot guarantee snapshot consistency across load-balanced RPC replicas. Once used,
/// number-based state must not be persisted as an exact snapshot.
#[derive(Clone, Debug)]
pub struct ForkState {
    block: BlockNumHash,
    exact: bool,
    pub(crate) status: Arc<RwLock<StateStatus>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StateStatus {
    Hash,
    HashOnly,
    Number,
    Invalid,
}

impl ForkState {
    /// Creates hash-preferred state access for a fork resolved from a number or tag.
    pub fn new(block: BlockNumHash) -> Self {
        Self { block, exact: false, status: Arc::new(RwLock::new(StateStatus::Hash)) }
    }

    /// Creates strictly hash-addressed state access, for replay and explicit hash selections.
    pub fn exact(block: BlockNumHash) -> Self {
        Self { exact: true, ..Self::new(block) }
    }

    /// Returns the immutable block identity.
    pub const fn block(&self) -> BlockNumHash {
        self.block
    }

    /// Whether state access forbids number-based retries.
    pub const fn is_exact(&self) -> bool {
        self.exact
    }

    /// Reads an account nonce using the same policy as the fork backend.
    pub async fn transaction_count<N: Network, P: Provider<N>>(
        &self,
        provider: &P,
        address: Address,
    ) -> eyre::Result<u64> {
        self.read(provider, |block| async move {
            let nonce: U64 = provider
                .raw_request("eth_getTransactionCount".into(), (address, StateBlockId(block)))
                .await?;
            Ok(nonce.to())
        })
        .await
    }

    /// Reads account code using the same policy as the fork backend.
    pub async fn code<N: Network, P: Provider<N>>(
        &self,
        provider: &P,
        address: Address,
    ) -> eyre::Result<Bytes> {
        self.read(provider, |block| async move {
            Ok(provider.raw_request("eth_getCode".into(), (address, StateBlockId(block))).await?)
        })
        .await
    }

    pub(crate) fn ensure_valid(&self) -> eyre::Result<()> {
        eyre::ensure!(
            *self.status.read() != StateStatus::Invalid,
            "fork state invalidated: resolved block changed during number-based state access"
        );
        Ok(())
    }

    pub(crate) fn require_hash_state(&self) -> eyre::Result<()> {
        let mut status = self.status.write();
        eyre::ensure!(
            matches!(*status, StateStatus::Hash | StateStatus::HashOnly),
            "transaction replay requires hash-addressed state; create a fresh transaction-targeted fork"
        );
        *status = StateStatus::HashOnly;
        Ok(())
    }

    pub(crate) async fn read<N, P, T, F, Fut>(&self, provider: &P, request: F) -> eyre::Result<T>
    where
        N: Network,
        P: Provider<N>,
        F: Fn(BlockId) -> Fut,
        Fut: Future<Output = eyre::Result<T>>,
    {
        self.ensure_valid()?;
        let result = request(self.block.hash.into()).await;
        self.ensure_valid()?;
        let original = match result {
            Ok(value) => return Ok(value),
            Err(err) => err,
        };
        // Invalid params is only a reason to try the equivalent numeric selector. It does not
        // establish endpoint-wide support, nor justify hiding an unsuccessful numeric retry.
        let invalid_params = original.downcast_ref::<TransportError>().is_some_and(|error| {
            // Some relays return invalid params with HTTP 400 and a null RPC id. Alloy keeps
            // those as HTTP errors; use its structured extraction, never the English message.
            let allowed_transport = error
                .as_transport_err()
                .is_none_or(|kind| kind.as_http_error().is_some_and(|http| http.status == 400));
            allowed_transport && error.error_code() == Some(-32602)
        });
        if self.exact || *self.status.read() == StateStatus::HashOnly || !invalid_params {
            return Err(original);
        }
        self.check_anchor(provider).await?;
        {
            let mut status = self.status.write();
            eyre::ensure!(*status != StateStatus::Invalid, "fork state was invalidated");
            if *status == StateStatus::HashOnly {
                return Err(original);
            }
            if *status == StateStatus::Hash {
                *status = StateStatus::Number;
                warn!(target: "fork", block = self.block.number,
                    "RPC rejected hash-addressed state; retrying by block number with reorg checks. Disk caching is disabled for this fork");
            }
        }
        let result = request(BlockId::number(self.block.number)).await;
        self.check_anchor(provider).await?;
        self.ensure_valid()?;
        result.map_err(|retry| {
            original.wrap_err(format!("number-based state retry also failed: {retry}"))
        })
    }

    async fn check_anchor<N: Network, P: Provider<N>>(&self, provider: &P) -> eyre::Result<()> {
        let block = provider.get_block_by_number(self.block.number.into()).await?;
        if block.is_none_or(|block| block.header().hash() != self.block.hash) {
            *self.status.write() = StateStatus::Invalid;
        }
        self.ensure_valid()
    }
}

// Mutable read status is shared by clones, but is never part of fork or cache identity.
impl PartialEq for ForkState {
    fn eq(&self, other: &Self) -> bool {
        self.block == other.block && self.exact == other.exact
    }
}

impl Eq for ForkState {}

impl Hash for ForkState {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.block.hash(state);
        self.exact.hash(state);
    }
}

/// Preserve EIP-1898 objects without the optional canonical flag. Bare hashes are ambiguous
/// on Moonbeam, while Rootstock rejects `requireCanonical`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct StateBlockId(pub(crate) BlockId);

impl Serialize for StateBlockId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let BlockId::Hash(hash) = self.0
            && hash.require_canonical.is_none()
        {
            let mut object = serializer.serialize_struct("StateBlockId", 1)?;
            object.serialize_field("blockHash", &hash.block_hash)?;
            object.end()
        } else {
            self.0.serialize(serializer)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ForkBlock, SharedBackend,
        cache::{BlockchainDb, BlockchainDbMeta},
    };
    use alloy_primitives::{B256, U256};
    use alloy_provider::{ProviderBuilder, network::AnyNetwork};
    use alloy_rpc_client::ClientBuilder;
    use alloy_transport::TransportErrorKind;
    use revm::{context::BlockEnv, database::DatabaseRef};
    use serde_json::{Value, json};
    use std::{
        sync::atomic::{AtomicBool, Ordering},
        thread::JoinHandle,
        time::Duration,
    };
    use tiny_http::{Response, Server};

    struct Rpc {
        endpoint: String,
        stopped: Arc<AtomicBool>,
        requests: Arc<RwLock<Vec<Value>>>,
        thread: Option<JoinHandle<()>>,
    }

    impl Rpc {
        fn new(
            error: Option<i64>,
            fail_number: bool,
            reorg_check: usize,
            http_status: u16,
        ) -> Self {
            let server = Server::http("127.0.0.1:0").unwrap();
            let endpoint = format!("http://{}", server.server_addr());
            let stopped = Arc::new(AtomicBool::new(false));
            let requests = Arc::new(RwLock::new(Vec::new()));
            let stop = stopped.clone();
            let recorded = requests.clone();
            let thread = std::thread::spawn(move || {
                let mut checks = 0;
                while !stop.load(Ordering::Relaxed) {
                    let Some(mut request) = server.recv_timeout(Duration::from_millis(10)).unwrap()
                    else {
                        continue;
                    };
                    let rpc: Value = serde_json::from_reader(request.as_reader()).unwrap();
                    let mut response = json!({"jsonrpc": "2.0", "id": rpc["id"]});
                    let method = rpc["method"].as_str().unwrap();
                    let mut status = 200;
                    if method == "eth_getBlockByNumber" {
                        checks += 1;
                        assert_eq!(rpc["params"][0], "0xa");
                        response["result"] = json!({
                            "hash": if checks == reorg_check { B256::ZERO } else { B256::with_last_byte(1) },
                            "parentHash": B256::ZERO, "sha3Uncles": B256::ZERO,
                            "miner": Address::ZERO, "stateRoot": B256::ZERO,
                            "transactionsRoot": B256::ZERO, "receiptsRoot": B256::ZERO,
                            "logsBloom": format!("0x{}", "00".repeat(256)),
                            "difficulty": "0x0", "number": "0xa", "gasLimit": "0x100000",
                            "gasUsed": "0x0", "timestamp": "0x1", "extraData": "0x",
                            "mixHash": B256::ZERO, "nonce": "0x0000000000000000",
                            "transactions": [], "uncles": []
                        });
                    } else if method == "eth_getAccountInfo" {
                        response["error"] = json!({"code": -32601, "message": "unsupported"});
                    } else {
                        let selector = rpc["params"].as_array().unwrap().last().unwrap();
                        if selector.is_object() {
                            assert_eq!(selector, &json!({"blockHash": B256::with_last_byte(1)}));
                        } else {
                            assert_eq!(selector, "0xa");
                        }
                        if let Some(code) = error.filter(|_| selector.is_object() || fail_number) {
                            response["error"] = json!({"code": code, "message": if selector.is_object() { "hash rejected" } else { "number rejected" }});
                            status = http_status;
                            if status == 400 {
                                response["id"] = Value::Null;
                            }
                        } else {
                            response["result"] =
                                json!(if method == "eth_getCode" { "0x00" } else { "0x2a" });
                        }
                    }
                    recorded.write().push(rpc);
                    request
                        .respond(
                            Response::from_string(response.to_string()).with_status_code(status),
                        )
                        .unwrap();
                }
            });
            Self { endpoint, stopped, requests, thread: Some(thread) }
        }

        fn provider(&self) -> impl Provider<AnyNetwork> + Clone + use<> {
            ProviderBuilder::new()
                .network::<AnyNetwork>()
                .connect_client(ClientBuilder::default().http(self.endpoint.parse().unwrap()))
        }
    }

    impl Drop for Rpc {
        fn drop(&mut self) {
            self.stopped.store(true, Ordering::Relaxed);
            self.thread.take().unwrap().join().unwrap();
        }
    }

    #[tokio::test]
    async fn fork_state_fallback_is_per_read_and_only_for_invalid_params() {
        for (error, fail_number, exact, status) in [
            (None, false, false, 200),
            (Some(-32602), false, false, 200),
            (Some(-32602), false, false, 400),
            (Some(-32000), false, false, 200),
            (Some(-32602), true, false, 200),
            (Some(-32602), false, true, 200),
        ] {
            let rpc = Rpc::new(error, fail_number, 0, status);
            let provider = rpc.provider();
            let block = BlockNumHash::new(10, B256::with_last_byte(1));
            let state = if exact { ForkState::exact(block) } else { ForkState::new(block) };
            let retry = error == Some(-32602) && !exact;
            for _ in 0..2 {
                let result = state.transaction_count(&provider, Address::ZERO).await;
                if error.is_none() || (retry && !fail_number) {
                    assert_eq!(result.unwrap(), 42);
                } else {
                    assert_eq!(
                        result
                            .unwrap_err()
                            .downcast_ref::<TransportError>()
                            .unwrap()
                            .as_error_resp()
                            .unwrap()
                            .message,
                        "hash rejected"
                    );
                }
            }
            let requests = rpc.requests.read();
            assert_eq!(
                requests.iter().filter(|r| r["method"] == "eth_getBlockByNumber").count(),
                if retry { 4 } else { 0 }
            );
            assert_eq!(requests.iter().filter(|r| r["params"][1].is_object()).count(), 2);
        }
    }

    #[tokio::test]
    async fn fork_state_does_not_retry_auth_or_rate_limit_transport_errors() {
        let rpc = Rpc::new(None, false, 0, 200);
        let provider = rpc.provider();
        for status in [401, 403, 429, 500] {
            let state = ForkState::new(BlockNumHash::new(10, B256::with_last_byte(1)));
            let result: eyre::Result<()> = state.read(&provider, |_| async move {
                Err(TransportErrorKind::http_error(status,
                    r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32602,"message":"rejected"}}"#.into()).into())
            }).await;
            assert!(result.is_err());
        }
        assert!(rpc.requests.read().is_empty());
    }

    #[tokio::test]
    async fn fork_state_replay_disallows_existing_and_future_fallbacks() {
        let rpc = Rpc::new(Some(-32602), false, 0, 200);
        let provider = rpc.provider();
        let block = BlockNumHash::new(10, B256::with_last_byte(1));
        let state = ForkState::new(block);
        let clone = state.clone();
        state.require_hash_state().unwrap();
        assert!(clone.code(&provider, Address::ZERO).await.is_err());
        assert_eq!(rpc.requests.read().len(), 1);
        let state = ForkState::new(block);
        state.code(&provider, Address::ZERO).await.unwrap();
        assert!(state.require_hash_state().is_err());
        // Runtime state never changes the immutable identity used in maps and caches.
        assert_eq!(state, ForkState::new(block));
    }

    #[tokio::test]
    async fn fork_state_reorg_invalidates_every_clone() {
        for check in [1, 2] {
            let rpc = Rpc::new(Some(-32602), false, check, 200);
            let provider = rpc.provider();
            let state = ForkState::new(BlockNumHash::new(10, B256::with_last_byte(1)));
            let clone = state.clone();
            assert!(state.code(&provider, Address::ZERO).await.is_err());
            assert_eq!(*clone.status.read(), StateStatus::Invalid);
            let requests = rpc.requests.read().len();
            assert!(clone.transaction_count(&provider, Address::ZERO).await.is_err());
            assert_eq!(rpc.requests.read().len(), requests);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fork_state_preflight_backend_and_all_flushes_share_provenance() {
        for (fallback, preflight) in [(false, true), (true, true), (true, false)] {
            let rpc = Rpc::new(fallback.then_some(-32602), false, 0, 200);
            let provider = rpc.provider();
            let hash = B256::with_last_byte(1);
            let state = ForkState::new(BlockNumHash::new(10, hash));
            // Preflight happens before a database exists.
            if preflight {
                assert_eq!(state.transaction_count(&provider, Address::ZERO).await.unwrap(), 42);
            }
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("cache.json");
            let explicit_path = dir.path().join("explicit.json");
            let meta = BlockchainDbMeta::new(BlockEnv::default(), rpc.endpoint.clone())
                .with_fork_identity(hash, B256::with_last_byte(2));
            let db = BlockchainDb::new(meta, Some(path.clone()));
            let cache = db.cache().clone();
            // EVM and RPC block numbers can differ. State must always use the RPC number.
            let (backend, handler) = SharedBackend::new_with_state(
                provider,
                db,
                ForkBlock::with_rpc_number(1000, 10, hash),
                state,
            )
            .unwrap();
            let task = tokio::spawn(handler);
            assert_eq!(backend.basic_ref(Address::ZERO).unwrap().unwrap().balance, U256::from(42));
            assert_eq!(backend.storage_ref(Address::ZERO, U256::ZERO).unwrap(), U256::from(42));
            backend.flush_cache();
            cache.flush_to(&explicit_path);
            drop(backend);
            task.await.unwrap();
            assert_eq!(path.exists(), !fallback);
            assert_eq!(explicit_path.exists(), !fallback);
        }
    }
}
