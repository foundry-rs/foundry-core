#![doc = include_str!("../README.md")]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[macro_use]
extern crate tracing;

pub mod backend;
pub mod cache;
pub mod error;

pub use backend::{BackendHandler, ForkBlock, SharedBackend};
pub use cache::{AccountFetchPolicy, BlockchainDb, ForkBlockEnv};
pub use error::{DatabaseError, DatabaseResult};

pub use evm2::{bytecode::Bytecode, evm::AccountInfo};

#[cfg(test)]
mod test_utils {
    #[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    pub struct BlockEnv {
        pub number: alloy_primitives::U256,
    }
}
