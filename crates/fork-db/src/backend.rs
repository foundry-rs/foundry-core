//! Smart caching and deduplication of requests when using a forking provider.

use crate::{
    AccountInfo,
    cache::{
        AccountFetchPolicy, BlockchainDb, FlushJsonBlockCacheDB, ForkBlockEnv, JsonBlockCacheDB,
        MemDb, StorageInfo,
    },
    error::{DatabaseError, DatabaseResult},
};
use alloy_chains::Chain;
use alloy_consensus::BlockHeader;
use alloy_json_rpc::{Id, RequestPacket, ResponsePacket, ResponsePayload};
use alloy_primitives::{
    Address, B256, Bytes, KECCAK256_EMPTY as KECCAK_EMPTY, Selector, U64, U256, keccak256,
    map::{AddressHashMap, HashMap, HashSet, U256Map, hash_map::Entry},
};
use alloy_provider::{
    DynProvider, Network, Provider,
    network::{AnyNetwork, BlockResponse, primitives::HeaderResponse},
};
use alloy_rpc_types::{AccessListResult, BlockId, TransactionRequest, state::StateOverride};
use eyre::WrapErr;
use futures::{
    FutureExt,
    channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded},
    pin_mut,
    stream::Stream,
    task::{Context, Poll},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    fmt,
    path::Path,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
        mpsc::{Sender as OneshotSender, channel as oneshot_channel},
    },
    time::Duration,
};
use tokio::select;
use tower::Service;

/// A [`BlockId`] that serializes as an RPC block parameter for state reads.
///
/// A hash without a canonicality requirement is sent as `{"blockHash": "0x…"}` instead of
/// alloy's bare hash string. Moonbeam parses bare hashes as block numbers, while Rootstock rejects
/// a boolean `requireCanonical`; the object with the optional flag omitted supports both. Other
/// selectors serialize unchanged.
#[derive(Clone, Copy, Debug)]
pub struct StateBlockId(pub BlockId);

/// Largest number of new keys, accounts plus slots that were neither cached nor requested, in an
/// access list for which the call is not simulated again: reading them directly takes no more
/// round trips than the simulation and the prefetch together.
const CHEAP_ACCESS_LIST: usize = 2;

/// Reads since the last simulation before the cache can count as warm.
const WARM_CACHE_READS: u64 = 32;

/// Cache hits per miss since the last simulation above which the cache counts as warm, so a few
/// new keys are read directly instead of starting a simulation.
const WARM_HITS_PER_MISS: u64 = 32;

/// Warm reads without a simulation after which the counts restart, so the hits of a long warm run
/// do not hide new cold state.
const WARM_WINDOW: u64 = 1024;

/// Slots per batch request of prefetched reads. Endpoints commonly cap batches at 100 calls.
const PREFETCH_BATCH_SIZE: usize = 64;

/// Account requests of prefetched reads in flight at once; see [`MAX_PREFETCH_BATCHES`].
const MAX_PREFETCH_ACCOUNTS: usize = 8;

/// Batch requests of prefetched reads in flight at once. The fork backend sends the EVM's own
/// reads without a limit; this bound keeps a large access list from flooding the endpoint.
const MAX_PREFETCH_BATCHES: usize = 2;

/// Longest wait for a batch of prefetched reads. A rate-limited endpoint answers with errors
/// that the transport retries with growing pauses, and reads that joined the batch would wait
/// through them, so a slow batch counts as failed and those reads get their own requests.
const PREFETCH_BATCH_TIMEOUT: Duration = Duration::from_secs(3);

/// Whether an RPC error says the endpoint rejected the parameters.
fn is_invalid_params(err: &eyre::Report) -> bool {
    let msg = err.to_string().to_ascii_lowercase();
    msg.contains("32602")
        || msg.contains("invalid params")
        || msg.contains("invalid argument")
        || msg.contains("too many arguments")
}

/// Whether an RPC error says the endpoint does not serve the method.
fn is_unsupported_method(err: &eyre::Report) -> bool {
    let msg = err.to_string().to_ascii_lowercase();
    msg.contains("32601")
        || msg.contains("method not found")
        || msg.contains("not supported")
        || msg.contains("unsupported method")
        || msg.contains("does not exist")
}

impl Serialize for StateBlockId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let BlockId::Hash(hash) = self.0
            && hash.require_canonical.is_none()
        {
            use serde::ser::SerializeStruct;
            let mut object = serializer.serialize_struct("StateBlockId", 1)?;
            object.serialize_field("blockHash", &hash.block_hash)?;
            object.end()
        } else {
            self.0.serialize(serializer)
        }
    }
}

/// An account as an `eth_getAccountInfo` response holds it, including the chain-specific
/// `extension` that some nodes, such as Tempo's, add when it is nonempty.
#[derive(Debug, Deserialize)]
struct RemoteAccount {
    balance: U256,
    nonce: U64,
    code: Bytes,
    #[serde(default)]
    extension: Bytes,
}

/// Logged when an error is indicative that the user is trying to fork from a non-archive node.
pub const NON_ARCHIVE_NODE_WARNING: &str = "\
It looks like you're trying to fork from an older block with a non-archive node which is not \
supported. Please try to change your RPC url to an archive node if the issue persists.";

// Various future/request type aliases

type AccountFuture<Err> =
    Pin<Box<dyn Future<Output = (Result<RemoteAccount, Err>, Address, u64)> + Send>>;
type StorageFuture<Err> =
    Pin<Box<dyn Future<Output = (Result<U256, Err>, Address, U256, u64)> + Send>>;
type BlockHashFuture<Err> =
    Pin<Box<dyn Future<Output = (Result<BlockHashData, Err>, u64, u64, bool)> + Send>>;
type FullBlockFuture<Err, N = AnyNetwork> = Pin<
    Box<
        dyn Future<
                Output = (
                    FullBlockSender<N>,
                    Result<Option<<N as Network>::BlockResponse>, Err>,
                    BlockId,
                ),
            > + Send,
    >,
>;
type TransactionFuture<Err, N = AnyNetwork> = Pin<
    Box<
        dyn Future<
                Output = (
                    TransactionSender<N>,
                    Result<<N as Network>::TransactionResponse, Err>,
                    B256,
                ),
            > + Send,
    >,
>;

/// A simulated call, keyed by its target, selector and the hash of the code the target runs.
type AccessListKey = (Address, Selector, B256);

/// Returns the key of a call to `tx.to` running `code_hash`, or `None` for a create.
fn access_list_key(tx: &TransactionRequest, code_hash: B256) -> Option<AccessListKey> {
    let to = *tx.to?.to()?;
    let selector = tx
        .input
        .input()
        .and_then(|input| Selector::try_from(input.get(..4)?).ok())
        .unwrap_or_default();
    Some((to, selector, code_hash))
}

type AccessListFuture<Err> =
    Pin<Box<dyn Future<Output = (Result<AccessListResult, Err>, AccessListKey, u64, bool)> + Send>>;

/// One batch of prefetched slot reads: the value or error of each slot, with the state generation
/// and the block the batch read at.
type StorageBatchFuture<Err> =
    Pin<Box<dyn Future<Output = (Vec<(Address, U256, Result<U256, Err>)>, u64, BlockId)> + Send>>;

type AccountInfoSender = OneshotSender<DatabaseResult<AccountInfo>>;
type StorageSender = OneshotSender<DatabaseResult<U256>>;
type BlockHashSender = OneshotSender<DatabaseResult<B256>>;
type FullBlockSender<N = AnyNetwork> = OneshotSender<DatabaseResult<<N as Network>::BlockResponse>>;
type TransactionSender<N = AnyNetwork> =
    OneshotSender<DatabaseResult<<N as Network>::TransactionResponse>>;

type AddressData = AddressHashMap<AccountInfo>;
type StorageData = AddressHashMap<StorageInfo>;
type BlockHashData = U256Map<B256>;

/// Which endpoints the backend uses to fetch account info under [`AccountFetchPolicy::Auto`].
///
/// Transitions, all driven by the outcome of account requests:
///
/// ```text
/// Unchecked --eth_getAccountInfo ok--------------------> SupportsAccountInfo
/// Unchecked --eth_getAccountInfo err / triple first----> SeparateRequests
/// SupportsAccountInfo --eth_getAccountInfo err---------> SeparateRequests
/// ```
///
/// Nothing moves back to `Unchecked`. Stores are unconditional, so concurrent `Unchecked`
/// probes may overwrite each other's outcome; the last one to finish wins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum AccountFetchMode {
    /// Not probed yet: race the non standard `eth_getAccountInfo` against the separate requests
    /// and settle on whichever answers.
    Unchecked = 0,
    /// The endpoint supports the non standard `eth_getAccountInfo`, which is more efficient than
    /// sending 3 separate requests.
    SupportsAccountInfo = 1,
    /// Use regular individual `getCode`, `getNonce`, `getBalance` calls.
    SeparateRequests = 2,
}

/// An [`AccountFetchMode`] shared between all in-flight account requests.
#[derive(Debug)]
struct SharedAccountFetchMode(AtomicU8);

impl SharedAccountFetchMode {
    const fn new(mode: AccountFetchMode) -> Self {
        Self(AtomicU8::new(mode as u8))
    }

    fn load(&self) -> AccountFetchMode {
        match self.0.load(Ordering::Relaxed) {
            0 => AccountFetchMode::Unchecked,
            1 => AccountFetchMode::SupportsAccountInfo,
            2 => AccountFetchMode::SeparateRequests,
            _ => unreachable!("Invalid account fetch mode"),
        }
    }

    fn store(&self, mode: AccountFetchMode) {
        self.0.store(mode as u8, Ordering::Relaxed);
    }
}

/// Whether access list simulations are wanted, shared by a handler and its [`SharedBackend`]s so
/// a caller can skip building a call that the handler would drop.
#[derive(Debug)]
struct AccessListGate {
    /// Cleared when the endpoint lacks `eth_createAccessList` or a prefetch batch fails.
    supported: AtomicBool,
    /// Cleared when the endpoint rejects state overrides in `eth_createAccessList`.
    overrides_supported: AtomicBool,
    /// Cache hits when the last simulation was requested, `u64::MAX` before the first.
    hits: AtomicU64,
    /// Cache misses when the last simulation was requested.
    misses: AtomicU64,
    /// Whether the cache was warm at the last check.
    was_warm: AtomicBool,
    /// Calls whose last access list held at most [`CHEAP_ACCESS_LIST`] new keys.
    cheap: parking_lot::Mutex<HashSet<AccessListKey>>,
    /// Calls simulated at the pinned block.
    requested: parking_lot::Mutex<HashSet<AccessListKey>>,
}

impl Default for AccessListGate {
    fn default() -> Self {
        Self {
            supported: AtomicBool::new(true),
            overrides_supported: AtomicBool::new(true),
            hits: AtomicU64::new(u64::MAX),
            misses: AtomicU64::new(0),
            was_warm: AtomicBool::new(false),
            cheap: Default::default(),
            requested: Default::default(),
        }
    }
}

impl AccessListGate {
    /// Returns whether a call, with or without `overrides`, is simulated now: the endpoint
    /// supports it, the call is not cheap, and the cache is not warm. Suites on one fork share
    /// the counts, so a call into a target the fork has not loaded, such as a new suite's test
    /// contract, is simulated once whatever the other reads hit.
    fn wanted<B>(&self, key: &AccessListKey, overrides: bool, cache: &JsonBlockCacheDB<B>) -> bool {
        self.supported.load(Ordering::Relaxed)
            && (!overrides || self.overrides_supported.load(Ordering::Relaxed))
            // The warm check runs first, so it rolls the window and clears cheap calls.
            && (!self.warm(cache)
                || (!cache.db().accounts.read().contains_key(&key.0)
                    && !self.requested.lock().contains(key)))
            && !self.cheap.lock().contains(key)
    }

    /// Returns whether the cache is warm: [`WARM_CACHE_READS`] reads since the last simulation,
    /// with at most one miss per [`WARM_HITS_PER_MISS`] hits.
    ///
    /// After [`WARM_WINDOW`] warm reads the counts restart from the last [`WARM_CACHE_READS`]
    /// hits, so the hits of a long warm run do not hide new cold state. When the cache turns
    /// cold, the cheap calls are cleared, since their arguments can now reach cold state.
    fn warm<B>(&self, cache: &JsonBlockCacheDB<B>) -> bool {
        let Some((hits, misses)) = self.reads_since_simulation(cache) else { return false };
        let warm = hits >= WARM_CACHE_READS && misses.saturating_mul(WARM_HITS_PER_MISS) <= hits;
        if warm && hits + misses >= WARM_WINDOW {
            self.misses.store(cache.cache_misses(), Ordering::Relaxed);
            self.hits.store(cache.cache_hits() - WARM_CACHE_READS, Ordering::Relaxed);
        }
        if self.was_warm.swap(warm, Ordering::Relaxed) && !warm {
            self.cheap.lock().clear();
        }
        warm
    }

    /// Records the cache counts of a simulation request.
    fn record<B>(&self, cache: &JsonBlockCacheDB<B>) {
        self.misses.store(cache.cache_misses(), Ordering::Relaxed);
        self.hits.store(cache.cache_hits(), Ordering::Relaxed);
    }

    /// Returns the cache hits and misses since the last simulation request.
    fn reads_since_simulation<B>(&self, cache: &JsonBlockCacheDB<B>) -> Option<(u64, u64)> {
        let hits = self.hits.load(Ordering::Relaxed);
        (hits != u64::MAX).then(|| {
            (
                cache.cache_hits().saturating_sub(hits),
                cache.cache_misses().saturating_sub(self.misses.load(Ordering::Relaxed)),
            )
        })
    }
}

struct AnyRequestFuture<T, Err> {
    sender: OneshotSender<Result<T, Err>>,
    future: Pin<Box<dyn Future<Output = Result<T, Err>> + Send>>,
}

impl<T, Err> fmt::Debug for AnyRequestFuture<T, Err> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("AnyRequestFuture").field(&self.sender).finish()
    }
}

trait WrappedAnyRequest: Unpin + Send + fmt::Debug {
    fn poll_inner(&mut self, cx: &mut Context<'_>) -> Poll<()>;
}

/// @dev Implements `WrappedAnyRequest` for `AnyRequestFuture`.
///
/// - `poll_inner` is similar to `Future` polling but intentionally consumes the Future<Output=T>
///   and return Future<Output=()>
/// - This design avoids storing `Future<Output = T>` directly, as its type may not be known at
///   compile time.
/// - Instead, the result (`Result<T, Err>`) is sent via the `sender` channel, which enforces type
///   safety.
impl<T, Err> WrappedAnyRequest for AnyRequestFuture<T, Err>
where
    T: fmt::Debug + Send + 'static,
    Err: fmt::Debug + Send + 'static,
{
    fn poll_inner(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        match self.future.poll_unpin(cx) {
            Poll::Ready(result) => {
                let _ = self.sender.send(result);
                Poll::Ready(())
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Request variants that are executed by the provider
enum ProviderRequest<Err, N: Network = AnyNetwork> {
    Account(AccountFuture<Err>),
    Storage(StorageFuture<Err>),
    BlockHash(BlockHashFuture<Err>),
    FullBlock(FullBlockFuture<Err, N>),
    Transaction(TransactionFuture<Err, N>),
    AccessList(AccessListFuture<Err>),
    StorageBatch(StorageBatchFuture<Err>),
    AnyRequest(Box<dyn WrappedAnyRequest>),
}

/// The Request type the Backend listens for
#[derive(Debug)]
enum BackendRequest<N: Network = AnyNetwork> {
    /// Fetch the account info, as a prefetch if the flag is set
    Basic(Address, AccountInfoSender, bool),
    /// Fetch a storage slot, as a prefetch if the flag is set
    Storage(Address, U256, StorageSender, bool),
    /// Fetch a block hash
    BlockHash(u64, BlockHashSender),
    /// Fetch an entire block with transactions
    FullBlock(BlockId, FullBlockSender<N>),
    /// Fetch a transaction
    Transaction(B256, TransactionSender<N>),
    /// Simulate a call and prefetch the accounts and slots it touches
    PrefetchAccessList(Box<(TransactionRequest, Option<StateOverride>, B256)>),
    /// Sets the pinned block to fetch data from
    SetPinnedBlock(BlockId),
    /// Removes an account, or one of its storage slots, from the cache and drops prefetched values
    /// and in-flight prefetches
    Evict(Address, Option<U256>),

    /// Update Address data
    UpdateAddress(AddressData),
    /// Update Storage data
    UpdateStorage(StorageData),
    /// Update Block Hashes
    UpdateBlockHash(BlockHashData),
    /// Any other request
    AnyRequest(Box<dyn WrappedAnyRequest>),
}

/// Handles an internal provider and listens for requests.
///
/// This handler will remain active as long as it is reachable (request channel still open) and
/// requests are in progress.
#[must_use = "futures do nothing unless polled"]
pub struct BackendHandler<N: Network = AnyNetwork, B = serde_json::Value> {
    provider: DynProvider<N>,
    /// Stores all the data.
    db: BlockchainDb<B>,
    /// Requests currently in progress
    pending_requests: Vec<ProviderRequest<eyre::Report, N>>,
    /// Listeners that wait for a `get_account` related response, keyed by state generation and
    /// address, and whether all are prefetches.
    account_requests: HashMap<(u64, Address), (Vec<AccountInfoSender>, bool)>,
    /// Listeners that wait for a `get_storage_at` response, keyed by state generation and slot,
    /// and whether all are prefetches.
    storage_requests: HashMap<(u64, Address, U256), (Vec<StorageSender>, bool)>,
    /// Listeners that wait for a `get_block` response, keyed by pin generation and block number.
    block_requests: HashMap<(u64, u64), Vec<BlockHashSender>>,
    /// Block hashes whose value depends on the pinned block.
    pinned_block_hashes: HashMap<(u64, u64), B256>,
    /// Incoming commands.
    incoming: UnboundedReceiver<BackendRequest<N>>,
    /// unprocessed queued requests
    queued_requests: VecDeque<BackendRequest<N>>,
    /// The block to fetch data from.
    // This is an `Option` so that we can have less code churn in the functions below
    block_id: Option<BlockId>,
    /// Exact block anchoring state reads and block ancestry.
    block_anchor: Option<ForkBlock>,
    /// Incremented whenever the pinned block changes. Keys block hashes.
    block_generation: u64,
    /// Incremented whenever the pinned block changes or prefetched state is dropped. Keys account,
    /// storage and access list requests.
    state_generation: u64,
    /// Whether block hashes are resolved in the remote EVM.
    block_hash_via_evm: Option<bool>,
    /// The mode for fetching account data
    account_fetch_mode: Arc<SharedAccountFetchMode>,
    /// Account-loading policy fixed when this backend is created.
    account_fetch_policy: AccountFetchPolicy,
    /// Prefetched accounts that no read has requested yet. They stay out of `db`, so the cache
    /// file only holds values that were read.
    prefetched_accounts: AddressHashMap<AccountInfo>,
    /// Prefetched storage slots that no read has requested yet, see `prefetched_accounts`.
    prefetched_storage: HashMap<(Address, U256), U256>,
    /// Access list simulations in flight, keyed by state generation, target and selector.
    access_list_requests: HashSet<(u64, AccessListKey)>,
    /// Whether simulations are wanted, shared with the [`SharedBackend`]s.
    access_list_gate: Arc<AccessListGate>,
    /// Accounts of access lists waiting for a request, keyed by state generation.
    account_queue: VecDeque<(u64, Address)>,
    /// Slots of access lists waiting for a request, keyed by state generation.
    prefetch_queue: VecDeque<(u64, Address, U256)>,
    /// Batch requests from the queue in flight.
    prefetch_batches: usize,
}

impl<N: Network, B: ForkBlockEnv> BackendHandler<N, B> {
    fn new(
        provider: DynProvider<N>,
        db: BlockchainDb<B>,
        rx: UnboundedReceiver<BackendRequest<N>>,
        block_id: Option<BlockId>,
        block_anchor: Option<ForkBlock>,
        access_list_gate: Arc<AccessListGate>,
    ) -> Self {
        let block_hash_via_evm = db.meta().read().chain.and_then(|chain| {
            if chain.is_arbitrum() {
                Some(true)
            } else if chain.is_named() {
                Some(false)
            } else {
                None
            }
        });
        let account_fetch_policy = db.meta().read().account_fetch_policy;
        Self {
            provider,
            db,
            account_fetch_policy,
            pending_requests: Default::default(),
            account_requests: Default::default(),
            storage_requests: Default::default(),
            block_requests: Default::default(),
            pinned_block_hashes: Default::default(),
            queued_requests: Default::default(),
            incoming: rx,
            block_id,
            block_anchor,
            block_generation: 0,
            state_generation: 0,
            block_hash_via_evm,
            account_fetch_mode: Arc::new(SharedAccountFetchMode::new(AccountFetchMode::Unchecked)),
            prefetched_accounts: Default::default(),
            prefetched_storage: Default::default(),
            access_list_requests: Default::default(),
            access_list_gate,
            account_queue: Default::default(),
            prefetch_queue: Default::default(),
            prefetch_batches: 0,
        }
    }

    /// handle the request in queue in the future.
    ///
    /// We always check:
    ///  1. if the requested value is already stored in the cache, then answer the sender
    ///  2. otherwise, fetch it via the provider but check if a request for that value is already in
    ///     progress (e.g. another Sender just requested the same account)
    fn on_request(&mut self, req: BackendRequest<N>) {
        match req {
            BackendRequest::Basic(addr, sender, prefetch) => {
                trace!(target: "backendhandler", "received request basic address={:?}", addr);
                let mut acc = self.db.accounts().read().get(&addr).cloned();
                if acc.is_none() {
                    acc = if prefetch {
                        self.prefetched_accounts.get(&addr).cloned()
                    } else {
                        // A read moves a prefetched account into the cache before the reply.
                        self.prefetched_accounts.remove(&addr).inspect(|acc| {
                            self.db.accounts().write().insert(addr, acc.clone());
                        })
                    };
                }
                if let Some(basic) = acc {
                    self.db.cache().record_cache_hit();
                    let _ = sender.send(Ok(basic));
                } else {
                    self.request_account(addr, sender, prefetch);
                }
            }
            BackendRequest::BlockHash(number, sender) => {
                if self.block_anchor.is_some_and(|anchor| number > anchor.number) {
                    let _ = sender.send(Ok(B256::ZERO));
                    return;
                }
                let hash = self
                    .pinned_block_hashes
                    .get(&(self.block_generation, number))
                    .copied()
                    .or_else(|| self.db.block_hashes().read().get(&U256::from(number)).copied());
                if let Some(hash) = hash {
                    self.db.cache().record_cache_hit();
                    let _ = sender.send(Ok(hash));
                } else {
                    self.request_hash(number, sender);
                }
            }
            BackendRequest::FullBlock(number, sender) => {
                self.request_full_block(number, sender);
            }
            BackendRequest::Transaction(tx, sender) => {
                self.request_transaction(tx, sender);
            }
            BackendRequest::Storage(addr, idx, sender, prefetch) => {
                // account is already stored in the cache
                let mut value =
                    self.db.storage().read().get(&addr).and_then(|acc| acc.get(&idx).copied());
                if value.is_none() {
                    value = if prefetch {
                        self.prefetched_storage.get(&(addr, idx)).copied()
                    } else {
                        // A read moves a prefetched slot into the cache before the reply.
                        self.prefetched_storage.remove(&(addr, idx)).inspect(|value| {
                            self.db.storage().write().entry(addr).or_default().insert(idx, *value);
                        })
                    };
                }
                if let Some(value) = value {
                    self.db.cache().record_cache_hit();
                    let _ = sender.send(Ok(value));
                } else {
                    // account present but not storage -> fetch storage
                    self.request_account_storage(addr, idx, sender, prefetch);
                }
            }
            BackendRequest::SetPinnedBlock(block_id) => {
                self.drop_prefetched();
                // Calls of the old block can reach cold state at the new one.
                self.access_list_gate.cheap.lock().clear();
                self.access_list_gate.requested.lock().clear();
                self.block_id = Some(block_id);
                self.block_anchor = None;
                self.block_generation = self.block_generation.wrapping_add(1);
            }
            BackendRequest::Evict(address, slot) => {
                // In the same step, so a response already received for the old generation cannot
                // put the old value back before the next read.
                self.drop_prefetched();
                if let Some(slot) = slot {
                    if let Some(storage) = self.db.storage().write().get_mut(&address) {
                        storage.remove(&slot);
                    }
                } else {
                    self.db.accounts().write().remove(&address);
                }
            }
            BackendRequest::UpdateAddress(address_data) => {
                for (address, data) in address_data {
                    self.db.accounts().write().insert(address, data);
                }
            }
            BackendRequest::UpdateStorage(storage_data) => {
                for (address, data) in storage_data {
                    self.db.storage().write().insert(address, data);
                }
            }
            BackendRequest::UpdateBlockHash(block_hash_data) => {
                for (block, hash) in block_hash_data {
                    self.db.block_hashes().write().insert(block, hash);
                }
            }
            BackendRequest::PrefetchAccessList(call) => {
                let (tx, overrides, code_hash) = *call;
                self.request_access_list(tx, overrides, code_hash);
            }
            BackendRequest::AnyRequest(fut) => {
                self.pending_requests.push(ProviderRequest::AnyRequest(fut));
            }
        }
    }

    /// Simulates a call with `eth_createAccessList` at the pinned block and prefetches the
    /// accounts and slots in the result.
    ///
    /// The simulation is skipped while it is not wanted, see [`AccessListGate::wanted`]. One
    /// simulation per call is in flight at a time.
    fn request_access_list(
        &mut self,
        tx: TransactionRequest,
        overrides: Option<StateOverride>,
        code_hash: B256,
    ) {
        let Some(key) = access_list_key(&tx, code_hash) else { return };
        let (to, selector, _) = key;
        let cache = self.db.cache();
        if !self.access_list_gate.wanted(&key, overrides.is_some(), cache) {
            trace!(target: "backendhandler", %to, %selector, "skipping access list request");
            return;
        }
        let generation = self.state_generation;
        if !self.access_list_requests.insert((generation, key)) {
            return;
        }
        self.access_list_gate.requested.lock().insert(key);
        self.access_list_gate.record(cache);
        let with_overrides = overrides.is_some();
        trace!(target: "backendhandler", %to, %selector, with_overrides, "preparing access list request");
        let provider = self.provider.clone();
        let block_id = self.block_id.unwrap_or_default();
        let fut = Box::pin(async move {
            let mut params = vec![serde_json::json!(tx), serde_json::json!(StateBlockId(block_id))];
            if let Some(overrides) = overrides {
                params.push(serde_json::json!(overrides));
            }
            let result = provider
                .raw_request::<_, AccessListResult>("eth_createAccessList".into(), params)
                .await
                .map_err(Into::into);
            (result, key, generation, with_overrides)
        });
        self.pending_requests.push(ProviderRequest::AccessList(fut));
    }

    /// Queues the accounts and slots of an access list for [`Self::drain_account_queue`] and
    /// [`Self::drain_prefetch_queue`].
    fn prefetch_access_list(&mut self, result: &AccessListResult) {
        let generation = self.state_generation;
        for item in &result.access_list.0 {
            self.account_queue.push_back((generation, item.address));
            for slot in &item.storage_keys {
                self.prefetch_queue.push_back((
                    generation,
                    item.address,
                    U256::from_be_bytes(slot.0),
                ));
            }
        }
        self.drain_account_queue();
        self.drain_prefetch_queue();
    }

    /// Returns the number of accounts and slots of an access list that are neither cached nor
    /// requested.
    fn new_keys(&self, result: &AccessListResult) -> usize {
        let generation = self.state_generation;
        let accounts = self.db.accounts().read();
        let storage = self.db.storage().read();
        let mut new_keys = 0;
        for item in &result.access_list.0 {
            let address = item.address;
            if !accounts.contains_key(&address)
                && !self.prefetched_accounts.contains_key(&address)
                && !self.account_requests.contains_key(&(generation, address))
            {
                new_keys += 1;
            }
            for slot in &item.storage_keys {
                let idx = U256::from_be_bytes(slot.0);
                if !storage.get(&address).is_some_and(|slots| slots.contains_key(&idx))
                    && !self.prefetched_storage.contains_key(&(address, idx))
                    && !self.storage_requests.contains_key(&(generation, address, idx))
                {
                    new_keys += 1;
                }
            }
        }
        new_keys
    }

    /// Drops prefetched values and queued prefetches, and starts a new state generation so that
    /// in-flight account and storage requests answer only their own listeners and are not cached
    /// or joined. Block hashes stay cached.
    fn drop_prefetched(&mut self) {
        self.prefetched_accounts.clear();
        self.prefetched_storage.clear();
        self.account_queue.clear();
        self.prefetch_queue.clear();
        self.state_generation = self.state_generation.wrapping_add(1);
    }

    /// Requests queued accounts until [`MAX_PREFETCH_ACCOUNTS`] prefetch-only requests are in
    /// flight. An account that a read fetched or requested in the meantime is dropped, so a read
    /// never waits on the queue.
    fn drain_account_queue(&mut self) {
        if self.account_queue.is_empty() {
            return;
        }
        let generation = self.state_generation;
        let mut in_flight =
            self.account_requests.values().filter(|(_, prefetch_only)| *prefetch_only).count();
        while in_flight < MAX_PREFETCH_ACCOUNTS {
            let Some((account_generation, address)) = self.account_queue.pop_front() else {
                return;
            };
            if account_generation != generation
                || self.account_requests.contains_key(&(generation, address))
                || self.prefetched_accounts.contains_key(&address)
                || self.db.accounts().read().contains_key(&address)
            {
                continue;
            }
            let (sender, _) = oneshot_channel();
            self.request_account(address, sender, true);
            in_flight += 1;
        }
    }

    /// Sends queued slots in batches of [`PREFETCH_BATCH_SIZE`] until [`MAX_PREFETCH_BATCHES`]
    /// are in flight. A slot that a read fetched or requested in the meantime is dropped.
    fn drain_prefetch_queue(&mut self) {
        while self.prefetch_batches < MAX_PREFETCH_BATCHES {
            let generation = self.state_generation;
            let mut slots = Vec::new();
            while slots.len() < PREFETCH_BATCH_SIZE {
                let Some((slot_generation, address, idx)) = self.prefetch_queue.pop_front() else {
                    break;
                };
                if slot_generation != generation
                    || self.storage_requests.contains_key(&(generation, address, idx))
                    || self.prefetched_storage.contains_key(&(address, idx))
                    || self
                        .db
                        .storage()
                        .read()
                        .get(&address)
                        .is_some_and(|slots| slots.contains_key(&idx))
                {
                    continue;
                }
                self.db.cache().record_cache_miss();
                self.storage_requests.insert((generation, address, idx), (Vec::new(), true));
                slots.push((address, idx));
            }
            if slots.is_empty() {
                return;
            }
            self.prefetch_batches += 1;
            self.request_storage_batch(slots, generation);
        }
    }

    /// Reads `slots` with one `eth_getStorageAt` batch request.
    fn request_storage_batch(&mut self, slots: Vec<(Address, U256)>, generation: u64) {
        trace!(target: "backendhandler", slots = slots.len(), "preparing storage batch request");
        let client = self.provider.client();
        let block_id = StateBlockId(self.block_id.unwrap_or_default());
        let requests = slots
            .iter()
            .map(|&(address, idx)| {
                client.make_request("eth_getStorageAt", (address, idx, block_id)).serialize()
            })
            .collect::<Result<Vec<_>, _>>();
        let mut transport = client.transport().clone();
        let fut = Box::pin(async move {
            let result = async {
                let requests = requests?;
                let ids = requests.iter().map(|request| request.id().clone()).collect::<Vec<_>>();
                // A transport layer such as a concurrency limit hands out permits in `poll_ready`.
                let response = tokio::time::timeout(PREFETCH_BATCH_TIMEOUT, async {
                    std::future::poll_fn(|cx| transport.poll_ready(cx)).await?;
                    transport.call(RequestPacket::Batch(requests)).await
                })
                .await
                .map_err(|_| eyre::eyre!("timed out after {PREFETCH_BATCH_TIMEOUT:?}"))??;
                let ResponsePacket::Batch(responses) = response else {
                    eyre::bail!("expected a batch response");
                };
                let mut responses = responses
                    .into_iter()
                    .map(|response| (response.id, response.payload))
                    .collect::<HashMap<Id, _>>();
                eyre::Ok(
                    ids.into_iter()
                        .map(|id| match responses.remove(&id) {
                            Some(ResponsePayload::Success(raw)) => {
                                serde_json::from_str::<U256>(raw.get()).map_err(Into::into)
                            }
                            Some(ResponsePayload::Failure(err)) => Err(eyre::eyre!("{err}")),
                            None => Err(eyre::eyre!("missing response")),
                        })
                        .collect::<Vec<_>>(),
                )
            }
            .await;
            // A failed transport counts as a failure of every member.
            let values = result
                .unwrap_or_else(|err| slots.iter().map(|_| Err(eyre::eyre!("{err}"))).collect());
            let values =
                slots.into_iter().zip(values).map(|((address, idx), value)| (address, idx, value));
            (values.collect(), generation, block_id.0)
        });
        self.pending_requests.push(ProviderRequest::StorageBatch(fut));
    }

    /// Reads a slot of a failed batch on its own at the block the batch read at, when a read waits
    /// for it. A slot that no read asked for is dropped.
    fn retry_storage_read(
        &mut self,
        address: Address,
        idx: U256,
        block_id: BlockId,
        generation: u64,
        err: &eyre::Report,
    ) {
        let key = (generation, address, idx);
        let Some((listeners, _)) = self.storage_requests.get(&key) else { return };
        if listeners.is_empty() {
            self.storage_requests.remove(&key);
            return;
        }
        trace!(target: "backendhandler", %address, %idx, %err, "retrying slot of a failed batch");
        let fut = Self::storage_future(self.provider.clone(), address, idx, block_id, generation);
        self.pending_requests.push(ProviderRequest::Storage(fut));
    }

    /// Reads one slot with `eth_getStorageAt` at `block_id`.
    fn storage_future(
        provider: DynProvider<N>,
        address: Address,
        idx: U256,
        block_id: BlockId,
        generation: u64,
    ) -> StorageFuture<eyre::Report> {
        Box::pin(async move {
            let storage = provider
                .raw_request("eth_getStorageAt".into(), (address, idx, StateBlockId(block_id)))
                .await
                .map_err(Into::into);
            (storage, address, idx, generation)
        })
    }

    /// Caches a storage response and answers its listeners.
    fn on_storage_response(
        &mut self,
        resp: Result<U256, eyre::Report>,
        addr: Address,
        idx: U256,
        generation: u64,
    ) {
        let request_key = (generation, addr, idx);
        let value = match resp {
            Ok(value) => value,
            Err(err) => {
                // notify all listeners
                let err = Arc::new(err);
                if let Some((listeners, _)) = self.storage_requests.remove(&request_key) {
                    for l in listeners {
                        let _ = l.send(Err(DatabaseError::GetStorage(addr, idx, Arc::clone(&err))));
                    }
                }
                return;
            }
        };

        // update the cache
        let (listeners, prefetch_only) =
            self.storage_requests.remove(&request_key).unwrap_or_default();
        // A value read at a superseded pin only answers its own listeners.
        let current = generation == self.state_generation;
        if current && prefetch_only {
            self.prefetched_storage.insert((addr, idx), value);
        } else if current {
            self.db.storage().write().entry(addr).or_default().insert(idx, value);
        }

        // notify all listeners
        for l in listeners {
            let _ = l.send(Ok(value));
        }
    }

    /// process a request for account's storage
    ///
    /// The value goes to the prefetch cache only if every listener is a prefetch.
    fn request_account_storage(
        &mut self,
        address: Address,
        idx: U256,
        listener: StorageSender,
        prefetch: bool,
    ) {
        let generation = self.state_generation;
        match self.storage_requests.entry((generation, address, idx)) {
            Entry::Occupied(mut entry) => {
                let (listeners, prefetch_only) = entry.get_mut();
                listeners.push(listener);
                *prefetch_only &= prefetch;
            }
            Entry::Vacant(entry) => {
                trace!(target: "backendhandler", %address, %idx, "preparing storage request");
                self.db.cache().record_cache_miss();
                entry.insert((vec![listener], prefetch));
                let block_id = self.block_id.unwrap_or_default();
                let fut =
                    Self::storage_future(self.provider.clone(), address, idx, block_id, generation);
                self.pending_requests.push(ProviderRequest::Storage(fut));
            }
        }
    }

    /// returns the future that fetches the account data
    fn get_account_req(&self, address: Address) -> ProviderRequest<eyre::Report, N> {
        trace!(target: "backendhandler", "preparing account request, address={:?}", address);

        let provider = self.provider.clone();
        let block_id = self.block_id.unwrap_or_default();
        let mode = Arc::clone(&self.account_fetch_mode);
        let policy = self.account_fetch_policy;
        let generation = self.state_generation;
        let fut = async move {
            if policy != AccountFetchPolicy::Auto {
                return Self::fetch_account_info(&provider, address, block_id)
                    .await
                    .wrap_err("fork account policy requires eth_getAccountInfo");
            }
            // depending on the tracked mode we can dispatch requests.
            match mode.load() {
                AccountFetchMode::Unchecked => {
                    // single request for accountinfo object
                    let acc_info_fut = Self::fetch_account_info(&provider, address, block_id);

                    // tri request for account info
                    let triple_fut = Self::fetch_account_separately(&provider, address, block_id);
                    pin_mut!(acc_info_fut, triple_fut);

                    select! {
                        acc_info = &mut acc_info_fut => {
                            match acc_info {
                                Ok(info) => {
                                 trace!(target: "backendhandler", "endpoint supports eth_getAccountInfo");
                                    mode.store(AccountFetchMode::SupportsAccountInfo);
                                    Ok(info)
                                }
                                Err(err) => {
                                    trace!(target: "backendhandler", ?err, "failed initial eth_getAccountInfo call");
                                    mode.store(AccountFetchMode::SeparateRequests);
                                    Ok(triple_fut.await?)
                                }
                            }
                        }
                        triple = &mut triple_fut => {
                            match triple {
                                Ok(info) => {
                                    mode.store(AccountFetchMode::SeparateRequests);
                                    Ok(info)
                                }
                                Err(err) => {
                                    // One of the separate requests can fail on its own (e.g. rate
                                    // limited) while eth_getAccountInfo is still in flight, so let
                                    // that settle the request before giving up.
                                    trace!(target: "backendhandler", ?err, "failed initial separate account requests");
                                    match acc_info_fut.await {
                                        Ok(info) => {
                                            mode.store(AccountFetchMode::SupportsAccountInfo);
                                            Ok(info)
                                        }
                                        Err(_) => Err(err),
                                    }
                                }
                            }
                        }
                    }
                }

                AccountFetchMode::SupportsAccountInfo => {
                    let mut res = Self::fetch_account_info(&provider, address, block_id).await;

                    // it's possible that the configured endpoint load balances requests to multiple
                    // instances and not all support that endpoint so we should reset here
                    if res.is_err() {
                        mode.store(AccountFetchMode::SeparateRequests);

                        res = Self::fetch_account_separately(&provider, address, block_id).await;
                    }

                    Ok(res?)
                }

                AccountFetchMode::SeparateRequests => {
                    Self::fetch_account_separately(&provider, address, block_id).await
                }
            }
        };

        ProviderRequest::Account(Box::pin(async move {
            let result = fut.await.map(|mut account| {
                // Only the extension policy keeps the extension, so the separate requests, which
                // cannot fetch it, return the same accounts.
                if policy != AccountFetchPolicy::RequireAccountInfoWithExtension {
                    account.extension = Bytes::new();
                }
                account
            });
            (result, address, generation)
        }))
    }

    async fn fetch_account_info(
        provider: &DynProvider<N>,
        address: Address,
        block_id: BlockId,
    ) -> eyre::Result<RemoteAccount> {
        Ok(provider
            .raw_request("eth_getAccountInfo".into(), (address, StateBlockId(block_id)))
            .await?)
    }

    async fn fetch_account_separately(
        provider: &DynProvider<N>,
        address: Address,
        block_id: BlockId,
    ) -> eyre::Result<RemoteAccount> {
        let params = (address, StateBlockId(block_id));
        let (balance, nonce, code) = futures::future::try_join3(
            provider.raw_request("eth_getBalance".into(), params),
            provider.raw_request("eth_getTransactionCount".into(), params),
            provider.raw_request("eth_getCode".into(), params),
        )
        .await?;
        Ok(RemoteAccount { balance, nonce, code, extension: Bytes::new() })
    }

    /// process a request for an account
    ///
    /// The value goes to the prefetch cache only if every listener is a prefetch.
    fn request_account(&mut self, address: Address, listener: AccountInfoSender, prefetch: bool) {
        match self.account_requests.entry((self.state_generation, address)) {
            Entry::Occupied(mut entry) => {
                let (listeners, prefetch_only) = entry.get_mut();
                listeners.push(listener);
                *prefetch_only &= prefetch;
            }
            Entry::Vacant(entry) => {
                self.db.cache().record_cache_miss();
                entry.insert((vec![listener], prefetch));
                self.pending_requests.push(self.get_account_req(address));
            }
        }
    }

    /// process a request for an entire block
    fn request_full_block(&mut self, number: BlockId, sender: FullBlockSender<N>) {
        let provider = self.provider.clone();
        let fut = Box::pin(async move {
            let block = provider
                .get_block(number)
                .full()
                .await
                .wrap_err(format!("could not fetch block {number:?}"));
            (sender, block, number)
        });

        self.pending_requests.push(ProviderRequest::FullBlock(fut));
    }

    /// process a request for a transactions
    fn request_transaction(&mut self, tx: B256, sender: TransactionSender<N>) {
        let provider = self.provider.clone();
        let fut = Box::pin(async move {
            let block = provider
                .get_transaction_by_hash(tx)
                .await
                .wrap_err_with(|| format!("could not get transaction {tx}"))
                .and_then(|maybe| {
                    maybe.ok_or_else(|| eyre::eyre!("could not get transaction {tx}"))
                });
            (sender, block, tx)
        });

        self.pending_requests.push(ProviderRequest::Transaction(fut));
    }

    /// Resolves an Arbitrum `BLOCKHASH` request in the remote EVM.
    ///
    /// Arbitrum's EVM block number is the L1 block number, while `eth_getBlockByNumber` uses the L2
    /// block number. Executing `BLOCKHASH` lets ArbOS apply its L1-to-L2 block hash mapping.
    async fn get_arbitrum_block_hash(
        provider: &DynProvider<N>,
        block_id: Option<BlockId>,
        number: u64,
    ) -> eyre::Result<B256> {
        let mut code = Vec::with_capacity(41);
        code.push(0x7f); // PUSH32
        code.extend_from_slice(&U256::from(number).to_be_bytes::<32>());
        code.extend_from_slice(&[
            0x40, // BLOCKHASH
            0x60, 0x00, // PUSH1 0
            0x52, // MSTORE
            0x60, 0x20, // PUSH1 32
            0x60, 0x00, // PUSH1 0
            0xf3, // RETURN
        ]);

        let address = Address::with_last_byte(0xde);
        let code = Bytes::from(code);
        let params = serde_json::json!([
            { "to": address, "data": code },
            StateBlockId(block_id.unwrap_or_else(BlockId::latest)),
            { address.to_string(): { "code": code } },
        ]);
        let output: Bytes = provider
            .raw_request("eth_call".into(), params)
            .await
            .wrap_err("failed to get Arbitrum block hash")?;
        eyre::ensure!(
            output.len() == 32,
            "invalid Arbitrum block hash response length: expected 32 bytes, got {}",
            output.len()
        );
        Ok(B256::from_slice(&output))
    }

    /// Returns whether the standard ArbSys precompile is available at the pinned block.
    async fn has_arbsys(
        provider: &DynProvider<N>,
        block_id: Option<BlockId>,
    ) -> eyre::Result<bool> {
        let params = serde_json::json!([
            {
                "to": Address::with_last_byte(0x64),
                "data": "0x051038f2", // arbOSVersion()
            },
            StateBlockId(block_id.unwrap_or_else(BlockId::latest)),
        ]);
        let output = provider
            .raw_request::<_, Bytes>("eth_call".into(), params)
            .await
            .wrap_err("failed to detect ArbSys")?;
        Ok(output.len() == 32 && U256::from_be_slice(&output) >= U256::from(56))
    }

    /// process a request for a block hash
    fn request_hash(&mut self, number: u64, listener: BlockHashSender) {
        let generation = self.block_generation;
        match self.block_requests.entry((generation, number)) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().push(listener);
            }
            Entry::Vacant(entry) => {
                trace!(target: "backendhandler", number, "preparing block hash request");
                self.db.cache().record_cache_miss();
                entry.insert(vec![listener]);
                let provider = self.provider.clone();
                let anchor = self.block_anchor;
                let block_id = self.block_id;
                let meta = Arc::clone(self.db.meta());
                let known_block_hash_via_evm = self.block_hash_via_evm;
                let fut = Box::pin(async move {
                    let result: eyre::Result<(BlockHashData, bool)> = async {
                        let cached_chain = meta.read().chain;
                        let chain = if let Some(chain) = cached_chain {
                            chain
                        } else {
                            let chain = Chain::from(
                                provider.get_chain_id().await.wrap_err("failed to get chain ID")?,
                            );
                            meta.write().chain = Some(chain);
                            chain
                        };
                        let via_evm = match known_block_hash_via_evm {
                            Some(value) => value,
                            None => {
                                chain.is_arbitrum()
                                    || (chain.is_id()
                                        && Self::has_arbsys(&provider, block_id).await?)
                            }
                        };
                        if via_evm {
                            let hash =
                                Self::get_arbitrum_block_hash(&provider, block_id, number).await?;
                            return Ok((
                                BlockHashData::from_iter([(U256::from(number), hash)]),
                                true,
                            ));
                        }

                        let hashes = if let Some(anchor) = anchor
                            && number < anchor.number
                            && anchor.number == anchor.rpc_number
                        {
                            let mut hashes = BlockHashData::default();
                            let mut descendant = anchor;
                            while descendant.number > number {
                                let block = provider
                                    .get_block_by_hash(descendant.hash)
                                    .hashes()
                                    .await
                                    .wrap_err_with(|| {
                                        format!(
                                            "failed to get anchored block {} ({})",
                                            descendant.rpc_number, descendant.hash
                                        )
                                    })?
                                    .ok_or_else(|| {
                                        eyre::eyre!(
                                            "anchored block {} ({}) not found",
                                            descendant.rpc_number,
                                            descendant.hash
                                        )
                                    })?;
                                let header = block.header();
                                eyre::ensure!(
                                    header.number() == descendant.rpc_number &&
                                        header.hash() == descendant.hash,
                                    "anchored block changed: expected {} ({}), got {} ({})",
                                    descendant.rpc_number,
                                    descendant.hash,
                                    header.number(),
                                    header.hash()
                                );
                                let rpc_number = descendant.rpc_number.checked_sub(1).ok_or_else(
                                    || eyre::eyre!("anchored ancestry precedes the genesis block"),
                                )?;
                                descendant = ForkBlock::with_rpc_number(
                                    descendant.number - 1,
                                    rpc_number,
                                    header.parent_hash(),
                                );
                                hashes.insert(U256::from(descendant.number), descendant.hash);
                            }
                            hashes
                        } else if anchor.is_none_or(|anchor| anchor.number != anchor.rpc_number) {
                            let block = provider
                                .get_block_by_number(number.into())
                                .hashes()
                                .await
                                .wrap_err("failed to get block");

                            match block {
                                Ok(Some(block)) => BlockHashData::from_iter([(
                                    U256::from(number),
                                    block.header().hash(),
                                )]),
                                Ok(None) => {
                                    warn!(target: "backendhandler", ?number, "block not found");
                                    BlockHashData::from_iter([(
                                        U256::from(number),
                                        KECCAK_EMPTY,
                                    )])
                                }
                                Err(err) => {
                                    error!(target: "backendhandler", %err, ?number, "failed to get block");
                                    return Err(err)
                                }
                            }
                        } else {
                            BlockHashData::from_iter([(U256::from(number), B256::ZERO)])
                        };
                        Ok((hashes, false))
                    }
                    .await;
                    let via_evm = result.as_ref().is_ok_and(|(_, via_evm)| *via_evm);
                    let block_hashes = result.map(|(hashes, _)| hashes);
                    (block_hashes, number, generation, via_evm)
                });
                self.pending_requests.push(ProviderRequest::BlockHash(fut));
            }
        }
    }
}

impl<N: Network, B: ForkBlockEnv> Future for BackendHandler<N, B> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let pin = self.get_mut();
        loop {
            // Drain queued requests first.
            while let Some(req) = pin.queued_requests.pop_front() {
                pin.on_request(req)
            }

            // receive new requests to delegate to the underlying provider
            loop {
                match Pin::new(&mut pin.incoming).poll_next(cx) {
                    Poll::Ready(Some(req)) => {
                        pin.queued_requests.push_back(req);
                    }
                    Poll::Ready(None) => {
                        trace!(target: "backendhandler", "last sender dropped, ready to drop (&flush cache)");
                        return Poll::Ready(());
                    }
                    Poll::Pending => break,
                }
            }

            // poll all requests in progress
            let mut kept = 0;
            for n in (0..pin.pending_requests.len()).rev() {
                let mut request = pin.pending_requests.swap_remove(n);
                match &mut request {
                    ProviderRequest::Account(fut) => {
                        if let Poll::Ready((resp, addr, generation)) = fut.poll_unpin(cx) {
                            let request_key = (generation, addr);
                            // get the response
                            let RemoteAccount { balance, nonce, code, extension } = match resp {
                                Ok(res) => res,
                                Err(err) => {
                                    let err = Arc::new(err);
                                    if let Some((listeners, _)) =
                                        pin.account_requests.remove(&request_key)
                                    {
                                        for l in listeners {
                                            let _ = l.send(Err(DatabaseError::GetAccount(
                                                addr,
                                                Arc::clone(&err),
                                            )));
                                        }
                                    }
                                    pin.drain_account_queue();
                                    continue;
                                }
                            };

                            // Convert the RPC account into cached metadata.
                            let (code, code_hash) = if code.is_empty() {
                                (Bytes::default(), KECCAK_EMPTY)
                            } else {
                                (code.clone(), keccak256(&code))
                            };

                            // update the cache
                            let acc = AccountInfo {
                                nonce: nonce.to(),
                                balance,
                                code: Some(code),
                                code_hash,
                                extension,
                            };
                            let (listeners, prefetch_only) =
                                pin.account_requests.remove(&request_key).unwrap_or_default();
                            // A value read at a superseded pin only answers its own listeners.
                            let current = generation == pin.state_generation;
                            if current && prefetch_only {
                                pin.prefetched_accounts.insert(addr, acc.clone());
                            } else if current {
                                pin.db.accounts().write().insert(addr, acc.clone());
                            }

                            // notify all listeners
                            for l in listeners {
                                let _ = l.send(Ok(acc.clone()));
                            }
                            pin.drain_account_queue();
                            continue;
                        }
                    }
                    ProviderRequest::Storage(fut) => {
                        if let Poll::Ready((resp, addr, idx, generation)) = fut.poll_unpin(cx) {
                            pin.on_storage_response(resp, addr, idx, generation);
                            continue;
                        }
                    }
                    ProviderRequest::StorageBatch(fut) => {
                        if let Poll::Ready((values, generation, block_id)) = fut.poll_unpin(cx) {
                            pin.prefetch_batches -= 1;
                            let mut failed = false;
                            for (addr, idx, value) in values {
                                match value {
                                    Ok(value) => {
                                        pin.on_storage_response(Ok(value), addr, idx, generation)
                                    }
                                    Err(err) => {
                                        failed = true;
                                        pin.retry_storage_read(
                                            addr, idx, block_id, generation, &err,
                                        );
                                    }
                                }
                            }
                            if failed {
                                // A rate-limited endpoint would slow every later burst, so the
                                // first failed batch stops prefetching for the run.
                                debug!(target: "backendhandler", "storage batch request failed, prefetching stopped");
                                pin.account_queue.clear();
                                pin.prefetch_queue.clear();
                                pin.access_list_gate.supported.store(false, Ordering::Relaxed);
                            }
                            pin.drain_prefetch_queue();
                            continue;
                        }
                    }
                    ProviderRequest::BlockHash(fut) => {
                        if let Poll::Ready((block_hash, number, generation, via_evm)) =
                            fut.poll_unpin(cx)
                        {
                            let request_key = (generation, number);
                            let hashes = match block_hash {
                                Ok(value) => value,
                                Err(err) => {
                                    let err = Arc::new(err);
                                    // notify all listeners
                                    if let Some(listeners) = pin.block_requests.remove(&request_key)
                                    {
                                        for l in listeners {
                                            let _ = l.send(Err(DatabaseError::GetBlockHash(
                                                number,
                                                Arc::clone(&err),
                                            )));
                                        }
                                    }
                                    continue;
                                }
                            };

                            // update the cache
                            let value = hashes[&U256::from(number)];
                            if via_evm {
                                pin.pinned_block_hashes.insert(request_key, value);
                            } else {
                                pin.db.block_hashes().write().extend(hashes);
                            }
                            pin.block_hash_via_evm = Some(via_evm);

                            // notify all listeners
                            if let Some(listeners) = pin.block_requests.remove(&request_key) {
                                for l in listeners {
                                    let _ = l.send(Ok(value));
                                }
                            }
                            continue;
                        }
                    }
                    ProviderRequest::FullBlock(fut) => {
                        if let Poll::Ready((sender, resp, number)) = fut.poll_unpin(cx) {
                            let msg = match resp {
                                Ok(Some(block)) => Ok(block),
                                Ok(None) => Err(DatabaseError::BlockNotFound(number)),
                                Err(err) => {
                                    let err = Arc::new(err);
                                    Err(DatabaseError::GetFullBlock(number, err))
                                }
                            };
                            let _ = sender.send(msg);
                            continue;
                        }
                    }
                    ProviderRequest::Transaction(fut) => {
                        if let Poll::Ready((sender, tx, tx_hash)) = fut.poll_unpin(cx) {
                            let msg = match tx {
                                Ok(tx) => Ok(tx),
                                Err(err) => {
                                    let err = Arc::new(err);
                                    Err(DatabaseError::GetTransaction(tx_hash, err))
                                }
                            };
                            let _ = sender.send(msg);
                            continue;
                        }
                    }
                    ProviderRequest::AccessList(fut) => {
                        if let Poll::Ready((resp, key, generation, with_overrides)) =
                            fut.poll_unpin(cx)
                        {
                            pin.access_list_requests.remove(&(generation, key));
                            match resp {
                                Ok(result) if generation == pin.state_generation => {
                                    let new_keys = pin.new_keys(&result);
                                    trace!(target: "backendhandler", to = %key.0, new_keys, error = ?result.error, "received access list");
                                    // A failed simulation may have stopped early; its short list
                                    // says nothing about the call.
                                    if result.error.is_none() && new_keys <= CHEAP_ACCESS_LIST {
                                        pin.access_list_gate.cheap.lock().insert(key);
                                    }
                                    // A failed batch in the meantime stopped prefetching.
                                    if pin.access_list_gate.supported.load(Ordering::Relaxed) {
                                        pin.prefetch_access_list(&result);
                                    }
                                }
                                // Superseded by a new pin or an eviction.
                                Ok(_) => {}
                                Err(err) => {
                                    if is_unsupported_method(&err) {
                                        debug!(target: "backendhandler", %err, "endpoint does not support eth_createAccessList, prefetching stopped");
                                        pin.access_list_gate
                                            .supported
                                            .store(false, Ordering::Relaxed);
                                    } else if with_overrides && is_invalid_params(&err) {
                                        debug!(target: "backendhandler", %err, "endpoint does not take state overrides in eth_createAccessList, prefetching local calls stopped");
                                        pin.access_list_gate
                                            .overrides_supported
                                            .store(false, Ordering::Relaxed);
                                    } else {
                                        trace!(target: "backendhandler", %err, "access list request failed");
                                    }
                                }
                            }
                            continue;
                        }
                    }
                    ProviderRequest::AnyRequest(fut) => {
                        if fut.poll_inner(cx).is_ready() {
                            continue;
                        }
                    }
                }
                // not ready, insert and poll again
                pin.pending_requests.push(request);
                kept += 1;
            }

            // A completed request may have started new ones, such as the reads of an access
            // list or the retry of a failed batch; poll them before waiting.
            if pin.pending_requests.len() > kept {
                continue;
            }

            // If no new requests have been queued, break to
            // be polled again later.
            if pin.queued_requests.is_empty() {
                return Poll::Pending;
            }
        }
    }
}

/// Mode for the `SharedBackend` how to block in the synchronous database methods when interacting
/// with [`BackendHandler`].
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub enum BlockingMode {
    /// This mode use `tokio::task::block_in_place()` to block in place.
    ///
    /// This should be used when blocking on the call site is disallowed.
    #[default]
    BlockInPlace,
    /// The mode blocks the current task
    ///
    /// This can be used if blocking on the call site is allowed, e.g. on a tokio blocking task.
    Block,
}

/// A block number and hash that identify the exact root of a fork.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForkBlock {
    /// EVM-visible block number used to key `BLOCKHASH` requests.
    pub number: u64,
    /// RPC block number used to validate fetched headers.
    pub rpc_number: u64,
    /// Block hash of the fork root.
    pub hash: B256,
}

impl ForkBlock {
    /// Creates a new exact fork root.
    pub const fn new(number: u64, hash: B256) -> Self {
        Self { number, rpc_number: number, hash }
    }

    /// Creates an exact fork root whose RPC and EVM-visible block numbers differ.
    pub const fn with_rpc_number(number: u64, rpc_number: u64, hash: B256) -> Self {
        Self { number, rpc_number, hash }
    }
}

impl BlockingMode {
    /// run process logic with the blocking mode
    pub fn run<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        match self {
            Self::BlockInPlace => tokio::task::block_in_place(f),
            Self::Block => f(),
        }
    }
}

/// A cloneable backend type that shares access to the backend data with all its clones.
///
/// This backend type is connected to the `BackendHandler` via a mpsc unbounded channel. The
/// `BackendHandler` is spawned on a tokio task and listens for incoming commands on the receiver
/// half of the channel. A `SharedBackend` holds a sender for that channel, which is `Clone`, so
/// there can be multiple `SharedBackend`s communicating with the same `BackendHandler`, hence this
/// `Backend` type is thread safe.
///
/// All `Backend` trait functions are delegated as a `BackendRequest` via the channel to the
/// `BackendHandler`. All `BackendRequest` variants include a sender half of an additional channel
/// that is used by the `BackendHandler` to send the result of an executed `BackendRequest` back to
/// `SharedBackend`.
///
/// The `BackendHandler` holds a `Provider` to look up missing accounts or storage slots
/// from remote (e.g. infura). It detects duplicate requests from multiple `SharedBackend`s and
/// bundles them together, so that always only one provider request is executed. For example, there
/// are two `SharedBackend`s, `A` and `B`, both request the basic account info of account
/// `0xasd9sa7d...` at the same time. After the `BackendHandler` receives the request from `A`, it
/// sends a new provider request to the provider's endpoint, then it reads the identical request
/// from `B` and simply adds it as an additional listener for the request already in progress,
/// instead of sending another one. So that after the provider returns the response all listeners
/// (`A` and `B`) get notified.
// **Note**: the implementation makes use of [tokio::task::block_in_place()] when interacting with
// the underlying [BackendHandler] which runs on a separate spawned tokio task.
// [tokio::task::block_in_place()]
// > Runs the provided blocking function on the current thread without blocking the executor.
// This prevents issues (hangs) we ran into were the [SharedBackend] itself is called from a spawned
// task.
#[derive(Clone, Debug)]
pub struct SharedBackend<N: Network = AnyNetwork, B: Serialize + Clone = serde_json::Value> {
    /// channel used for sending commands related to database operations
    backend: UnboundedSender<BackendRequest<N>>,
    /// Ensures that the underlying cache gets flushed once the last `SharedBackend` is dropped.
    ///
    /// There is only one instance of the type, so as soon as the last `SharedBackend` is deleted,
    /// `FlushJsonBlockCacheDB<B>` is also deleted and the cache is flushed.
    cache: Arc<FlushJsonBlockCacheDB<B>>,

    /// The mode for the `SharedBackend` to block in place or not
    blocking_mode: BlockingMode,
    /// Whether this backend is pinned to an immutable exact fork identity.
    exact: bool,
    /// Whether simulations are wanted, shared with the handler.
    access_list_gate: Arc<AccessListGate>,
}

impl<N: Network, B: ForkBlockEnv> SharedBackend<N, B> {
    /// _Spawns_ a new `BackendHandler` on a `tokio::task` that listens for requests from any
    /// `SharedBackend`. Missing values get inserted in the `db`.
    ///
    /// The spawned `BackendHandler` finishes once the last `SharedBackend` connected to it is
    /// dropped.
    pub async fn spawn_backend<P: Provider<N> + 'static>(
        provider: P,
        db: BlockchainDb<B>,
        pin_block: Option<BlockId>,
    ) -> Self {
        let (shared, handler) = Self::new(provider, db, pin_block);
        // spawn the provider handler to a task
        trace!(target: "backendhandler", "spawning Backendhandler task");
        tokio::spawn(handler);
        shared
    }

    /// Same as `Self::spawn_backend` but spawns the `BackendHandler` on a separate `std::thread` in
    /// its own `tokio::Runtime`
    pub fn spawn_backend_thread<P: Provider<N> + 'static>(
        provider: P,
        db: BlockchainDb<B>,
        pin_block: Option<BlockId>,
    ) -> Self {
        let (shared, handler) = Self::new(provider, db, pin_block);

        // spawn a light-weight thread with a thread-local async runtime just for
        // sending and receiving data from the remote client
        std::thread::Builder::new()
            .name("fork-backend".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("failed to build tokio runtime");

                rt.block_on(handler);
            })
            .expect("failed to spawn thread");
        trace!(target: "backendhandler", "spawned Backendhandler thread");

        shared
    }

    /// Returns a new `SharedBackend` and the `BackendHandler`
    pub fn new<P: Provider<N> + 'static>(
        provider: P,
        db: BlockchainDb<B>,
        pin_block: Option<BlockId>,
    ) -> (Self, BackendHandler<N, B>) {
        let (backend, backend_rx) = unbounded();
        let cache = Arc::new(FlushJsonBlockCacheDB(Arc::clone(db.cache())));
        let access_list_gate = Arc::new(AccessListGate::default());
        let handler = BackendHandler::new(
            provider.erased(),
            db,
            backend_rx,
            pin_block,
            None,
            Arc::clone(&access_list_gate),
        );
        let blocking_mode = Default::default();
        (Self { backend, cache, blocking_mode, exact: false, access_list_gate }, handler)
    }

    /// Returns a new [`SharedBackend`] and [`BackendHandler`] pinned to an exact block.
    pub fn new_with_anchor<P: Provider<N> + 'static>(
        provider: P,
        db: BlockchainDb<B>,
        anchor: ForkBlock,
    ) -> eyre::Result<(Self, BackendHandler<N, B>)> {
        Self::new_anchored(provider, db, anchor, BlockId::from((anchor.hash, None)))
    }

    /// Same as [`Self::new_with_anchor`], but addresses state reads by the anchor's RPC block
    /// number instead of its hash, for RPCs that cannot serve state by block hash.
    ///
    /// Block hash ancestry is still validated against the anchor; only the per-read reorg
    /// protection of hash-addressed state reads is lost.
    pub fn new_with_anchor_by_number<P: Provider<N> + 'static>(
        provider: P,
        db: BlockchainDb<B>,
        anchor: ForkBlock,
    ) -> eyre::Result<(Self, BackendHandler<N, B>)> {
        Self::new_anchored(provider, db, anchor, BlockId::number(anchor.rpc_number))
    }

    fn new_anchored<P: Provider<N> + 'static>(
        provider: P,
        db: BlockchainDb<B>,
        anchor: ForkBlock,
        block_id: BlockId,
    ) -> eyre::Result<(Self, BackendHandler<N, B>)> {
        let meta = db.meta().read();
        eyre::ensure!(
            meta.fork_hash == Some(anchor.hash),
            "exact fork database is not anchored at {}",
            anchor.hash
        );
        eyre::ensure!(meta.source_id.is_some(), "exact fork database has no RPC source identity");
        drop(meta);

        let (backend, backend_rx) = unbounded();
        let cache = Arc::new(FlushJsonBlockCacheDB(Arc::clone(db.cache())));
        db.block_hashes().write().insert(U256::from(anchor.number), anchor.hash);
        let access_list_gate = Arc::new(AccessListGate::default());
        let handler = BackendHandler::new(
            provider.erased(),
            db,
            backend_rx,
            Some(block_id),
            Some(anchor),
            Arc::clone(&access_list_gate),
        );
        let blocking_mode = Default::default();
        Ok((Self { backend, cache, blocking_mode, exact: true, access_list_gate }, handler))
    }

    /// Returns a new `SharedBackend` and the `BackendHandler` with a specific blocking mode
    pub fn with_blocking_mode(&self, mode: BlockingMode) -> Self {
        Self {
            backend: self.backend.clone(),
            cache: self.cache.clone(),
            blocking_mode: mode,
            exact: self.exact,
            access_list_gate: Arc::clone(&self.access_list_gate),
        }
    }

    /// Removes an account, or one of its storage `slot`s, from the cache, drops prefetched values
    /// and makes in-flight requests answer only their own listeners, so the next read of every
    /// key goes to the endpoint. Call it after the endpoint's state changes out of band, for
    /// example through `anvil_setStorageAt`.
    ///
    /// The handler applies it before any read sent after it.
    pub fn evict(&self, address: Address, slot: Option<U256>) {
        let _ = self.backend.unbounded_send(BackendRequest::Evict(address, slot));
    }

    /// Updates the pinned block to fetch data from
    pub fn set_pinned_block(&self, block: impl Into<BlockId>) -> eyre::Result<()> {
        eyre::ensure!(!self.exact, "exact fork backends must be replaced instead of re-pinned");
        let req = BackendRequest::SetPinnedBlock(block.into());
        self.backend.unbounded_send(req).map_err(|e| eyre::eyre!("{:?}", e))
    }

    /// Returns the full block for the given block identifier
    pub fn get_full_block(&self, block: impl Into<BlockId>) -> DatabaseResult<N::BlockResponse> {
        self.blocking_mode.run(|| {
            let (sender, rx) = oneshot_channel();
            let req = BackendRequest::FullBlock(block.into(), sender);
            self.backend.unbounded_send(req)?;
            rx.recv()?
        })
    }

    /// Returns the transaction for the hash
    pub fn get_transaction(&self, tx: B256) -> DatabaseResult<N::TransactionResponse> {
        self.blocking_mode.run(|| {
            let (sender, rx) = oneshot_channel();
            let req = BackendRequest::Transaction(tx, sender);
            self.backend.unbounded_send(req)?;
            rx.recv()?
        })
    }

    /// Fetches an account like [`Self::account`], but keeps it out of the cache file
    /// until a normal read requests it, so a wrong prediction does not persist.
    ///
    /// Errors are returned without logging.
    pub fn prefetch_basic(&self, address: Address) -> DatabaseResult<()> {
        self.do_get_basic(address, true).map(drop)
    }

    /// Fetches a storage slot like [`Self::storage_at`], but keeps it out of the cache
    /// file until a normal read requests it. See [`Self::prefetch_basic`].
    pub fn prefetch_storage(&self, address: Address, index: U256) -> DatabaseResult<()> {
        self.do_get_storage(address, index, true).map(drop)
    }

    /// Simulates a call on the endpoint with `eth_createAccessList` and prefetches the accounts
    /// and slots it touches, in parallel and without waiting. Reads that reach those keys while
    /// the requests are in flight wait for them instead of sending their own.
    ///
    /// The simulation runs on the remote state plus the overrides, so a call into state that only
    /// exists locally needs that state in `overrides`; a wrong prediction costs the requests and
    /// stays out of the cache file like [`Self::prefetch_storage`]. `code_hash` is the code the
    /// target runs, so calls into different contracts at one address, such as the test contracts
    /// of two suites, are told apart.
    ///
    /// Nothing is simulated while reads mostly hit the cache, and a call whose last list held few
    /// new keys is not simulated again. An endpoint without the method turns the feature off, and
    /// one that rejects the overrides stops the calls that need them.
    pub fn prefetch_access_list(
        &self,
        tx: TransactionRequest,
        overrides: Option<StateOverride>,
        code_hash: B256,
    ) {
        let call = Box::new((tx, overrides, code_hash));
        let _ = self.backend.unbounded_send(BackendRequest::PrefetchAccessList(call));
    }

    /// Returns whether a call would be simulated now, so a caller can skip building its
    /// overrides, see [`Self::prefetch_access_list`].
    pub fn wants_access_list(
        &self,
        tx: &TransactionRequest,
        code_hash: B256,
        overrides: bool,
    ) -> bool {
        access_list_key(tx, code_hash)
            .is_some_and(|key| self.access_list_gate.wanted(&key, overrides, &self.cache.0))
    }

    fn do_get_basic(
        &self,
        address: Address,
        prefetch: bool,
    ) -> DatabaseResult<Option<AccountInfo>> {
        self.blocking_mode.run(|| {
            let (sender, rx) = oneshot_channel();
            let req = BackendRequest::Basic(address, sender, prefetch);
            self.backend.unbounded_send(req)?;
            rx.recv()?.map(Some)
        })
    }

    fn do_get_storage(
        &self,
        address: Address,
        index: U256,
        prefetch: bool,
    ) -> DatabaseResult<U256> {
        self.blocking_mode.run(|| {
            let (sender, rx) = oneshot_channel();
            let req = BackendRequest::Storage(address, index, sender, prefetch);
            self.backend.unbounded_send(req)?;
            rx.recv()?
        })
    }

    fn do_get_block_hash(&self, number: u64) -> DatabaseResult<B256> {
        self.blocking_mode.run(|| {
            let (sender, rx) = oneshot_channel();
            let req = BackendRequest::BlockHash(number, sender);
            self.backend.unbounded_send(req)?;
            rx.recv()?
        })
    }

    /// Inserts or updates data for multiple addresses
    pub fn insert_or_update_address(&self, address_data: AddressData) {
        let req = BackendRequest::UpdateAddress(address_data);
        let err = self.backend.unbounded_send(req);
        match err {
            Ok(_) => (),
            Err(e) => {
                error!(target: "sharedbackend", "Failed to send update address request: {:?}", e)
            }
        }
    }

    /// Inserts or updates data for multiple storage slots
    pub fn insert_or_update_storage(&self, storage_data: StorageData) {
        let req = BackendRequest::UpdateStorage(storage_data);
        let err = self.backend.unbounded_send(req);
        match err {
            Ok(_) => (),
            Err(e) => {
                error!(target: "sharedbackend", "Failed to send update address request: {:?}", e)
            }
        }
    }

    /// Inserts or updates data for multiple block hashes
    pub fn insert_or_update_block_hashes(&self, block_hash_data: BlockHashData) {
        let req = BackendRequest::UpdateBlockHash(block_hash_data);
        let err = self.backend.unbounded_send(req);
        match err {
            Ok(_) => (),
            Err(e) => {
                error!(target: "sharedbackend", "Failed to send update address request: {:?}", e)
            }
        }
    }

    /// Returns any arbitrary request on the provider
    pub fn do_any_request<T, F>(&mut self, fut: F) -> DatabaseResult<T>
    where
        F: Future<Output = Result<T, eyre::Report>> + Send + 'static,
        T: fmt::Debug + Send + 'static,
    {
        self.blocking_mode.run(|| {
            let (sender, rx) = oneshot_channel::<Result<T, eyre::Report>>();
            let req = BackendRequest::AnyRequest(Box::new(AnyRequestFuture {
                sender,
                future: Box::pin(fut),
            }));
            self.backend.unbounded_send(req)?;
            rx.recv()?.map_err(|err| DatabaseError::AnyRequest(Arc::new(err)))
        })
    }

    /// Flushes the DB to disk if caching is enabled
    pub fn flush_cache(&self) {
        self.cache.0.flush();
    }

    /// Flushes the DB to a specific file
    pub fn flush_cache_to(&self, cache_path: &Path) {
        self.cache.0.flush_to(cache_path);
    }

    /// Returns the DB
    pub fn data(&self) -> Arc<MemDb> {
        self.cache.0.db().clone()
    }

    /// Returns the DB accounts
    pub fn accounts(&self) -> AddressData {
        self.cache.0.db().accounts.read().clone()
    }

    /// Returns the DB accounts length
    pub fn accounts_len(&self) -> usize {
        self.cache.0.db().accounts.read().len()
    }

    /// Returns the DB storage
    pub fn storage(&self) -> StorageData {
        self.cache.0.db().storage.read().clone()
    }

    /// Returns the DB storage length
    pub fn storage_len(&self) -> usize {
        self.cache.0.db().storage.read().len()
    }

    /// Returns the DB block_hashes
    pub fn block_hashes(&self) -> BlockHashData {
        self.cache.0.db().block_hashes.read().clone()
    }

    /// Returns the DB block_hashes length
    pub fn block_hashes_len(&self) -> usize {
        self.cache.0.db().block_hashes.read().len()
    }

    /// Returns the number of database requests that were served from cache.
    pub fn cache_hits(&self) -> u64 {
        self.cache.0.cache_hits()
    }

    /// Returns the number of cache lookups that scheduled a provider request.
    pub fn cache_misses(&self) -> u64 {
        self.cache.0.cache_misses()
    }
}

impl<N: Network, B: ForkBlockEnv> SharedBackend<N, B> {
    /// Loads account metadata and bytecode from the shared cache or RPC.
    pub fn account(&self, address: Address) -> Result<Option<AccountInfo>, DatabaseError> {
        trace!(target: "sharedbackend", %address, "request basic");
        self.do_get_basic(address, false).inspect_err(|err| {
            error!(target: "sharedbackend", %err, %address, "Failed to send/recv `basic`");
            if err.is_possibly_non_archive_node_error() {
                error!(target: "sharedbackend", "{NON_ARCHIVE_NODE_WARNING}");
            }
        })
    }

    /// Loads a storage slot from the shared cache or RPC.
    pub fn storage_at(&self, address: Address, index: U256) -> Result<U256, DatabaseError> {
        trace!(target: "sharedbackend", "request storage {:?} at {:?}", address, index);
        self.do_get_storage(address, index, false).inspect_err(|err| {
            error!(target: "sharedbackend", %err, %address, %index, "Failed to send/recv `storage`");
            if err.is_possibly_non_archive_node_error() {
                error!(target: "sharedbackend", "{NON_ARCHIVE_NODE_WARNING}");
            }
        })
    }

    /// Loads a historical block hash relative to the pinned fork.
    pub fn block_hash(&self, number: u64) -> Result<B256, DatabaseError> {
        trace!(target: "sharedbackend", "request block hash for number {:?}", number);
        self.do_get_block_hash(number).inspect_err(|err| {
            error!(target: "sharedbackend", %err, %number, "Failed to send/recv `block_hash`");
            if err.is_possibly_non_archive_node_error() {
                error!(target: "sharedbackend", "{NON_ARCHIVE_NODE_WARNING}");
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{cache::BlockchainDbMeta, test_utils::BlockEnv};
    use alloy_primitives::hex;
    use alloy_provider::ProviderBuilder;
    use alloy_rpc_client::ClientBuilder;
    use parking_lot::Mutex;
    use serde_json::{Value, json};
    use std::{
        path::PathBuf,
        sync::mpsc::{Receiver, channel},
    };
    use tiny_http::{Response, Server};

    type Backend = SharedBackend<AnyNetwork, BlockEnv>;

    const ANCHOR: B256 = B256::with_last_byte(0xaa);

    /// In-process JSON-RPC server that records every call, including each member of a batch, as
    /// `[method, params]`.
    struct MockRpc {
        server: Arc<Server>,
        calls: Arc<Mutex<Vec<Value>>>,
    }

    impl MockRpc {
        /// Answers each call in order with `handler(method, params)`: `Ok` is the result, `Err`
        /// the error.
        fn new(
            handler: impl Fn(&str, &Value) -> Result<Value, Value> + Send + Sync + 'static,
        ) -> Self {
            Self::serve(move |method, params, _| handler(method, params), false)
        }

        /// Like [`Self::new`], but answers each request on its own thread, so a handler can delay
        /// its reply without holding back the others.
        fn concurrent(
            handler: impl Fn(&str, &Value) -> Result<Value, Value> + Send + Sync + 'static,
        ) -> Self {
            Self::serve(move |method, params, _| handler(method, params), true)
        }

        /// Answers each call with `handler(method, params, in_batch)`, where `in_batch` tells
        /// whether the call is a member of a batch request.
        fn serve(
            handler: impl Fn(&str, &Value, bool) -> Result<Value, Value> + Send + Sync + 'static,
            concurrent: bool,
        ) -> Self {
            let server = Arc::new(Server::http("127.0.0.1:0").unwrap());
            let calls = Arc::<Mutex<Vec<Value>>>::default();
            let handler = Arc::new(handler);
            let (server2, calls2) = (Arc::clone(&server), Arc::clone(&calls));
            std::thread::spawn(move || {
                for mut request in server2.incoming_requests() {
                    let body: Value = serde_json::from_reader(request.as_reader()).unwrap();
                    let batch = body.as_array().cloned().unwrap_or_else(|| vec![body.clone()]);
                    calls2
                        .lock()
                        .extend(batch.iter().map(|rpc| json!([rpc["method"], rpc["params"]])));
                    let handler = Arc::clone(&handler);
                    let respond = move || {
                        let answer = |rpc: &Value| {
                            let mut response = json!({"jsonrpc": "2.0", "id": rpc["id"]});
                            let (method, params) =
                                (rpc["method"].as_str().unwrap(), &rpc["params"]);
                            match handler(method, params, body.is_array()) {
                                Ok(result) => response["result"] = result,
                                Err(error) => response["error"] = error,
                            }
                            response
                        };
                        let response = match body {
                            Value::Array(_) => Value::Array(batch.iter().map(answer).collect()),
                            _ => answer(&body),
                        };
                        // The client may drop a raced request before reading its response.
                        let _ = request.respond(Response::from_string(response.to_string()));
                    };
                    if concurrent {
                        std::thread::spawn(respond);
                    } else {
                        respond();
                    }
                }
            });
            Self { server, calls }
        }

        /// A server for backends that must not make any RPC calls.
        fn rejecting() -> Self {
            Self::new(|method, _| Err(unsupported(method)))
        }

        fn endpoint(&self) -> String {
            format!("http://{}", self.server.server_addr())
        }

        fn provider(&self) -> impl Provider<AnyNetwork> + Clone + use<> {
            ProviderBuilder::new()
                .network::<AnyNetwork>()
                .connect_client(ClientBuilder::default().http(self.endpoint().parse().unwrap()))
        }

        fn meta(&self) -> BlockchainDbMeta<BlockEnv> {
            BlockchainDbMeta::new(BlockEnv::default(), self.endpoint())
        }

        /// Mainnet metadata for an exact fork anchored at `hash`. Setting the chain skips chain ID
        /// detection.
        fn exact_meta(&self, hash: B256) -> BlockchainDbMeta<BlockEnv> {
            self.meta()
                .set_chain(Chain::mainnet())
                .with_fork_identity(hash, B256::with_last_byte(1))
        }

        async fn backend(&self, meta: BlockchainDbMeta<BlockEnv>, pin: Option<BlockId>) -> Backend {
            SharedBackend::spawn_backend(self.provider(), BlockchainDb::new(meta, None), pin).await
        }

        fn anchored(&self, meta: BlockchainDbMeta<BlockEnv>, anchor: ForkBlock) -> Backend {
            let db = BlockchainDb::new(meta, None);
            let (backend, handler) =
                SharedBackend::new_with_anchor(self.provider(), db, anchor).unwrap();
            tokio::spawn(handler);
            backend
        }

        /// Returns the calls once no call arrived for 50ms, so that requests the backend sent
        /// without waiting for them are included.
        fn calls(&self) -> Vec<Value> {
            let mut calls = self.calls.lock().clone();
            loop {
                std::thread::sleep(Duration::from_millis(50));
                let settled = self.calls.lock().clone();
                if settled.len() == calls.len() {
                    return calls;
                }
                calls = settled;
            }
        }

        /// Returns how often each method was called, as a JSON object.
        fn counts(&self) -> Value {
            let mut counts = json!({});
            for call in self.calls() {
                let count = &mut counts[call[0].as_str().unwrap()];
                *count = json!(count.as_u64().unwrap_or(0) + 1);
            }
            counts
        }
    }

    impl Drop for MockRpc {
        fn drop(&mut self) {
            self.server.unblock();
        }
    }

    /// The error of an RPC that does not support `method`.
    fn unsupported(method: &str) -> Value {
        json!({"code": -32601, "message": format!("the method {method} does not exist/is not available")})
    }

    /// Serves deliberately inconsistent balances: 42 and an extension from `eth_getAccountInfo`,
    /// which can be rejected, and 65535 from `eth_getBalance`.
    fn account_rpc(reject_account_info: bool) -> MockRpc {
        MockRpc::new(move |method, _| match method {
            "eth_getAccountInfo" if !reject_account_info => Ok(json!({
                "balance": "0x2a", "nonce": "0x7", "code": "0x6000", "extension": "0x01aa",
            })),
            "eth_getBalance" => Ok(json!("0xffff")),
            "eth_getTransactionCount" => Ok(json!("0x7")),
            "eth_getCode" => Ok(json!("0x6000")),
            _ => Err(unsupported(method)),
        })
    }

    /// An RPC block header with no transactions.
    fn block(number: u64, hash: B256, parent_hash: B256) -> Value {
        json!({
            "hash": hash,
            "parentHash": parent_hash,
            "sha3Uncles": B256::ZERO,
            "miner": Address::ZERO,
            "stateRoot": B256::ZERO,
            "transactionsRoot": B256::ZERO,
            "receiptsRoot": B256::ZERO,
            "logsBloom": format!("0x{}", "00".repeat(256)),
            "difficulty": "0x0",
            "number": format!("{number:#x}"),
            "gasLimit": "0x1c9c380",
            "gasUsed": "0x0",
            "timestamp": "0x1",
            "extraData": "0x",
            "mixHash": B256::ZERO,
            "nonce": "0x0000000000000000",
            "baseFeePerGas": "0x1",
            "transactions": [],
            "uncles": [],
        })
    }

    /// The `eth_call` that runs `BLOCKHASH(number)` in the remote EVM at `block`.
    fn arbitrum_block_hash_call(number: u64, block: Value) -> Value {
        let address = Address::with_last_byte(0xde);
        let code = format!("0x7f{number:064x}4060005260206000f3");
        json!(["eth_call", [{"to": address, "data": code}, block, {address.to_string(): {"code": code}}]])
    }

    #[test]
    fn state_block_id_preserves_hash_objects_numbers_and_tags() {
        let hash = B256::with_last_byte(1);
        assert_eq!(
            serde_json::to_value(StateBlockId(hash.into())).unwrap(),
            json!({"blockHash": hash})
        );
        for block in [
            BlockId::number(123),
            BlockId::latest(),
            BlockId::pending(),
            BlockId::safe(),
            BlockId::finalized(),
            BlockId::earliest(),
            BlockId::from((hash, Some(false))),
            BlockId::from((hash, Some(true))),
        ] {
            assert_eq!(
                serde_json::to_value(StateBlockId(block)).unwrap(),
                serde_json::to_value(block).unwrap()
            );
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn exact_anchor_account_requests_use_hash_objects_without_canonical_flag() {
        for (policy, mode, reject_account_info) in [
            (AccountFetchPolicy::RequireAccountInfo, AccountFetchMode::Unchecked, false),
            (
                AccountFetchPolicy::RequireAccountInfoWithExtension,
                AccountFetchMode::Unchecked,
                false,
            ),
            (AccountFetchPolicy::Auto, AccountFetchMode::Unchecked, false),
            (AccountFetchPolicy::Auto, AccountFetchMode::Unchecked, true),
            (AccountFetchPolicy::Auto, AccountFetchMode::SupportsAccountInfo, false),
            (AccountFetchPolicy::Auto, AccountFetchMode::SupportsAccountInfo, true),
            (AccountFetchPolicy::Auto, AccountFetchMode::SeparateRequests, true),
        ] {
            let rpc = account_rpc(reject_account_info);
            let db =
                BlockchainDb::new(rpc.exact_meta(ANCHOR).with_account_fetch_policy(policy), None);
            let (backend, handler) =
                SharedBackend::new_with_anchor(rpc.provider(), db, ForkBlock::new(123, ANCHOR))
                    .unwrap();
            handler.account_fetch_mode.store(mode);
            tokio::spawn(handler);

            for address in [Address::with_last_byte(1), Address::with_last_byte(2)] {
                let account = backend.account(address).unwrap().unwrap();
                assert_eq!(account.nonce, 7);
                assert_eq!(account.code.unwrap(), Bytes::from_static(&[0x60, 0]));
                if reject_account_info {
                    assert_eq!(account.balance, U256::from(65535));
                } else if policy != AccountFetchPolicy::Auto {
                    assert_eq!(account.balance, U256::from(42));
                }
                // Only the extension policy keeps the extension, whichever request answered.
                let extension = if policy == AccountFetchPolicy::RequireAccountInfoWithExtension {
                    Bytes::from_static(&[0x01, 0xaa])
                } else {
                    Bytes::new()
                };
                assert_eq!(account.extension, extension);
            }
            let calls = rpc.calls();
            assert!(!calls.is_empty());
            for call in calls {
                // Exact equality rejects both Moonbeam's ambiguous bare-hash form and
                // Rootstock's unsupported boolean (or null) requireCanonical field.
                assert_eq!(call[1][1], json!({"blockHash": ANCHOR}));
            }
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn required_account_info_uses_authoritative_balance_and_pinned_block() {
        let rpc = account_rpc(false);
        let meta = rpc.meta().with_account_fetch_policy(AccountFetchPolicy::RequireAccountInfo);
        let backend = rpc.backend(meta, Some(ANCHOR.into())).await;
        let (first, second) = (Address::with_last_byte(1), Address::with_last_byte(2));

        let account = backend.account(first).unwrap().unwrap();
        assert_eq!(account.balance, U256::from(42));
        assert_eq!(account.nonce, 7);
        assert_eq!(account.code.unwrap(), Bytes::from_static(&[0x60, 0]));

        // Repinning affects clones and keeps cached accounts.
        backend.set_pinned_block(123).unwrap();
        let clone = backend.clone();
        assert_eq!(clone.account(second).unwrap().unwrap().balance, U256::from(42));
        assert_eq!(backend.account(first).unwrap().unwrap().balance, U256::from(42));
        assert_eq!(
            rpc.calls(),
            [
                json!(["eth_getAccountInfo", [first, {"blockHash": ANCHOR}]]),
                json!(["eth_getAccountInfo", [second, "0x7b"]]),
            ]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn required_account_info_never_falls_back_after_rpc_errors() {
        let rpc = account_rpc(true);
        let meta = rpc.meta().with_account_fetch_policy(AccountFetchPolicy::RequireAccountInfo);
        let backend = rpc.backend(meta, None).await;

        assert!(backend.account(Address::with_last_byte(1)).is_err());
        assert!(backend.account(Address::with_last_byte(2)).is_err());
        assert_eq!(backend.accounts_len(), 0);
        assert_eq!(rpc.counts(), json!({"eth_getAccountInfo": 2}));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn auto_account_fetch_still_supports_separate_requests() {
        let rpc = account_rpc(true);
        let backend = rpc.backend(rpc.meta(), None).await;

        let account = backend.account(Address::with_last_byte(1)).unwrap().unwrap();
        assert_eq!(account.balance, U256::from(65535));
        assert_eq!(
            rpc.counts(),
            json!({
                "eth_getAccountInfo": 1,
                "eth_getBalance": 1,
                "eth_getTransactionCount": 1,
                "eth_getCode": 1,
            })
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn repinned_reads_neither_join_nor_cache_superseded_requests() {
        for storage in [true, false] {
            let (received_tx, received) = channel();
            let (release, release_rx) = channel::<()>();
            let held = Mutex::new(Some((received_tx, release_rx)));
            // Answers each state read with the block it was made at, holding the first one.
            let rpc = MockRpc::new(move |method, params| {
                let first = held.lock().take();
                if let Some((received, release)) = first {
                    received.send(()).unwrap();
                    release.recv().unwrap();
                }
                let block = params.as_array().unwrap().last().unwrap();
                match method {
                    "eth_getStorageAt" => Ok(block.clone()),
                    "eth_getAccountInfo" => {
                        Ok(json!({"balance": block, "nonce": "0x0", "code": "0x"}))
                    }
                    _ => Err(unsupported(method)),
                }
            });
            let meta = rpc.meta().with_account_fetch_policy(AccountFetchPolicy::RequireAccountInfo);
            let backend =
                rpc.backend(meta, Some(1.into())).await.with_blocking_mode(BlockingMode::Block);
            let read = move |backend: Backend| {
                std::thread::spawn(move || {
                    if storage {
                        backend.storage_at(Address::ZERO, U256::ZERO).unwrap()
                    } else {
                        backend.account(Address::ZERO).unwrap().unwrap().balance
                    }
                })
            };

            let stale = read(backend.clone());
            received.recv().unwrap();
            // Re-pin and wipe the cache the way a fork reset does, while the old read is in flight.
            backend.set_pinned_block(2).unwrap();
            backend.data().clear();
            let fresh = read(backend.clone());
            std::thread::sleep(Duration::from_millis(200));
            release.send(()).unwrap();

            assert_eq!(stale.join().unwrap(), U256::from(1));
            assert_eq!(fresh.join().unwrap(), U256::from(2));
            let call = |block| {
                if storage {
                    assert_eq!(backend.storage()[&Address::ZERO][&U256::ZERO], U256::from(2));
                    json!(["eth_getStorageAt", [Address::ZERO, "0x0", block]])
                } else {
                    assert_eq!(backend.accounts()[&Address::ZERO].balance, U256::from(2));
                    json!(["eth_getAccountInfo", [Address::ZERO, block]])
                }
            };
            assert_eq!(rpc.calls(), [call("0x1"), call("0x2")]);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn unchecked_account_fetch_falls_back_to_account_info_when_separate_requests_fail() {
        let rpc = MockRpc::concurrent(|method, _| match method {
            // Answers slower than the separate requests fail, e.g. on a rate limit.
            "eth_getAccountInfo" => {
                std::thread::sleep(Duration::from_millis(200));
                Ok(json!({"balance": "0x2a", "nonce": "0x7", "code": "0x"}))
            }
            "eth_getCode" => Err(json!({"code": 429, "message": "rate limited"})),
            "eth_getBalance" | "eth_getTransactionCount" => Ok(json!("0x1")),
            _ => Err(unsupported(method)),
        });
        let db = BlockchainDb::new(rpc.meta(), None);
        let (backend, handler) = SharedBackend::new(rpc.provider(), db, None);
        let mode = Arc::clone(&handler.account_fetch_mode);
        tokio::spawn(handler);

        let account = backend.account(Address::with_last_byte(1)).unwrap().unwrap();
        assert_eq!(account.balance, U256::from(42));
        assert_eq!(account.nonce, 7);
        assert_eq!(mode.load(), AccountFetchMode::SupportsAccountInfo);
        assert_eq!(
            rpc.counts(),
            json!({
                "eth_getAccountInfo": 1,
                "eth_getBalance": 1,
                "eth_getTransactionCount": 1,
                "eth_getCode": 1,
            })
        );
    }

    /// Fetched values land in the shared database, where clones on other threads see them.
    #[tokio::test(flavor = "multi_thread")]
    async fn shared_backend() {
        let hash = B256::with_last_byte(10);
        let rpc = MockRpc::new(move |method, _| match method {
            "eth_getAccountInfo" => Ok(json!({"balance": "0x2a", "nonce": "0x7", "code": "0x"})),
            "eth_getStorageAt" => Ok(json!("0x2a")),
            "eth_getBlockByNumber" => Ok(block(10, hash, B256::ZERO)),
            _ => Err(unsupported(method)),
        });
        let meta = rpc
            .meta()
            .set_chain(Chain::mainnet())
            .with_account_fetch_policy(AccountFetchPolicy::RequireAccountInfo);
        let backend = rpc.backend(meta, None).await;
        let address = Address::with_last_byte(1);

        let account = backend.account(address).unwrap().unwrap();
        assert_eq!(backend.storage_at(address, U256::ZERO).unwrap(), U256::from(42));
        assert_eq!(backend.block_hash(10).unwrap(), hash);
        assert_eq!(backend.accounts()[&address], account);
        assert_eq!(
            backend.storage()[&address],
            StorageInfo::from_iter([(U256::ZERO, U256::from(42))])
        );
        assert_eq!(backend.block_hashes()[&U256::from(10)], hash);

        let clone = backend.clone();
        std::thread::spawn(move || {
            for slot in 1..5 {
                clone.storage_at(address, U256::from(slot)).unwrap();
            }
        })
        .join()
        .unwrap();
        assert_eq!(backend.storage()[&address].len(), 5);
    }

    #[test]
    fn can_read_cache() {
        let cache_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-data/storage.json");
        let json = JsonBlockCacheDB::<BlockEnv>::load(cache_path).unwrap();
        assert!(!json.db().accounts.read().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cache_metrics_record_hits_for_cached_database_reads() {
        let rpc = MockRpc::rejecting();
        let backend = rpc.backend(rpc.meta(), None).await;
        let (address, slot, value) = (Address::with_last_byte(1), U256::from(3), U256::from(4));
        let (number, hash) = (5, B256::with_last_byte(6));
        let code = Bytes::from_static(&[0x60, 0x2a]);
        let account = AccountInfo {
            nonce: 1,
            balance: U256::from(2),
            code_hash: keccak256(&code),
            code: Some(code.clone()),
            extension: Bytes::new(),
        };
        backend.insert_or_update_address(AddressData::from_iter([(address, account.clone())]));
        backend.insert_or_update_storage(StorageData::from_iter([(
            address,
            StorageInfo::from_iter([(slot, value)]),
        )]));
        backend
            .insert_or_update_block_hashes(BlockHashData::from_iter([(U256::from(number), hash)]));

        assert_eq!((backend.cache_hits(), backend.cache_misses()), (0, 0));
        assert_eq!(backend.account(address).unwrap(), Some(account));
        assert_eq!(backend.storage_at(address, slot).unwrap(), value);
        assert_eq!(backend.block_hash(number).unwrap(), hash);
        assert_eq!((backend.cache_hits(), backend.cache_misses()), (3, 0));

        let clone = backend.with_blocking_mode(BlockingMode::Block);
        assert_eq!((clone.cache_hits(), clone.cache_misses()), (3, 0));
        assert_eq!(clone.accounts()[&address].code.as_ref().unwrap(), &code);
        assert_eq!(clone.storage()[&address][&slot], value);
        assert_eq!(clone.block_hashes()[&U256::from(number)], hash);
        assert_eq!(
            (clone.accounts_len(), clone.storage_len(), clone.block_hashes_len()),
            (1, 1, 1)
        );
        assert!(rpc.calls().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cache_metrics_record_storage_miss_then_hit() {
        let rpc = MockRpc::new(|method, _| match method {
            "eth_getStorageAt" => Ok(json!("0x2a")),
            _ => Err(unsupported(method)),
        });
        let backend = rpc.backend(rpc.meta(), None).await;
        let (address, slot) = (Address::with_last_byte(1), U256::from(1));

        assert_eq!(backend.storage_at(address, slot).unwrap(), U256::from(42));
        assert_eq!((backend.cache_hits(), backend.cache_misses()), (0, 1));
        assert_eq!(backend.storage_at(address, slot).unwrap(), U256::from(42));
        assert_eq!((backend.cache_hits(), backend.cache_misses()), (1, 1));
        assert_eq!(rpc.counts(), json!({"eth_getStorageAt": 1}));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cache_file_keeps_only_read_prefetches() {
        let joined = Address::with_last_byte(1);
        let unread = Address::with_last_byte(2);
        let (held_tx, held_rx) = channel();
        let (release_tx, release_rx) = channel::<()>();
        let release_rx = Mutex::new(release_rx);
        let rpc = MockRpc::new(move |method, params| {
            let account = method == "eth_getAccountInfo";
            // Hold only the requests used to test a read joining an in-flight prefetch.
            if params[0] == json!(joined) && (account || params[1] == "0x2") {
                held_tx.send(()).unwrap();
                release_rx.lock().recv_timeout(Duration::from_secs(10)).unwrap();
            }
            match method {
                "eth_getAccountInfo" => {
                    Ok(json!({"balance": "0x2a", "nonce": "0x0", "code": "0x"}))
                }
                "eth_getStorageAt" => Ok(json!("0x2a")),
                _ => Err(unsupported(method)),
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.json");
        let meta = rpc.meta().with_account_fetch_policy(AccountFetchPolicy::RequireAccountInfo);
        let db = BlockchainDb::new(meta, Some(path.clone()));
        let mut backend = SharedBackend::spawn_backend(rpc.provider(), db, None)
            .await
            .with_blocking_mode(BlockingMode::Block);

        // Unread prefetches stay out of the file.
        backend.prefetch_basic(unread).unwrap();
        backend.prefetch_storage(unread, U256::from(1)).unwrap();
        // A read promotes a completed prefetch without another RPC request.
        backend.prefetch_storage(joined, U256::from(1)).unwrap();
        assert_eq!(backend.storage_at(joined, U256::from(1)).unwrap(), U256::from(42));

        let prefetch = std::thread::spawn({
            let backend = backend.clone();
            move || backend.prefetch_basic(joined).unwrap()
        });
        held_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let (sender, read) = oneshot_channel();
        backend.backend.unbounded_send(BackendRequest::Basic(joined, sender, false)).unwrap();
        // This barrier completes only after the handler registers the preceding read.
        backend.do_any_request(async { Ok(()) }).unwrap();
        release_tx.send(()).unwrap();
        prefetch.join().unwrap();
        assert_eq!(read.recv().unwrap().unwrap().balance, U256::from(42));

        let prefetch = std::thread::spawn({
            let backend = backend.clone();
            move || backend.prefetch_storage(joined, U256::from(2)).unwrap()
        });
        held_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let (sender, read) = oneshot_channel();
        backend
            .backend
            .unbounded_send(BackendRequest::Storage(joined, U256::from(2), sender, false))
            .unwrap();
        backend.do_any_request(async { Ok(()) }).unwrap();
        release_tx.send(()).unwrap();
        prefetch.join().unwrap();
        assert_eq!(read.recv().unwrap().unwrap(), U256::from(42));
        assert_eq!(rpc.calls().len(), 5);
        backend.flush_cache();

        let cache = JsonBlockCacheDB::<BlockEnv>::load(&path).unwrap();
        assert_eq!(cache.db().accounts.read().keys().collect::<Vec<_>>(), [&joined]);
        let storage = cache.db().storage.read();
        let slots = StorageInfo::from_iter([
            (U256::from(1), U256::from(42)),
            (U256::from(2), U256::from(42)),
        ]);
        assert_eq!(*storage, AddressHashMap::from_iter([(joined, slots)]));
    }

    /// Serves storage value 42 and anchor block 10 with parent `parent_hash`.
    fn anchor_rpc(parent_hash: B256) -> MockRpc {
        MockRpc::new(move |method, _| match method {
            "eth_getStorageAt" => Ok(json!("0x2a")),
            "eth_getBlockByHash" => Ok(block(10, ANCHOR, parent_hash)),
            _ => Err(unsupported(method)),
        })
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn exact_anchor_pins_storage_and_block_ancestry() {
        let parent_hash = B256::with_last_byte(0xbb);
        let rpc = anchor_rpc(parent_hash);
        let backend = rpc.anchored(rpc.exact_meta(ANCHOR), ForkBlock::new(10, ANCHOR));

        assert_eq!(backend.storage_at(Address::ZERO, U256::ZERO).unwrap(), U256::from(42));
        assert_eq!(backend.block_hash(9).unwrap(), parent_hash);
        assert_eq!(
            rpc.calls(),
            [
                json!(["eth_getStorageAt", [Address::ZERO, "0x0", {"blockHash": ANCHOR}]]),
                json!(["eth_getBlockByHash", [ANCHOR, false]]),
            ]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn exact_anchor_by_number_reads_state_by_number() {
        let parent_hash = B256::with_last_byte(0xbb);
        let rpc = anchor_rpc(parent_hash);
        let db = BlockchainDb::new(rpc.exact_meta(ANCHOR), None);
        let (backend, handler) = SharedBackend::new_with_anchor_by_number(
            rpc.provider(),
            db,
            ForkBlock::new(10, ANCHOR),
        )
        .unwrap();
        tokio::spawn(handler);

        assert_eq!(backend.storage_at(Address::ZERO, U256::ZERO).unwrap(), U256::from(42));
        assert_eq!(backend.block_hash(9).unwrap(), parent_hash);
        assert_eq!(
            rpc.calls(),
            [
                json!(["eth_getStorageAt", [Address::ZERO, "0x0", "0xa"]]),
                json!(["eth_getBlockByHash", [ANCHOR, false]]),
            ]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn exact_anchor_walks_beyond_256_blocks() {
        let hash = |number: u64| B256::from(U256::from(number + 1));
        let rpc = MockRpc::new(move |method, params| {
            let requested: B256 = serde_json::from_value(params[0].clone()).unwrap();
            let number = U256::from_be_bytes(requested.0).to::<u64>() - 1;
            match method {
                "eth_getBlockByHash" => Ok(block(number, hash(number), hash(number - 1))),
                _ => Err(unsupported(method)),
            }
        });
        let backend = rpc.anchored(rpc.exact_meta(hash(300)), ForkBlock::new(300, hash(300)));

        assert_eq!(backend.block_hash(43).unwrap(), hash(43));
        let walked = (44..=300).rev().map(|n| json!(["eth_getBlockByHash", [hash(n), false]]));
        assert_eq!(rpc.calls(), walked.collect::<Vec<_>>());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn remapped_exact_anchor_uses_numbered_block_hash_lookup() {
        let hash = B256::with_last_byte(2);
        let rpc = MockRpc::new(move |method, _| match method {
            "eth_getBlockByNumber" => Ok(block(99, hash, B256::ZERO)),
            _ => Err(unsupported(method)),
        });
        let backend =
            rpc.anchored(rpc.exact_meta(ANCHOR), ForkBlock::with_rpc_number(100, 10, ANCHOR));

        assert_eq!(backend.block_hash(99).unwrap(), hash);
        assert_eq!(rpc.calls(), [json!(["eth_getBlockByNumber", ["0x63", false]])]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn exact_anchor_returns_zero_for_post_anchor_block() {
        let rpc = MockRpc::rejecting();
        // Without a chain, any block hash lookup would first ask the rejecting server for it.
        let meta = rpc.meta().with_fork_identity(ANCHOR, B256::with_last_byte(1));
        let backend = rpc.anchored(meta, ForkBlock::new(10, ANCHOR));

        assert_eq!(backend.block_hash(11).unwrap(), B256::ZERO);
        assert!(rpc.calls().is_empty());
    }

    #[test]
    fn exact_backend_cannot_be_repinned() {
        let rpc = MockRpc::rejecting();
        let db = BlockchainDb::new(rpc.exact_meta(ANCHOR), None);
        let (backend, _handler) =
            SharedBackend::new_with_anchor(rpc.provider(), db, ForkBlock::new(1, ANCHOR)).unwrap();

        let err = backend.set_pinned_block(2).unwrap_err();
        assert_eq!(err.to_string(), "exact fork backends must be replaced instead of re-pinned");
    }

    #[test]
    fn exact_backend_requires_matching_fork_identity() {
        let rpc = MockRpc::rejecting();
        let db = BlockchainDb::new(rpc.exact_meta(B256::with_last_byte(2)), None);

        let err = SharedBackend::new_with_anchor(rpc.provider(), db, ForkBlock::new(1, ANCHOR))
            .err()
            .unwrap();
        assert_eq!(err.to_string(), format!("exact fork database is not anchored at {ANCHOR}"));
    }

    #[test]
    fn exact_backend_requires_source_identity() {
        let rpc = MockRpc::rejecting();
        let meta = BlockchainDbMeta { fork_hash: Some(ANCHOR), ..rpc.meta() };
        let db = BlockchainDb::new(meta, None);

        let err = SharedBackend::new_with_anchor(rpc.provider(), db, ForkBlock::new(1, ANCHOR))
            .err()
            .unwrap();
        assert_eq!(err.to_string(), "exact fork database has no RPC source identity");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn arbitrum_block_hash_uses_pinned_eth_call() {
        let (first, second) = (B256::with_last_byte(0x42), B256::with_last_byte(0x43));
        let rpc = MockRpc::new(move |method, params| match (method, params[1].as_str()) {
            ("eth_call", Some("0x64")) => Ok(json!(first)),
            ("eth_call", Some("0xc8")) => Ok(json!(second)),
            _ => Err(unsupported(method)),
        });
        let meta = rpc.meta().set_chain(Chain::arbitrum_mainnet());
        let backend = rpc.backend(meta, Some(100.into())).await;

        assert_eq!(backend.block_hash(99).unwrap(), first);
        backend.evict(Address::ZERO, None);
        assert_eq!(backend.block_hash(99).unwrap(), first);
        backend.set_pinned_block(200).unwrap();
        assert_eq!(backend.block_hash(99).unwrap(), second);
        assert_eq!(
            rpc.calls(),
            [
                arbitrum_block_hash_call(99, json!("0x64")),
                arbitrum_block_hash_call(99, json!("0xc8"))
            ]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn exact_arbitrum_anchor_uses_pinned_eth_call() {
        let hash = B256::with_last_byte(0x42);
        let rpc = MockRpc::new(move |method, _| match method {
            "eth_call" => Ok(json!(hash)),
            _ => Err(unsupported(method)),
        });
        let meta = rpc.exact_meta(ANCHOR).set_chain(Chain::arbitrum_mainnet());
        let backend = rpc.anchored(meta, ForkBlock::new(100, ANCHOR));

        assert_eq!(backend.block_hash(99).unwrap(), hash);
        assert_eq!(rpc.calls(), [arbitrum_block_hash_call(99, json!({"blockHash": ANCHOR}))]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn custom_orbit_chain_is_detected_through_arbsys() {
        let hash = B256::with_last_byte(0x42);
        let arbsys = Address::with_last_byte(0x64);
        let rpc = MockRpc::new(move |method, params| match method {
            "eth_chainId" => Ok(json!("0x123456")),
            "eth_call" if params[0]["to"] == json!(arbsys) => Ok(json!(format!("0x{:064x}", 56))),
            "eth_call" => Ok(json!(hash)),
            _ => Err(unsupported(method)),
        });
        let backend = rpc.backend(rpc.meta(), Some(100.into())).await;

        assert_eq!(backend.block_hash(99).unwrap(), hash);
        assert_eq!(
            rpc.calls(),
            [
                json!(["eth_chainId", null]),
                json!(["eth_call", [{"to": arbsys, "data": "0x051038f2"}, "0x64"]]),
                arbitrum_block_hash_call(99, json!("0x64")),
            ]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn can_modify_address() {
        let rpc = MockRpc::rejecting();
        let backend = rpc.backend(rpc.meta(), None).await;
        let address = Address::with_last_byte(1);
        let account = AccountInfo { nonce: 1000, balance: U256::from(2000), ..Default::default() };

        backend.insert_or_update_address(AddressData::from_iter([(address, account.clone())]));
        assert_eq!(backend.account(address).unwrap(), Some(account.clone()));
        assert_eq!(backend.accounts()[&address], account);
        assert!(rpc.calls().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn can_modify_storage() {
        let rpc = MockRpc::rejecting();
        let backend = rpc.backend(rpc.meta(), None).await;
        let address = Address::with_last_byte(1);
        let storage = StorageInfo::from_iter(
            [(20, 10), (30, 15), (40, 20)]
                .map(|(slot, value)| (U256::from(slot), U256::from(value))),
        );

        backend.insert_or_update_storage(StorageData::from_iter([(address, storage.clone())]));
        for (&slot, &value) in &storage {
            assert_eq!(backend.storage_at(address, slot).unwrap(), value);
        }
        assert_eq!(backend.storage()[&address], storage);
        assert!(rpc.calls().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn can_modify_block_hashes() {
        let rpc = MockRpc::rejecting();
        let backend = rpc.backend(rpc.meta(), None).await;
        let hashes =
            BlockHashData::from_iter((1..=5).map(|n| (U256::from(n), B256::with_last_byte(n))));

        backend.insert_or_update_block_hashes(hashes.clone());
        for n in 1..=5 {
            assert_eq!(backend.block_hash(n).unwrap(), B256::with_last_byte(n as u8));
        }
        assert_eq!(backend.block_hashes(), hashes);
        assert!(rpc.calls().is_empty());
    }

    /// Overridden accounts and storage are flushed to the disk cache and loaded back.
    #[tokio::test(flavor = "multi_thread")]
    async fn can_modify_storage_with_cache() {
        let rpc = MockRpc::rejecting();
        let dir = tempfile::tempdir().unwrap();
        let cache_path = dir.path().join("cache.json");
        let db = BlockchainDb::new(rpc.meta(), Some(cache_path.clone()));
        let backend = SharedBackend::spawn_backend(rpc.provider(), db, None).await;
        let address = Address::with_last_byte(1);
        let code = Bytes::from_static(&[10, 20, 30, 40]);
        let account =
            AccountInfo { code_hash: keccak256(&code), code: Some(code), ..Default::default() };
        let storage =
            StorageInfo::from_iter((1..=6).map(|slot| (U256::from(slot), U256::from(slot * 10))));

        backend.insert_or_update_storage(StorageData::from_iter([(address, storage.clone())]));
        backend.insert_or_update_address(AddressData::from_iter([(address, account.clone())]));
        for (&slot, &value) in &storage {
            assert_eq!(backend.storage_at(address, slot).unwrap(), value);
        }
        backend.flush_cache();

        let reloaded = BlockchainDb::new(rpc.meta(), Some(cache_path));
        assert_eq!(reloaded.accounts().read()[&address], account);
        assert_eq!(reloaded.storage().read()[&address], storage);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shared_backend_any_request() {
        let rpc = MockRpc::new(|method, _| match method {
            "foo_callCustomMethod" => Ok(json!("0xffee")),
            _ => Err(unsupported(method)),
        });
        let provider = rpc.provider();
        let mut backend = rpc.backend(rpc.meta(), None).await;

        let bytes = backend
            .do_any_request(async move {
                Ok(provider
                    .raw_request::<_, Bytes>("foo_callCustomMethod".into(), ["0001"])
                    .await?)
            })
            .unwrap();
        assert_eq!(bytes, Bytes::from_static(&[0xff, 0xee]));
        assert_eq!(rpc.calls(), [json!(["foo_callCustomMethod", ["0001"]])]);
    }

    /// Answers account and storage reads with 42 and `eth_createAccessList` with `access_list`.
    /// With `failing_slot`, storage reads inside batch requests wait until its flag is set, and
    /// the read of that slot fails, like a rate-limited member of a prefetch batch.
    fn access_list_rpc(
        access_list: Result<Value, Value>,
        failing_slot: Option<(U256, Arc<AtomicBool>)>,
    ) -> MockRpc {
        MockRpc::serve(
            move |method, params, in_batch| match method {
                "eth_createAccessList" => access_list.clone(),
                "eth_getAccountInfo" => {
                    Ok(json!({"balance": "0x2a", "nonce": "0x0", "code": "0x"}))
                }
                "eth_getStorageAt" => {
                    if in_batch && let Some((slot, released)) = &failing_slot {
                        wait_until("batch release", || released.load(Ordering::Relaxed));
                        if params[1] == json!(slot) {
                            return Err(json!({"code": -32005, "message": "rate limited"}));
                        }
                    }
                    Ok(json!("0x2a"))
                }
                _ => Err(unsupported(method)),
            },
            false,
        )
    }

    /// An access list with one slot of `target` and one of `other`.
    fn listed(target: Address, other: Address) -> Result<Value, Value> {
        Ok(json!({
            "accessList": [
                {"address": target, "storageKeys": [B256::with_last_byte(1)]},
                {"address": other, "storageKeys": [B256::with_last_byte(2)]},
            ],
            "gasUsed": "0x5208",
        }))
    }

    /// A blocking backend whose account reads use `eth_getAccountInfo`.
    async fn access_list_backend(rpc: &MockRpc, pin: Option<BlockId>) -> Backend {
        let meta = rpc.meta().with_account_fetch_policy(AccountFetchPolicy::RequireAccountInfo);
        rpc.backend(meta, pin).await.with_blocking_mode(BlockingMode::Block)
    }

    fn access_list_call(target: Address, selector: u8) -> TransactionRequest {
        TransactionRequest::default()
            .from(Address::with_last_byte(9))
            .to(target)
            .input(Bytes::from(vec![selector, 0, 0, 0, 1]).into())
    }

    /// Waits until `done` returns true, failing after 10 seconds.
    fn wait_until(what: &str, done: impl Fn() -> bool) {
        let started = std::time::Instant::now();
        while !done() {
            assert!(started.elapsed() < Duration::from_secs(10), "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn access_list_prefetch_serves_reads_and_skips_cheap_calls() {
        let target = Address::with_last_byte(1);
        let other = Address::with_last_byte(2);
        let rpc = access_list_rpc(listed(target, other), None);
        let mut backend = access_list_backend(&rpc, Some(123.into())).await;

        backend.prefetch_access_list(access_list_call(target, 0xaa), None, B256::ZERO);
        // Waits until the two accounts and two slots of the access list have been requested.
        wait_until("prefetch", || backend.cache_misses() >= 4);
        // Reads of the listed keys are served from the prefetch; the slot of `other` stays out
        // of the cache until it is read.
        assert_eq!(backend.storage_at(target, U256::from(1)).unwrap(), U256::from(42));
        assert_eq!(backend.account(other).unwrap().unwrap().balance, U256::from(42));
        assert!(!backend.storage().contains_key(&other));
        assert_eq!(backend.storage_at(other, U256::from(2)).unwrap(), U256::from(42));

        // The same target and selector is simulated again, since its list held four keys and the
        // cache missed since, but a simulation in flight is not repeated unless the target's
        // override code differs.
        let overrides = StateOverride::default();
        backend.prefetch_access_list(access_list_call(target, 0xaa), None, B256::ZERO);
        backend.prefetch_access_list(access_list_call(target, 0xaa), None, B256::ZERO);
        backend.prefetch_access_list(
            access_list_call(target, 0xaa),
            Some(overrides),
            B256::with_last_byte(1),
        );
        backend.do_any_request(async { Ok(()) }).unwrap();
        assert_eq!(backend.storage_at(target, U256::from(1)).unwrap(), U256::from(42));
        // Any call into a loaded target is skipped once the reads since the last simulation mostly
        // hit the cache, so one new slot is read directly.
        assert!(backend.account(target).unwrap().is_some());
        for _ in 0..WARM_CACHE_READS {
            assert_eq!(backend.storage_at(target, U256::from(1)).unwrap(), U256::from(42));
        }
        assert_eq!(backend.storage_at(target, U256::from(7)).unwrap(), U256::from(42));
        assert!(!backend.wants_access_list(&access_list_call(target, 0xcc), B256::ZERO, false));
        backend.prefetch_access_list(access_list_call(target, 0xcc), None, B256::ZERO);
        backend.do_any_request(async { Ok(()) }).unwrap();
        // A target the fork has not loaded is simulated once whatever the other reads hit, and
        // then goes through the warm gate.
        let unloaded = access_list_call(Address::with_last_byte(5), 0xcc);
        assert!(backend.wants_access_list(&unloaded, B256::ZERO, false));
        backend.prefetch_access_list(unloaded.clone(), None, B256::ZERO);
        backend.do_any_request(async { Ok(()) }).unwrap();
        for _ in 0..WARM_CACHE_READS {
            assert_eq!(backend.storage_at(target, U256::from(1)).unwrap(), U256::from(42));
        }
        assert!(!backend.wants_access_list(&unloaded, B256::ZERO, false));
        // After a long warm run, a few new slots turn simulations back on, cheap calls included.
        for _ in 0..WARM_WINDOW {
            assert!(!backend.wants_access_list(&access_list_call(target, 0xcc), B256::ZERO, false));
            assert_eq!(backend.storage_at(target, U256::from(1)).unwrap(), U256::from(42));
        }
        for slot in 8..12 {
            assert_eq!(backend.storage_at(target, U256::from(slot)).unwrap(), U256::from(42));
        }
        assert!(backend.wants_access_list(&access_list_call(target, 0xaa), B256::ZERO, false));

        assert_eq!(
            rpc.counts(),
            json!({"eth_createAccessList": 4, "eth_getStorageAt": 7, "eth_getAccountInfo": 2})
        );
        let simulation = rpc.calls().into_iter().find(|call| call[0] == "eth_createAccessList");
        assert_eq!(simulation.unwrap()[1], json!([access_list_call(target, 0xaa), "0x7b"]));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn access_list_prefetch_retries_failed_batch_members() {
        let target = Address::with_last_byte(1);
        let other = Address::with_last_byte(2);
        let release = Arc::new(AtomicBool::new(false));
        let rpc =
            access_list_rpc(listed(target, other), Some((U256::from(1), Arc::clone(&release))));
        let mut backend = access_list_backend(&rpc, None).await;

        backend.prefetch_access_list(access_list_call(target, 0xaa), None, B256::ZERO);
        // Waits until the two accounts and two slots of the access list have been requested.
        wait_until("prefetch", || backend.cache_misses() >= 4);
        // The read joins the held batch. Its member fails, so a read of its own answers it; the
        // other slot of the batch is served from the prefetch.
        let reader = backend.clone();
        let read = std::thread::spawn(move || reader.storage_at(target, U256::from(1)));
        std::thread::sleep(Duration::from_millis(100));
        release.store(true, Ordering::Relaxed);
        assert_eq!(read.join().unwrap().unwrap(), U256::from(42));
        assert_eq!(backend.storage_at(other, U256::from(2)).unwrap(), U256::from(42));
        // The failed batch stopped prefetching.
        backend.prefetch_access_list(access_list_call(target, 0xbb), None, B256::ZERO);
        backend.do_any_request(async { Ok(()) }).unwrap();
        assert_eq!(backend.storage_at(target, U256::from(3)).unwrap(), U256::from(42));

        assert_eq!(
            rpc.counts(),
            json!({"eth_createAccessList": 1, "eth_getStorageAt": 4, "eth_getAccountInfo": 2})
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn access_list_prefetch_repeats_failed_simulations() {
        let target = Address::with_last_byte(1);
        let reverted = json!({"accessList": [], "gasUsed": "0x5208", "error": "out of gas"});
        let rpc = access_list_rpc(Ok(reverted), None);
        let mut backend = access_list_backend(&rpc, None).await;

        // An empty list of a reverted simulation does not mark the call as cheap, so a later
        // call with other gas or arguments is simulated again.
        for slot in 1..=2 {
            backend.prefetch_access_list(access_list_call(target, 0xaa), None, B256::ZERO);
            backend.do_any_request(async { Ok(()) }).unwrap();
            assert_eq!(backend.storage_at(target, U256::from(slot)).unwrap(), U256::from(42));
            // Waits for the simulation, so the next call does not find it in flight.
            assert_eq!(
                rpc.counts(),
                json!({"eth_createAccessList": slot, "eth_getStorageAt": slot})
            );
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn access_list_prefetch_stops_on_unsupported_method() {
        let target = Address::with_last_byte(1);
        let rpc = access_list_rpc(Err(unsupported("eth_createAccessList")), None);
        let mut backend = access_list_backend(&rpc, None).await;

        backend.prefetch_access_list(access_list_call(target, 0xaa), None, B256::ZERO);
        // Waits for the rejection, which arrives after the simulation request.
        wait_until("rejection", || {
            !backend.wants_access_list(&access_list_call(target, 0xbb), B256::ZERO, false)
        });
        assert_eq!(backend.storage_at(target, U256::from(1)).unwrap(), U256::from(42));
        backend.prefetch_access_list(access_list_call(target, 0xbb), None, B256::ZERO);
        backend.do_any_request(async { Ok(()) }).unwrap();
        assert_eq!(backend.storage_at(target, U256::from(2)).unwrap(), U256::from(42));

        assert_eq!(rpc.counts(), json!({"eth_createAccessList": 1, "eth_getStorageAt": 2}));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn rpc_accounts_preserve_empty_and_delegated_code() {
        let (empty, delegated) = (Address::with_last_byte(1), Address::with_last_byte(2));
        let delegation =
            Bytes::from_static(&hex!("ef01000000000000000000000000000000000000000042"));
        let rpc = MockRpc::new({
            let delegation = delegation.clone();
            move |method, params| match method {
                "eth_getAccountInfo" => {
                    let code = if params[0] == json!(delegated) {
                        delegation.clone()
                    } else {
                        Bytes::new()
                    };
                    Ok(json!({"balance": "0x0", "nonce": "0x0", "code": code}))
                }
                _ => Err(unsupported(method)),
            }
        });
        let meta = rpc
            .exact_meta(ANCHOR)
            .with_account_fetch_policy(AccountFetchPolicy::RequireAccountInfo);
        let backend = rpc.anchored(meta, ForkBlock::new(10, ANCHOR));

        for (address, code) in [(empty, Bytes::new()), (delegated, delegation)] {
            // RPCs cannot distinguish absent from empty accounts: retain Some(empty).
            let account = backend.account(address).unwrap().unwrap();
            assert_eq!(account.balance, U256::ZERO);
            assert_eq!(account.nonce, 0);
            assert_eq!(account.code_hash, keccak256(&code));
            assert_eq!(account.code.as_ref().unwrap(), &code);
            let cached = backend.clone().account(address).unwrap().unwrap();
            assert_eq!(cached.code.unwrap(), code);
        }
        assert_eq!((backend.cache_hits(), backend.cache_misses()), (2, 2));
        assert_eq!(
            rpc.calls(),
            [
                json!(["eth_getAccountInfo", [empty, {"blockHash": ANCHOR}]]),
                json!(["eth_getAccountInfo", [delegated, {"blockHash": ANCHOR}]]),
            ]
        );
    }

    /// A storage response that the handler received for the old state before an eviction does
    /// not put the old value back, so the read after the eviction fetches the new value.
    #[tokio::test(flavor = "multi_thread")]
    async fn evict_drops_a_response_received_before_it() {
        let target = Address::with_last_byte(1);
        let value = Arc::new(AtomicU64::new(1));
        let rpc = MockRpc::new({
            let value = Arc::clone(&value);
            move |method, _| match method {
                "eth_getStorageAt" => Ok(json!(U256::from(value.load(Ordering::Relaxed)))),
                _ => Err(unsupported(method)),
            }
        });
        let db = BlockchainDb::new(rpc.meta(), None);
        let (backend, mut handler) = Backend::new(rpc.provider(), db, Some(123.into()));
        // Polls the handler by hand, so a received response waits until the next poll.
        let mut poll_until = async |done: &dyn Fn() -> bool| {
            let started = std::time::Instant::now();
            while !done() {
                assert!(started.elapsed() < Duration::from_secs(10), "handler did not finish");
                std::future::poll_fn(|cx| {
                    assert!(Pin::new(&mut handler).poll(cx).is_pending());
                    Poll::Ready(())
                })
                .await;
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        };
        let read = |backend: &Backend| {
            let (sender, rx) = oneshot_channel();
            backend
                .backend
                .unbounded_send(BackendRequest::Storage(target, U256::ZERO, sender, false))
                .unwrap();
            rx
        };

        let old = read(&backend);
        // The server records a call before answering it, so the handler is not polled again once
        // the old value is sent.
        poll_until(&|| rpc.calls.lock().len() == 1).await;
        // Let the old value reach the transport before the endpoint changes.
        tokio::time::sleep(Duration::from_millis(100)).await;
        value.store(2, Ordering::Relaxed);
        backend.evict(target, Some(U256::ZERO));
        let new = read(&backend);
        let new_value = std::cell::OnceCell::new();
        poll_until(&|| {
            new_value.get().is_some() || new.try_recv().is_ok_and(|v| new_value.set(v).is_ok())
        })
        .await;

        assert_eq!(old.recv().unwrap().unwrap(), U256::from(1));
        assert_eq!(*new_value.get().unwrap().as_ref().unwrap(), U256::from(2));
    }
}
