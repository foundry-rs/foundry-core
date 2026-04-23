//! Transaction types for Optimism.

mod deposit;
pub use deposit::{DepositTransaction, TxDeposit};

mod tx_type;
pub use tx_type::DEPOSIT_TX_TYPE_ID;

mod envelope;
pub use envelope::{OpTransaction, OpTxEnvelope, OpTxType};

mod typed;
pub use typed::OpTypedTransaction;

#[cfg(feature = "serde")]
pub use deposit::serde_deposit_tx_rpc;

mod meta;
pub use meta::{OpDepositInfo, OpTransactionInfo};
