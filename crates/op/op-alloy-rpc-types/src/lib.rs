//! Optimism RPC types.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

#[cfg(feature = "arbitrary")]
use arbitrary as _;

mod receipt;
pub use receipt::{L1BlockInfo, OpTransactionReceipt, OpTransactionReceiptFields};

mod transaction;
pub use transaction::{OpTransactionFields, OpTransactionRequest, Transaction};
