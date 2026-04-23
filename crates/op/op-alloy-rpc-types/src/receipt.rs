//! Receipt types for RPC

use alloy_consensus::{Receipt, ReceiptWithBloom, TxReceipt};
use alloy_rpc_types_eth::Log;
use alloy_serde::OtherFields;
use op_alloy_consensus::{
    OpDepositReceipt, OpDepositReceiptWithBloom, OpReceipt, OpReceiptEnvelope,
};
use serde::{Deserialize, Serialize};

/// OP Transaction Receipt type
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[doc(alias = "OpTxReceipt")]
pub struct OpTransactionReceipt {
    /// Regular eth transaction receipt including deposit receipts
    #[serde(flatten)]
    pub inner: alloy_rpc_types_eth::TransactionReceipt<ReceiptWithBloom<OpReceipt<Log>>>,
    /// L1 block info of the transaction.
    #[serde(flatten)]
    pub l1_block_info: L1BlockInfo,
    /// Per-transaction gas refund from post-exec block-level warming.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "alloy_serde::quantity::opt")]
    pub op_gas_refund: Option<u64>,
}

impl alloy_network_primitives::ReceiptResponse for OpTransactionReceipt {
    fn contract_address(&self) -> Option<alloy_primitives::Address> {
        self.inner.contract_address
    }

    fn status(&self) -> bool {
        self.inner.inner.status()
    }

    fn block_hash(&self) -> Option<alloy_primitives::BlockHash> {
        self.inner.block_hash
    }

    fn block_number(&self) -> Option<u64> {
        self.inner.block_number
    }

    fn transaction_hash(&self) -> alloy_primitives::TxHash {
        self.inner.transaction_hash
    }

    fn transaction_index(&self) -> Option<u64> {
        self.inner.transaction_index()
    }

    fn gas_used(&self) -> u64 {
        self.inner.gas_used()
    }

    fn effective_gas_price(&self) -> u128 {
        self.inner.effective_gas_price()
    }

    fn blob_gas_used(&self) -> Option<u64> {
        self.inner.blob_gas_used()
    }

    fn blob_gas_price(&self) -> Option<u128> {
        self.inner.blob_gas_price()
    }

    fn from(&self) -> alloy_primitives::Address {
        self.inner.from()
    }

    fn to(&self) -> Option<alloy_primitives::Address> {
        self.inner.to()
    }

    fn cumulative_gas_used(&self) -> u64 {
        self.inner.cumulative_gas_used()
    }

    fn state_root(&self) -> Option<alloy_primitives::B256> {
        self.inner.state_root()
    }
}

/// Additional fields for Optimism transaction receipts: <https://github.com/ethereum-optimism/op-geth/blob/f2e69450c6eec9c35d56af91389a1c47737206ca/core/types/receipt.go#L87-L87>
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[doc(alias = "OptimismTxReceiptFields")]
pub struct OpTransactionReceiptFields {
    /// L1 block info.
    #[serde(flatten)]
    pub l1_block_info: L1BlockInfo,
    /// Per-transaction gas refund from post-exec block-level warming.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "alloy_serde::quantity::opt")]
    pub op_gas_refund: Option<u64>,
    /* --------------------------------------- Regolith --------------------------------------- */
    /// Deposit nonce for deposit transactions.
    ///
    /// Always null prior to the Regolith hardfork.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "alloy_serde::quantity::opt")]
    pub deposit_nonce: Option<u64>,
    /* ---------------------------------------- Canyon ---------------------------------------- */
    /// Deposit receipt version for deposit transactions.
    ///
    /// Always null prior to the Canyon hardfork.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "alloy_serde::quantity::opt")]
    pub deposit_receipt_version: Option<u64>,
}

/// Serialize/Deserialize l1FeeScalar to/from string
mod l1_fee_scalar_serde {
    #[cfg(not(feature = "std"))]
    use alloc::string::{String, ToString};

    use serde::{Deserialize, de};

    pub(super) fn serialize<S>(value: &Option<f64>, s: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if let Some(v) = value {
            return s.serialize_str(&v.to_string());
        }
        s.serialize_none()
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s: Option<String> = Option::deserialize(deserializer)?;
        if let Some(s) = s {
            return Ok(Some(s.parse::<f64>().map_err(de::Error::custom)?));
        }

        Ok(None)
    }
}

impl From<OpTransactionReceiptFields> for OtherFields {
    fn from(value: OpTransactionReceiptFields) -> Self {
        serde_json::to_value(value).unwrap().try_into().unwrap()
    }
}

/// L1 block info extracted from input of first transaction in every block.
///
/// The subset of [`OpTransactionReceiptFields`], that encompasses L1 block
/// info:
/// <https://github.com/ethereum-optimism/op-geth/blob/f2e69450c6eec9c35d56af91389a1c47737206ca/core/types/receipt.go#L87-L87>
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct L1BlockInfo {
    /// L1 base fee is the minimum price per unit of gas.
    ///
    /// Present from pre-bedrock as de facto L1 price per unit of gas. L1 base fee after Bedrock.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "alloy_serde::quantity::opt")]
    pub l1_gas_price: Option<u128>,
    /// L1 gas used.
    ///
    /// Present from pre-bedrock, deprecated as of Fjord.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "alloy_serde::quantity::opt")]
    pub l1_gas_used: Option<u128>,
    /// L1 fee for the transaction.
    ///
    /// Present from pre-bedrock.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "alloy_serde::quantity::opt")]
    pub l1_fee: Option<u128>,
    /// L1 fee scalar for the transaction
    ///
    /// Present from pre-bedrock to Ecotone. Null after Ecotone.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "l1_fee_scalar_serde")]
    pub l1_fee_scalar: Option<f64>,
    /* ---------------------------------------- Ecotone ---------------------------------------- */
    /// L1 base fee scalar. Applied to base fee to compute weighted gas price multiplier.
    ///
    /// Always null prior to the Ecotone hardfork.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "alloy_serde::quantity::opt")]
    pub l1_base_fee_scalar: Option<u128>,
    /// L1 blob base fee.
    ///
    /// Always null prior to the Ecotone hardfork.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "alloy_serde::quantity::opt")]
    pub l1_blob_base_fee: Option<u128>,
    /// L1 blob base fee scalar. Applied to blob base fee to compute weighted gas price multiplier.
    ///
    /// Always null prior to the Ecotone hardfork.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "alloy_serde::quantity::opt")]
    pub l1_blob_base_fee_scalar: Option<u128>,
    /* ---------------------------------------- Isthmus ---------------------------------------- */
    /// Operator fee scalar.
    ///
    /// Always null prior to the Isthmus hardfork.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "alloy_serde::quantity::opt")]
    pub operator_fee_scalar: Option<u128>,
    /// Operator fee constant.
    ///
    /// Always null prior to the Isthmus hardfork.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "alloy_serde::quantity::opt")]
    pub operator_fee_constant: Option<u128>,
    /* ---------------------------------------- Jovian ---------------------------------------- */
    /// DA footprint gas scalar. Used to set the DA footprint block limit on the L2.
    ///
    /// Always null prior to the Jovian hardfork.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "alloy_serde::quantity::opt")]
    pub da_footprint_gas_scalar: Option<u16>,
}

impl Eq for L1BlockInfo {}

impl From<OpTransactionReceipt> for OpReceiptEnvelope<alloy_primitives::Log> {
    fn from(value: OpTransactionReceipt) -> Self {
        let inner_envelope = value.inner.inner.into();

        /// Helper function to convert the inner logs within a [`ReceiptWithBloom`] from RPC to
        /// consensus types.
        #[inline(always)]
        fn convert_standard_receipt(
            receipt: ReceiptWithBloom<Receipt<alloy_rpc_types_eth::Log>>,
        ) -> ReceiptWithBloom<Receipt<alloy_primitives::Log>> {
            let ReceiptWithBloom { logs_bloom, receipt } = receipt;

            let consensus_logs = receipt.logs.into_iter().map(|log| log.inner).collect();
            ReceiptWithBloom {
                receipt: Receipt {
                    status: receipt.status,
                    cumulative_gas_used: receipt.cumulative_gas_used,
                    logs: consensus_logs,
                },
                logs_bloom,
            }
        }

        match inner_envelope {
            OpReceiptEnvelope::Legacy(receipt) => Self::Legacy(convert_standard_receipt(receipt)),
            OpReceiptEnvelope::Eip2930(receipt) => Self::Eip2930(convert_standard_receipt(receipt)),
            OpReceiptEnvelope::Eip1559(receipt) => Self::Eip1559(convert_standard_receipt(receipt)),
            OpReceiptEnvelope::Eip7702(receipt) => Self::Eip7702(convert_standard_receipt(receipt)),
            OpReceiptEnvelope::PostExec(receipt) => {
                Self::PostExec(convert_standard_receipt(receipt))
            }
            OpReceiptEnvelope::Deposit(OpDepositReceiptWithBloom { logs_bloom, receipt }) => {
                let consensus_logs = receipt.inner.logs.into_iter().map(|log| log.inner).collect();
                let consensus_receipt = OpDepositReceiptWithBloom {
                    receipt: OpDepositReceipt {
                        inner: Receipt {
                            status: receipt.inner.status,
                            cumulative_gas_used: receipt.inner.cumulative_gas_used,
                            logs: consensus_logs,
                        },
                        deposit_nonce: receipt.deposit_nonce,
                        deposit_receipt_version: receipt.deposit_receipt_version,
                    },
                    logs_bloom,
                };
                Self::Deposit(consensus_receipt)
            }
        }
    }
}
