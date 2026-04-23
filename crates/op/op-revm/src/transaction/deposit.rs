//! Contains Deposit transaction parts.
use revm::primitives::B256;

/// Deposit transaction type.
pub const DEPOSIT_TRANSACTION_TYPE: u8 = 0x7E;

/// Deposit transaction parts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DepositTransactionParts {
    /// Source hash of the deposit transaction.
    pub source_hash: B256,
    /// Minted value of the deposit transaction.
    pub mint: Option<u128>,
    /// Whether the transaction is a system transaction.
    pub is_system_transaction: bool,
}

impl DepositTransactionParts {
    /// Create a new deposit transaction parts.
    pub const fn new(source_hash: B256, mint: Option<u128>, is_system_transaction: bool) -> Self {
        Self { source_hash, mint, is_system_transaction }
    }
}
