//! Optimism consensus types.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

#[cfg(feature = "alloy-compat")]
mod alloy_compat;

mod receipts;
pub use receipts::{
    OpDepositReceipt, OpDepositReceiptWithBloom, OpReceipt, OpReceiptEnvelope, OpTxReceipt,
};

pub mod transaction;
pub use transaction::{
    DEPOSIT_TX_TYPE_ID, DepositTransaction, OpTransaction, OpTxEnvelope, OpTxType,
    OpTypedTransaction, TxDeposit,
};

pub mod post_exec;
pub use post_exec::*;

#[cfg(feature = "serde")]
pub use transaction::serde_deposit_tx_rpc;
