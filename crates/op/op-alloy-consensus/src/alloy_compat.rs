//! Additional compatibility implementations.

#[cfg(not(feature = "std"))]
use alloc::string::ToString;

use crate::{DEPOSIT_TX_TYPE_ID, OpTxEnvelope, TxDeposit};
use alloy_consensus::Sealed;
use alloy_eips::Typed2718;
use alloy_network::{AnyRpcTransaction, AnyTxEnvelope, UnknownTxEnvelope, UnknownTypedTransaction};
use alloy_rpc_types_eth::{ConversionError, Transaction as AlloyRpcTransaction};
use alloy_serde::WithOtherFields;

impl TryFrom<UnknownTxEnvelope> for TxDeposit {
    type Error = ConversionError;

    fn try_from(value: UnknownTxEnvelope) -> Result<Self, Self::Error> {
        value.inner.try_into()
    }
}

impl TryFrom<UnknownTypedTransaction> for TxDeposit {
    type Error = ConversionError;

    fn try_from(value: UnknownTypedTransaction) -> Result<Self, Self::Error> {
        if !value.is_type(DEPOSIT_TX_TYPE_ID) {
            return Err(ConversionError::Custom("invalid transaction type".to_string()));
        }
        value
            .fields
            .deserialize_into()
            .map_err(|_| ConversionError::Custom("invalid transaction data".to_string()))
    }
}

impl TryFrom<AnyTxEnvelope> for OpTxEnvelope {
    type Error = AnyTxEnvelope;

    fn try_from(value: AnyTxEnvelope) -> Result<Self, Self::Error> {
        Self::try_from_any_envelope(value)
    }
}

impl TryFrom<AnyRpcTransaction> for OpTxEnvelope {
    type Error = ConversionError;

    fn try_from(tx: AnyRpcTransaction) -> Result<Self, Self::Error> {
        let WithOtherFields { inner: AlloyRpcTransaction { inner, .. }, other: _ } = tx.0;

        let from = inner.signer();
        match inner.into_inner() {
            AnyTxEnvelope::Ethereum(tx) => Self::try_from_eth_envelope(tx).map_err(|_| {
                ConversionError::Custom("unable to convert from ethereum type".to_string())
            }),
            AnyTxEnvelope::Unknown(mut tx) => {
                // Re-insert `from` field which was consumed by outer `Transaction`.
                // Ref hack in op-alloy <https://github.com/alloy-rs/op-alloy/blob/7d50b698631dd73f8d20f9f60ee78cd0597dc278/crates/rpc-types/src/transaction.rs#L236-L237>
                tx.inner
                    .fields
                    .insert_value("from".to_string(), from)
                    .map_err(|err| ConversionError::Custom(err.to_string()))?;
                Ok(Self::Deposit(Sealed::new(tx.try_into()?)))
            }
        }
    }
}
