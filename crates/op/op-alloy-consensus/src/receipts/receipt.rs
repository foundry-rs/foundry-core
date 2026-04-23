//! Optimism receipt type for execution and storage.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use core::fmt::Debug;

use super::{OpDepositReceipt, OpTxReceipt};
use crate::{OpReceiptEnvelope, OpTxType};
use alloy_consensus::{
    Eip658Value, Eip2718DecodableReceipt, Eip2718EncodableReceipt, Receipt, ReceiptWithBloom,
    RlpDecodableReceipt, RlpEncodableReceipt, TxReceipt, Typed2718,
};
use alloy_eips::eip2718::{Eip2718Error, Eip2718Result, IsTyped2718};
use alloy_primitives::{Bloom, Log};
use alloy_rlp::{Buf, BufMut, Decodable, Encodable, Header};

/// Typed Optimism transaction receipt.
///
/// Receipt containing result of transaction execution.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
pub enum OpReceipt<T = Log> {
    /// Legacy receipt
    #[cfg_attr(feature = "serde", serde(rename = "0x0", alias = "0x00"))]
    Legacy(Receipt<T>),
    /// EIP-2930 receipt
    #[cfg_attr(feature = "serde", serde(rename = "0x1", alias = "0x01"))]
    Eip2930(Receipt<T>),
    /// EIP-1559 receipt
    #[cfg_attr(feature = "serde", serde(rename = "0x2", alias = "0x02"))]
    Eip1559(Receipt<T>),
    /// EIP-7702 receipt
    #[cfg_attr(feature = "serde", serde(rename = "0x4", alias = "0x04"))]
    Eip7702(Receipt<T>),
    /// Post-exec receipt
    #[cfg_attr(feature = "serde", serde(rename = "0x7d", alias = "0x7D"))]
    PostExec(Receipt<T>),
    /// Deposit receipt
    #[cfg_attr(feature = "serde", serde(rename = "0x7e", alias = "0x7E"))]
    Deposit(OpDepositReceipt<T>),
}

impl<T> OpReceipt<T> {
    /// Returns [`OpTxType`] of the receipt.
    pub const fn tx_type(&self) -> OpTxType {
        match self {
            Self::Legacy(_) => OpTxType::Legacy,
            Self::Eip2930(_) => OpTxType::Eip2930,
            Self::Eip1559(_) => OpTxType::Eip1559,
            Self::Eip7702(_) => OpTxType::Eip7702,
            Self::PostExec(_) => OpTxType::PostExec,
            Self::Deposit(_) => OpTxType::Deposit,
        }
    }

    /// Returns inner [`Receipt`].
    pub const fn as_receipt(&self) -> &Receipt<T> {
        match self {
            Self::Legacy(receipt)
            | Self::Eip2930(receipt)
            | Self::Eip1559(receipt)
            | Self::Eip7702(receipt)
            | Self::PostExec(receipt) => receipt,
            Self::Deposit(receipt) => &receipt.inner,
        }
    }

    /// Returns a mutable reference to the inner [`Receipt`].
    pub const fn as_receipt_mut(&mut self) -> &mut Receipt<T> {
        match self {
            Self::Legacy(receipt)
            | Self::Eip2930(receipt)
            | Self::Eip1559(receipt)
            | Self::Eip7702(receipt)
            | Self::PostExec(receipt) => receipt,
            Self::Deposit(receipt) => &mut receipt.inner,
        }
    }

    /// Consumes this and returns the inner [`Receipt`].
    pub fn into_receipt(self) -> Receipt<T> {
        match self {
            Self::Legacy(receipt)
            | Self::Eip2930(receipt)
            | Self::Eip1559(receipt)
            | Self::Eip7702(receipt)
            | Self::PostExec(receipt) => receipt,
            Self::Deposit(receipt) => receipt.inner,
        }
    }

    /// Converts the receipt's log type by applying a function to each log.
    ///
    /// Returns the receipt with the new log type
    pub fn map_logs<U>(self, f: impl FnMut(T) -> U) -> OpReceipt<U> {
        match self {
            Self::Legacy(receipt) => OpReceipt::Legacy(receipt.map_logs(f)),
            Self::Eip2930(receipt) => OpReceipt::Eip2930(receipt.map_logs(f)),
            Self::Eip1559(receipt) => OpReceipt::Eip1559(receipt.map_logs(f)),
            Self::Eip7702(receipt) => OpReceipt::Eip7702(receipt.map_logs(f)),
            Self::PostExec(receipt) => OpReceipt::PostExec(receipt.map_logs(f)),
            Self::Deposit(receipt) => OpReceipt::Deposit(receipt.map_logs(f)),
        }
    }

    /// Returns length of RLP-encoded receipt fields with the given [`Bloom`] without an RLP header.
    pub fn rlp_encoded_fields_length(&self, bloom: &Bloom) -> usize
    where
        T: Encodable,
    {
        match self {
            Self::Legacy(receipt)
            | Self::Eip2930(receipt)
            | Self::Eip1559(receipt)
            | Self::Eip7702(receipt)
            | Self::PostExec(receipt) => receipt.rlp_encoded_fields_length_with_bloom(bloom),
            Self::Deposit(receipt) => receipt.rlp_encoded_fields_length_with_bloom(bloom),
        }
    }

    /// RLP-encodes receipt fields with the given [`Bloom`] without an RLP header.
    pub fn rlp_encode_fields(&self, bloom: &Bloom, out: &mut dyn BufMut)
    where
        T: Encodable,
    {
        match self {
            Self::Legacy(receipt)
            | Self::Eip2930(receipt)
            | Self::Eip1559(receipt)
            | Self::Eip7702(receipt)
            | Self::PostExec(receipt) => receipt.rlp_encode_fields_with_bloom(bloom, out),
            Self::Deposit(receipt) => receipt.rlp_encode_fields_with_bloom(bloom, out),
        }
    }

    /// Returns RLP header for inner encoding.
    pub fn rlp_header_inner(&self, bloom: &Bloom) -> Header
    where
        T: Encodable,
    {
        Header { list: true, payload_length: self.rlp_encoded_fields_length(bloom) }
    }

    /// Returns RLP header for inner encoding without bloom.
    pub fn rlp_header_without_bloom(&self) -> Header
    where
        T: Encodable,
    {
        Header { list: true, payload_length: self.rlp_encoded_fields_length_without_bloom() }
    }

    /// RLP-decodes the receipt from the provided buffer. This does not expect a type byte or
    /// network header.
    pub fn rlp_decode_inner(
        buf: &mut &[u8],
        tx_type: OpTxType,
    ) -> alloy_rlp::Result<ReceiptWithBloom<Self>>
    where
        T: Decodable,
    {
        match tx_type {
            OpTxType::Legacy => {
                let ReceiptWithBloom { receipt, logs_bloom } =
                    RlpDecodableReceipt::rlp_decode_with_bloom(buf)?;
                Ok(ReceiptWithBloom { receipt: Self::Legacy(receipt), logs_bloom })
            }
            OpTxType::Eip2930 => {
                let ReceiptWithBloom { receipt, logs_bloom } =
                    RlpDecodableReceipt::rlp_decode_with_bloom(buf)?;
                Ok(ReceiptWithBloom { receipt: Self::Eip2930(receipt), logs_bloom })
            }
            OpTxType::Eip1559 => {
                let ReceiptWithBloom { receipt, logs_bloom } =
                    RlpDecodableReceipt::rlp_decode_with_bloom(buf)?;
                Ok(ReceiptWithBloom { receipt: Self::Eip1559(receipt), logs_bloom })
            }
            OpTxType::Eip7702 => {
                let ReceiptWithBloom { receipt, logs_bloom } =
                    RlpDecodableReceipt::rlp_decode_with_bloom(buf)?;
                Ok(ReceiptWithBloom { receipt: Self::Eip7702(receipt), logs_bloom })
            }
            OpTxType::PostExec => {
                let ReceiptWithBloom { receipt, logs_bloom } =
                    RlpDecodableReceipt::rlp_decode_with_bloom(buf)?;
                Ok(ReceiptWithBloom { receipt: Self::PostExec(receipt), logs_bloom })
            }
            OpTxType::Deposit => {
                let ReceiptWithBloom { receipt, logs_bloom } =
                    RlpDecodableReceipt::rlp_decode_with_bloom(buf)?;
                Ok(ReceiptWithBloom { receipt: Self::Deposit(receipt), logs_bloom })
            }
        }
    }

    /// RLP-encodes receipt fields without an RLP header.
    pub fn rlp_encode_fields_without_bloom(&self, out: &mut dyn BufMut)
    where
        T: Encodable,
    {
        self.tx_type().encode(out);
        match self {
            Self::Legacy(receipt)
            | Self::Eip2930(receipt)
            | Self::Eip1559(receipt)
            | Self::Eip7702(receipt)
            | Self::PostExec(receipt) => {
                receipt.status.encode(out);
                receipt.cumulative_gas_used.encode(out);
                receipt.logs.encode(out);
            }
            Self::Deposit(receipt) => {
                receipt.inner.status.encode(out);
                receipt.inner.cumulative_gas_used.encode(out);
                receipt.inner.logs.encode(out);
                if let Some(nonce) = receipt.deposit_nonce {
                    nonce.encode(out);
                }
                if let Some(version) = receipt.deposit_receipt_version {
                    version.encode(out);
                }
            }
        }
    }

    /// Returns length of RLP-encoded receipt fields without an RLP header.
    pub fn rlp_encoded_fields_length_without_bloom(&self) -> usize
    where
        T: Encodable,
    {
        self.tx_type().length()
            + match self {
                Self::Legacy(receipt)
                | Self::Eip2930(receipt)
                | Self::Eip1559(receipt)
                | Self::Eip7702(receipt)
                | Self::PostExec(receipt) => {
                    receipt.status.length()
                        + receipt.cumulative_gas_used.length()
                        + receipt.logs.length()
                }
                Self::Deposit(receipt) => {
                    receipt.inner.status.length()
                        + receipt.inner.cumulative_gas_used.length()
                        + receipt.inner.logs.length()
                        + receipt.deposit_nonce.map_or(0, |nonce| nonce.length())
                        + receipt.deposit_receipt_version.map_or(0, |version| version.length())
                }
            }
    }

    /// RLP-decodes the receipt from the provided buffer without bloom.
    pub fn rlp_decode_fields_without_bloom(buf: &mut &[u8]) -> alloy_rlp::Result<Self>
    where
        T: Decodable,
    {
        let tx_type = OpTxType::decode(buf)?;
        let status = Decodable::decode(buf)?;
        let cumulative_gas_used = Decodable::decode(buf)?;
        let logs = Decodable::decode(buf)?;

        let mut deposit_nonce = None;
        let mut deposit_receipt_version = None;

        // For deposit receipts, try to decode nonce and version if they exist
        if tx_type == OpTxType::Deposit && !buf.is_empty() {
            deposit_nonce = Some(Decodable::decode(buf)?);
            if !buf.is_empty() {
                deposit_receipt_version = Some(Decodable::decode(buf)?);
            }
        }

        match tx_type {
            OpTxType::Legacy => Ok(Self::Legacy(Receipt { status, cumulative_gas_used, logs })),
            OpTxType::Eip2930 => Ok(Self::Eip2930(Receipt { status, cumulative_gas_used, logs })),
            OpTxType::Eip1559 => Ok(Self::Eip1559(Receipt { status, cumulative_gas_used, logs })),
            OpTxType::Eip7702 => Ok(Self::Eip7702(Receipt { status, cumulative_gas_used, logs })),
            OpTxType::PostExec => Ok(Self::PostExec(Receipt { status, cumulative_gas_used, logs })),
            OpTxType::Deposit => Ok(Self::Deposit(OpDepositReceipt {
                inner: Receipt { status, cumulative_gas_used, logs },
                deposit_nonce,
                deposit_receipt_version,
            })),
        }
    }
}

impl<T: Encodable> Eip2718EncodableReceipt for OpReceipt<T> {
    fn eip2718_encoded_length_with_bloom(&self, bloom: &Bloom) -> usize {
        !self.tx_type().is_legacy() as usize + self.rlp_header_inner(bloom).length_with_payload()
    }

    fn eip2718_encode_with_bloom(&self, bloom: &Bloom, out: &mut dyn BufMut) {
        if !self.tx_type().is_legacy() {
            out.put_u8(self.tx_type() as u8);
        }
        self.rlp_header_inner(bloom).encode(out);
        self.rlp_encode_fields(bloom, out);
    }
}

impl<T: Decodable> Eip2718DecodableReceipt for OpReceipt<T> {
    fn typed_decode_with_bloom(ty: u8, buf: &mut &[u8]) -> Eip2718Result<ReceiptWithBloom<Self>> {
        let tx_type = OpTxType::try_from(ty).map_err(|_| Eip2718Error::UnexpectedType(ty))?;
        Ok(Self::rlp_decode_inner(buf, tx_type)?)
    }

    fn fallback_decode_with_bloom(buf: &mut &[u8]) -> Eip2718Result<ReceiptWithBloom<Self>> {
        Ok(Self::rlp_decode_inner(buf, OpTxType::Legacy)?)
    }
}

impl<T: Encodable> RlpEncodableReceipt for OpReceipt<T> {
    fn rlp_encoded_length_with_bloom(&self, bloom: &Bloom) -> usize {
        let mut len = self.eip2718_encoded_length_with_bloom(bloom);
        if !self.tx_type().is_legacy() {
            len += Header {
                list: false,
                payload_length: self.eip2718_encoded_length_with_bloom(bloom),
            }
            .length();
        }

        len
    }

    fn rlp_encode_with_bloom(&self, bloom: &Bloom, out: &mut dyn BufMut) {
        if !self.tx_type().is_legacy() {
            Header { list: false, payload_length: self.eip2718_encoded_length_with_bloom(bloom) }
                .encode(out);
        }
        self.eip2718_encode_with_bloom(bloom, out);
    }
}

impl<T: Decodable> RlpDecodableReceipt for OpReceipt<T> {
    fn rlp_decode_with_bloom(buf: &mut &[u8]) -> alloy_rlp::Result<ReceiptWithBloom<Self>> {
        let header_buf = &mut &**buf;
        let header = Header::decode(header_buf)?;

        // Legacy receipt, reuse initial buffer without advancing
        if header.list {
            return Self::rlp_decode_inner(buf, OpTxType::Legacy);
        }

        // Otherwise, advance the buffer and try decoding type flag followed by receipt
        *buf = *header_buf;

        let remaining = buf.len();
        let tx_type = OpTxType::decode(buf)?;
        let this = Self::rlp_decode_inner(buf, tx_type)?;

        if buf.len() + header.payload_length != remaining {
            return Err(alloy_rlp::Error::UnexpectedLength);
        }

        Ok(this)
    }
}

impl<T: Encodable + Send + Sync> Encodable for OpReceipt<T> {
    fn encode(&self, out: &mut dyn BufMut) {
        self.rlp_header_without_bloom().encode(out);
        self.rlp_encode_fields_without_bloom(out);
    }

    fn length(&self) -> usize {
        self.rlp_header_without_bloom().length_with_payload()
    }
}

impl<T: Decodable> Decodable for OpReceipt<T> {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let header = Header::decode(buf)?;
        if !header.list {
            return Err(alloy_rlp::Error::UnexpectedString);
        }

        if buf.len() < header.payload_length {
            return Err(alloy_rlp::Error::InputTooShort);
        }
        let mut fields_buf = &buf[..header.payload_length];
        let this = Self::rlp_decode_fields_without_bloom(&mut fields_buf)?;

        if !fields_buf.is_empty() {
            return Err(alloy_rlp::Error::UnexpectedLength);
        }

        buf.advance(header.payload_length);

        Ok(this)
    }
}

impl<T: Send + Sync + Clone + Debug + Eq + AsRef<Log>> TxReceipt for OpReceipt<T> {
    type Log = T;

    fn status_or_post_state(&self) -> Eip658Value {
        self.as_receipt().status_or_post_state()
    }

    fn status(&self) -> bool {
        self.as_receipt().status()
    }

    fn bloom(&self) -> Bloom {
        self.as_receipt().bloom()
    }

    fn cumulative_gas_used(&self) -> u64 {
        self.as_receipt().cumulative_gas_used()
    }

    fn logs(&self) -> &[Self::Log] {
        self.as_receipt().logs()
    }

    fn into_logs(self) -> Vec<Self::Log> {
        match self {
            Self::Legacy(receipt)
            | Self::Eip2930(receipt)
            | Self::Eip1559(receipt)
            | Self::Eip7702(receipt)
            | Self::PostExec(receipt) => receipt.logs,
            Self::Deposit(receipt) => receipt.inner.logs,
        }
    }
}

impl<T> Typed2718 for OpReceipt<T> {
    fn ty(&self) -> u8 {
        self.tx_type().into()
    }
}

impl<T> IsTyped2718 for OpReceipt<T> {
    fn is_type(type_id: u8) -> bool {
        <OpTxType as IsTyped2718>::is_type(type_id)
    }
}

impl<T: Send + Sync + Clone + Debug + Eq + AsRef<Log>> OpTxReceipt for OpReceipt<T> {
    fn deposit_nonce(&self) -> Option<u64> {
        match self {
            Self::Deposit(receipt) => receipt.deposit_nonce,
            _ => None,
        }
    }

    fn deposit_receipt_version(&self) -> Option<u64> {
        match self {
            Self::Deposit(receipt) => receipt.deposit_receipt_version,
            _ => None,
        }
    }
}

impl From<super::OpReceiptEnvelope> for OpReceipt {
    fn from(envelope: super::OpReceiptEnvelope) -> Self {
        match envelope {
            super::OpReceiptEnvelope::Legacy(receipt) => Self::Legacy(receipt.receipt),
            super::OpReceiptEnvelope::Eip2930(receipt) => Self::Eip2930(receipt.receipt),
            super::OpReceiptEnvelope::Eip1559(receipt) => Self::Eip1559(receipt.receipt),
            super::OpReceiptEnvelope::Eip7702(receipt) => Self::Eip7702(receipt.receipt),
            super::OpReceiptEnvelope::PostExec(receipt) => Self::PostExec(receipt.receipt),
            super::OpReceiptEnvelope::Deposit(receipt) => Self::Deposit(OpDepositReceipt {
                deposit_nonce: receipt.receipt.deposit_nonce,
                deposit_receipt_version: receipt.receipt.deposit_receipt_version,
                inner: receipt.receipt.inner,
            }),
        }
    }
}

impl<T> From<ReceiptWithBloom<OpReceipt<T>>> for OpReceiptEnvelope<T> {
    fn from(value: ReceiptWithBloom<OpReceipt<T>>) -> Self {
        let (receipt, logs_bloom) = value.into_components();
        match receipt {
            OpReceipt::Legacy(receipt) => Self::Legacy(ReceiptWithBloom { receipt, logs_bloom }),
            OpReceipt::Eip2930(receipt) => Self::Eip2930(ReceiptWithBloom { receipt, logs_bloom }),
            OpReceipt::Eip1559(receipt) => Self::Eip1559(ReceiptWithBloom { receipt, logs_bloom }),
            OpReceipt::Eip7702(receipt) => Self::Eip7702(ReceiptWithBloom { receipt, logs_bloom }),
            OpReceipt::PostExec(receipt) => {
                Self::PostExec(ReceiptWithBloom { receipt, logs_bloom })
            }
            OpReceipt::Deposit(receipt) => Self::Deposit(ReceiptWithBloom { receipt, logs_bloom }),
        }
    }
}
