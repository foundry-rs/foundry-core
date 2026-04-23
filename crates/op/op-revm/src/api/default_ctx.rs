//! Contains trait [`DefaultOp`] used to create a default context.
use crate::{L1BlockInfo, OpSpecId, OpTransaction};
use revm::{
    Context, Journal, MainContext,
    context::{BlockEnv, CfgEnv, TxEnv},
    database_interface::EmptyDB,
};

/// Type alias for the default context type of the `OpEvm`.
pub type OpContext<DB> =
    Context<BlockEnv, OpTransaction<TxEnv>, CfgEnv<OpSpecId>, DB, Journal<DB>, L1BlockInfo>;

/// Trait that allows for a default context to be created.
pub trait DefaultOp {
    /// Create a default context.
    fn op() -> OpContext<EmptyDB>;
}

impl DefaultOp for OpContext<EmptyDB> {
    fn op() -> Self {
        Context::mainnet()
            .with_tx(OpTransaction::default())
            .with_cfg(CfgEnv::new_with_spec(OpSpecId::BEDROCK))
            .with_chain(L1BlockInfo::default())
    }
}
