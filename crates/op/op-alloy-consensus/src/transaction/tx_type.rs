//! Contains the transaction type identifier for Optimism.

use crate::transaction::envelope::OpTxType;
use core::fmt::Display;

/// Identifier for an Optimism deposit transaction
pub const DEPOSIT_TX_TYPE_ID: u8 = 126; // 0x7E

#[allow(clippy::derivable_impls)]
impl Default for OpTxType {
    fn default() -> Self {
        Self::Legacy
    }
}

impl Display for OpTxType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Legacy => write!(f, "legacy"),
            Self::Eip2930 => write!(f, "eip2930"),
            Self::Eip1559 => write!(f, "eip1559"),
            Self::Eip7702 => write!(f, "eip7702"),
            Self::Deposit => write!(f, "deposit"),
            Self::PostExec => write!(f, "post-exec"),
        }
    }
}

impl OpTxType {
    /// List of all variants.
    pub const ALL: [Self; 6] =
        [Self::Legacy, Self::Eip2930, Self::Eip1559, Self::Eip7702, Self::Deposit, Self::PostExec];

    /// Returns `true` if the type is [`OpTxType::Deposit`].
    pub const fn is_deposit(&self) -> bool {
        matches!(self, Self::Deposit)
    }
}
