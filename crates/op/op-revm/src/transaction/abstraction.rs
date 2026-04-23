//! Optimism transaction abstraction containing the `[OpTxTr]` trait and corresponding
//! `[OpTransaction]` type.
use super::deposit::{DEPOSIT_TRANSACTION_TYPE, DepositTransactionParts};
use auto_impl::auto_impl;
use revm::{
    context::TxEnv,
    context_interface::transaction::Transaction,
    handler::SystemCallTx,
    primitives::{Address, B256, Bytes, TxKind, U256},
};

/// Optimism Transaction trait.
#[auto_impl(&, &mut, Box, Arc)]
pub trait OpTxTr: Transaction {
    /// Enveloped transaction bytes.
    fn enveloped_tx(&self) -> Option<&Bytes>;

    /// Source hash of the deposit transaction.
    fn source_hash(&self) -> Option<B256>;

    /// Mint of the deposit transaction
    fn mint(&self) -> Option<u128>;

    /// Whether the transaction is a system transaction
    fn is_system_transaction(&self) -> bool;

    /// Returns `true` if transaction is of type [`DEPOSIT_TRANSACTION_TYPE`].
    fn is_deposit(&self) -> bool {
        self.tx_type() == DEPOSIT_TRANSACTION_TYPE
    }
}

/// Optimism transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OpTransaction<T: Transaction> {
    /// Base transaction fields.
    pub base: T,
    /// An enveloped EIP-2718 typed transaction
    ///
    /// This is used to compute the L1 tx cost using the L1 block info, as
    /// opposed to requiring downstream apps to compute the cost
    /// externally.
    pub enveloped_tx: Option<Bytes>,
    /// Deposit transaction parts.
    pub deposit: DepositTransactionParts,
}

impl<T: Transaction> AsRef<T> for OpTransaction<T> {
    fn as_ref(&self) -> &T {
        &self.base
    }
}

impl<T: Transaction> OpTransaction<T> {
    /// Create a new Optimism transaction.
    pub fn new(base: T) -> Self {
        Self { base, enveloped_tx: None, deposit: DepositTransactionParts::default() }
    }
}

impl Default for OpTransaction<TxEnv> {
    fn default() -> Self {
        Self {
            base: TxEnv::default(),
            enveloped_tx: Some(std::vec![0x00].into()),
            deposit: DepositTransactionParts::default(),
        }
    }
}

impl<TX: Transaction + SystemCallTx> SystemCallTx for OpTransaction<TX> {
    fn new_system_tx_with_caller(
        caller: Address,
        system_contract_address: Address,
        data: Bytes,
    ) -> Self {
        let mut tx =
            Self::new(TX::new_system_tx_with_caller(caller, system_contract_address, data));

        tx.enveloped_tx = Some(Bytes::default());

        tx
    }
}

impl<T: Transaction> Transaction for OpTransaction<T> {
    type AccessListItem<'a>
        = T::AccessListItem<'a>
    where
        T: 'a;
    type Authorization<'a>
        = T::Authorization<'a>
    where
        T: 'a;

    fn tx_type(&self) -> u8 {
        // If this is a deposit transaction (has source_hash set), return deposit type
        if self.deposit.source_hash == B256::ZERO {
            self.base.tx_type()
        } else {
            DEPOSIT_TRANSACTION_TYPE
        }
    }

    fn caller(&self) -> Address {
        self.base.caller()
    }

    fn gas_limit(&self) -> u64 {
        self.base.gas_limit()
    }

    fn value(&self) -> U256 {
        self.base.value()
    }

    fn input(&self) -> &Bytes {
        self.base.input()
    }

    fn nonce(&self) -> u64 {
        self.base.nonce()
    }

    fn kind(&self) -> TxKind {
        self.base.kind()
    }

    fn chain_id(&self) -> Option<u64> {
        self.base.chain_id()
    }

    fn access_list(&self) -> Option<impl Iterator<Item = Self::AccessListItem<'_>>> {
        self.base.access_list()
    }

    fn max_priority_fee_per_gas(&self) -> Option<u128> {
        self.base.max_priority_fee_per_gas()
    }

    fn max_fee_per_gas(&self) -> u128 {
        self.base.max_fee_per_gas()
    }

    fn gas_price(&self) -> u128 {
        self.base.gas_price()
    }

    fn blob_versioned_hashes(&self) -> &[B256] {
        self.base.blob_versioned_hashes()
    }

    fn max_fee_per_blob_gas(&self) -> u128 {
        self.base.max_fee_per_blob_gas()
    }

    fn effective_gas_price(&self, base_fee: u128) -> u128 {
        // Deposit transactions use gas_price directly
        if self.tx_type() == DEPOSIT_TRANSACTION_TYPE {
            return self.gas_price();
        }
        self.base.effective_gas_price(base_fee)
    }

    fn authorization_list_len(&self) -> usize {
        self.base.authorization_list_len()
    }

    fn authorization_list(&self) -> impl Iterator<Item = Self::Authorization<'_>> {
        self.base.authorization_list()
    }
}

impl<T: Transaction> OpTxTr for OpTransaction<T> {
    fn enveloped_tx(&self) -> Option<&Bytes> {
        self.enveloped_tx.as_ref()
    }

    fn source_hash(&self) -> Option<B256> {
        if self.tx_type() != DEPOSIT_TRANSACTION_TYPE {
            return None;
        }
        Some(self.deposit.source_hash)
    }

    fn mint(&self) -> Option<u128> {
        self.deposit.mint
    }

    fn is_system_transaction(&self) -> bool {
        self.deposit.is_system_transaction
    }
}
