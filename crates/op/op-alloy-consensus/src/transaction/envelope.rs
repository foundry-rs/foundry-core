#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::{
    TxDeposit, TxPostExec,
    transaction::{OpDepositInfo, OpTransactionInfo},
};
use alloy_consensus::{
    EthereumTxEnvelope, Extended, Sealable, Sealed, SignableTransaction, Signed,
    TransactionEnvelope, TxEip1559, TxEip2930, TxEip7702, TxEnvelope, TxLegacy,
    error::ValueError,
    transaction::{TransactionInfo, TxHashRef},
};
use alloy_eips::eip2718::Encodable2718;
use alloy_primitives::{B256, Bytes, Signature, TxHash};

/// The Ethereum [EIP-2718] Transaction Envelope, modified for OP Stack chains.
///
/// # Note:
///
/// This enum distinguishes between tagged and untagged legacy transactions, as
/// the in-protocol merkle tree may commit to EITHER 0-prefixed or raw.
/// Therefore we must ensure that encoding returns the precise byte-array that
/// was decoded, preserving the presence or absence of the `TransactionType`
/// flag.
///
/// [EIP-2718]: https://eips.ethereum.org/EIPS/eip-2718
#[derive(Debug, Clone, TransactionEnvelope)]
#[envelope(tx_type_name = OpTxType, typed = OpTypedTransaction, serde_cfg(feature = "serde"))]
pub enum OpTxEnvelope {
    /// An untagged [`TxLegacy`].
    #[envelope(ty = 0)]
    Legacy(Signed<TxLegacy>),
    /// A [`TxEip2930`] tagged with type 1.
    #[envelope(ty = 1)]
    Eip2930(Signed<TxEip2930>),
    /// A [`TxEip1559`] tagged with type 2.
    #[envelope(ty = 2)]
    Eip1559(Signed<TxEip1559>),
    /// A [`TxEip7702`] tagged with type 4.
    #[envelope(ty = 4)]
    Eip7702(Signed<TxEip7702>),
    /// A [`TxDeposit`] tagged with type 0x7E.
    #[envelope(ty = 126)]
    #[serde(serialize_with = "crate::serde_deposit_tx_rpc")]
    Deposit(Sealed<TxDeposit>),
    /// A [`TxPostExec`] tagged with type 0x7D.
    #[envelope(ty = 0x7D)]
    #[serde(serialize_with = "crate::post_exec::serde_post_exec_tx_rpc")]
    PostExec(Sealed<TxPostExec>),
}

/// Represents an Optimism transaction envelope.
///
/// Compared to Ethereum it can tell whether the transaction is a deposit or post-exec synthetic
/// transaction.
pub trait OpTransaction {
    /// Returns `true` if the transaction is a deposit.
    fn is_deposit(&self) -> bool;

    /// Returns `Some` if the transaction is a deposit.
    fn as_deposit(&self) -> Option<&Sealed<TxDeposit>>;

    /// Returns `Some` if the transaction is a post-exec transaction.
    fn as_post_exec(&self) -> Option<&Sealed<TxPostExec>>;
}

impl OpTransaction for OpTxEnvelope {
    fn is_deposit(&self) -> bool {
        self.is_deposit()
    }

    fn as_deposit(&self) -> Option<&Sealed<TxDeposit>> {
        self.as_deposit()
    }

    fn as_post_exec(&self) -> Option<&Sealed<TxPostExec>> {
        self.as_post_exec()
    }
}

impl<B, T> OpTransaction for Extended<B, T>
where
    B: OpTransaction,
    T: OpTransaction,
{
    fn is_deposit(&self) -> bool {
        match self {
            Self::BuiltIn(b) => b.is_deposit(),
            Self::Other(t) => t.is_deposit(),
        }
    }

    fn as_deposit(&self) -> Option<&Sealed<TxDeposit>> {
        match self {
            Self::BuiltIn(b) => b.as_deposit(),
            Self::Other(t) => t.as_deposit(),
        }
    }

    fn as_post_exec(&self) -> Option<&Sealed<TxPostExec>> {
        match self {
            Self::BuiltIn(b) => b.as_post_exec(),
            Self::Other(t) => t.as_post_exec(),
        }
    }
}

impl AsRef<Self> for OpTxEnvelope {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl From<Signed<TxLegacy>> for OpTxEnvelope {
    fn from(v: Signed<TxLegacy>) -> Self {
        Self::Legacy(v)
    }
}

impl From<Signed<TxEip2930>> for OpTxEnvelope {
    fn from(v: Signed<TxEip2930>) -> Self {
        Self::Eip2930(v)
    }
}

impl From<Signed<TxEip1559>> for OpTxEnvelope {
    fn from(v: Signed<TxEip1559>) -> Self {
        Self::Eip1559(v)
    }
}

impl From<Signed<TxEip7702>> for OpTxEnvelope {
    fn from(v: Signed<TxEip7702>) -> Self {
        Self::Eip7702(v)
    }
}

impl From<TxDeposit> for OpTxEnvelope {
    fn from(v: TxDeposit) -> Self {
        v.seal_slow().into()
    }
}

impl From<Signed<OpTypedTransaction>> for OpTxEnvelope {
    fn from(value: Signed<OpTypedTransaction>) -> Self {
        let (tx, sig, hash) = value.into_parts();
        match tx {
            OpTypedTransaction::Legacy(tx_legacy) => {
                let tx = Signed::new_unchecked(tx_legacy, sig, hash);
                Self::Legacy(tx)
            }
            OpTypedTransaction::Eip2930(tx_eip2930) => {
                let tx = Signed::new_unchecked(tx_eip2930, sig, hash);
                Self::Eip2930(tx)
            }
            OpTypedTransaction::Eip1559(tx_eip1559) => {
                let tx = Signed::new_unchecked(tx_eip1559, sig, hash);
                Self::Eip1559(tx)
            }
            OpTypedTransaction::Eip7702(tx_eip7702) => {
                let tx = Signed::new_unchecked(tx_eip7702, sig, hash);
                Self::Eip7702(tx)
            }
            OpTypedTransaction::Deposit(tx) => Self::Deposit(Sealed::new_unchecked(tx, hash)),
            OpTypedTransaction::PostExec(tx) => Self::PostExec(Sealed::new_unchecked(tx, hash)),
        }
    }
}

impl From<(OpTypedTransaction, Signature)> for OpTxEnvelope {
    fn from(value: (OpTypedTransaction, Signature)) -> Self {
        Self::new_unhashed(value.0, value.1)
    }
}

impl From<Sealed<TxDeposit>> for OpTxEnvelope {
    fn from(v: Sealed<TxDeposit>) -> Self {
        Self::Deposit(v)
    }
}

impl From<TxPostExec> for OpTxEnvelope {
    fn from(v: TxPostExec) -> Self {
        v.seal_slow().into()
    }
}

impl From<Sealed<TxPostExec>> for OpTxEnvelope {
    fn from(v: Sealed<TxPostExec>) -> Self {
        Self::PostExec(v)
    }
}

impl<Tx> From<OpTxEnvelope> for Extended<OpTxEnvelope, Tx> {
    fn from(value: OpTxEnvelope) -> Self {
        Self::BuiltIn(value)
    }
}

impl<T> TryFrom<EthereumTxEnvelope<T>> for OpTxEnvelope {
    type Error = EthereumTxEnvelope<T>;

    fn try_from(value: EthereumTxEnvelope<T>) -> Result<Self, Self::Error> {
        Self::try_from_eth_envelope(value)
    }
}

impl TryFrom<OpTxEnvelope> for TxEnvelope {
    type Error = ValueError<OpTxEnvelope>;

    fn try_from(value: OpTxEnvelope) -> Result<Self, Self::Error> {
        value.try_into_eth_envelope()
    }
}

#[cfg(feature = "alloy-compat")]
impl From<OpTxEnvelope> for alloy_rpc_types_eth::TransactionRequest {
    fn from(value: OpTxEnvelope) -> Self {
        match value {
            OpTxEnvelope::Eip2930(tx) => tx.into_parts().0.into(),
            OpTxEnvelope::Eip1559(tx) => tx.into_parts().0.into(),
            OpTxEnvelope::Eip7702(tx) => tx.into_parts().0.into(),
            OpTxEnvelope::Deposit(tx) => tx.into_inner().into(),
            OpTxEnvelope::PostExec(tx) => tx.into_inner().into(),
            OpTxEnvelope::Legacy(tx) => tx.into_parts().0.into(),
        }
    }
}

impl OpTxEnvelope {
    /// Creates a new enveloped transaction from the given transaction, signature and hash.
    ///
    /// Caution: This assumes the given hash is the correct transaction hash.
    pub fn new_unchecked(
        transaction: OpTypedTransaction,
        signature: Signature,
        hash: B256,
    ) -> Self {
        Signed::new_unchecked(transaction, signature, hash).into()
    }

    /// Creates a new signed transaction from the given typed transaction and signature without the
    /// hash.
    ///
    /// Note: this only calculates the hash on the first [`OpTxEnvelope::hash`] call.
    pub fn new_unhashed(transaction: OpTypedTransaction, signature: Signature) -> Self {
        transaction.into_signed(signature).into()
    }

    /// Returns true if the transaction is a legacy transaction.
    #[inline]
    pub const fn is_legacy(&self) -> bool {
        matches!(self, Self::Legacy(_))
    }

    /// Returns true if the transaction is an EIP-2930 transaction.
    #[inline]
    pub const fn is_eip2930(&self) -> bool {
        matches!(self, Self::Eip2930(_))
    }

    /// Returns true if the transaction is an EIP-1559 transaction.
    #[inline]
    pub const fn is_eip1559(&self) -> bool {
        matches!(self, Self::Eip1559(_))
    }

    /// Returns true if the transaction is a system transaction.
    #[inline]
    pub const fn is_system_transaction(&self) -> bool {
        match self {
            Self::Deposit(tx) => tx.inner().is_system_transaction,
            _ => false,
        }
    }

    /// Attempts to convert the optimism variant into an ethereum [`TxEnvelope`].
    ///
    /// Returns the envelope as error if it is a variant unsupported on ethereum: [`TxDeposit`]
    pub fn try_into_eth_envelope(self) -> Result<TxEnvelope, ValueError<Self>> {
        match self {
            Self::Legacy(tx) => Ok(tx.into()),
            Self::Eip2930(tx) => Ok(tx.into()),
            Self::Eip1559(tx) => Ok(tx.into()),
            Self::Eip7702(tx) => Ok(tx.into()),
            tx @ Self::Deposit(_) => Err(ValueError::new(
                tx,
                "Deposit transactions cannot be converted to ethereum transaction",
            )),
            tx @ Self::PostExec(_) => Err(ValueError::new(
                tx,
                "PostExec transactions cannot be converted to ethereum transaction",
            )),
        }
    }

    /// Helper that creates [`OpTransactionInfo`] by adding [`OpDepositInfo`] obtained from the
    /// given closure if this transaction is a deposit and return the [`OpTransactionInfo`].
    pub fn try_to_tx_info<F, E>(
        &self,
        tx_info: TransactionInfo,
        f: F,
    ) -> Result<OpTransactionInfo, E>
    where
        F: FnOnce(TxHash) -> Result<Option<OpDepositInfo>, E>,
    {
        let deposit_meta =
            if self.is_deposit() { f(self.tx_hash())? } else { None }.unwrap_or_default();

        Ok(OpTransactionInfo::new(tx_info, deposit_meta))
    }

    /// Attempts to convert an ethereum [`TxEnvelope`] into the optimism variant.
    ///
    /// Returns the given envelope as error if [`OpTxEnvelope`] doesn't support the variant
    /// (EIP-4844)
    pub fn try_from_eth_envelope<T>(
        tx: EthereumTxEnvelope<T>,
    ) -> Result<Self, EthereumTxEnvelope<T>> {
        match tx {
            EthereumTxEnvelope::Legacy(tx) => Ok(tx.into()),
            EthereumTxEnvelope::Eip2930(tx) => Ok(tx.into()),
            EthereumTxEnvelope::Eip1559(tx) => Ok(tx.into()),
            tx @ EthereumTxEnvelope::<T>::Eip4844(_) => Err(tx),
            EthereumTxEnvelope::Eip7702(tx) => Ok(tx.into()),
        }
    }

    /// Returns mutable access to the input bytes.
    ///
    /// Caution: modifying this will cause side-effects on the hash.
    ///
    /// For [`TxPostExec`], this mutates the cached encoded payload bytes directly and may leave
    /// them out of sync with [`TxPostExec::payload`]. Rebuild the transaction with
    /// [`TxPostExec::new`] if you need to restore that invariant after mutating the input.
    #[doc(hidden)]
    pub const fn input_mut(&mut self) -> &mut Bytes {
        match self {
            Self::Eip1559(tx) => &mut tx.tx_mut().input,
            Self::Eip2930(tx) => &mut tx.tx_mut().input,
            Self::Legacy(tx) => &mut tx.tx_mut().input,
            Self::Eip7702(tx) => &mut tx.tx_mut().input,
            Self::Deposit(tx) => &mut tx.inner_mut().input,
            Self::PostExec(tx) => &mut tx.inner_mut().input,
        }
    }

    /// Attempts to convert an ethereum [`TxEnvelope`] into the optimism variant.
    ///
    /// Returns the given envelope as error if [`OpTxEnvelope`] doesn't support the variant
    /// (EIP-4844)
    #[cfg(feature = "alloy-compat")]
    pub fn try_from_any_envelope(
        tx: alloy_network::AnyTxEnvelope,
    ) -> Result<Self, alloy_network::AnyTxEnvelope> {
        match tx.try_into_envelope() {
            Ok(eth) => {
                Self::try_from_eth_envelope(eth).map_err(alloy_network::AnyTxEnvelope::Ethereum)
            }
            Err(err) => match err.into_value() {
                alloy_network::AnyTxEnvelope::Unknown(unknown) => {
                    let Ok(deposit) = unknown.inner.clone().try_into() else {
                        return Err(alloy_network::AnyTxEnvelope::Unknown(unknown));
                    };
                    Ok(Self::Deposit(Sealed::new_unchecked(deposit, unknown.hash)))
                }
                unsupported => Err(unsupported),
            },
        }
    }

    /// Returns true if the transaction is a deposit transaction.
    #[inline]
    pub const fn is_deposit(&self) -> bool {
        matches!(self, Self::Deposit(_))
    }

    /// Returns true if the transaction is a post-exec transaction.
    #[inline]
    pub const fn is_post_exec(&self) -> bool {
        matches!(self, Self::PostExec(_))
    }

    /// Returns the [`TxLegacy`] variant if the transaction is a legacy transaction.
    pub const fn as_legacy(&self) -> Option<&Signed<TxLegacy>> {
        match self {
            Self::Legacy(tx) => Some(tx),
            _ => None,
        }
    }

    /// Returns the [`TxEip2930`] variant if the transaction is an EIP-2930 transaction.
    pub const fn as_eip2930(&self) -> Option<&Signed<TxEip2930>> {
        match self {
            Self::Eip2930(tx) => Some(tx),
            _ => None,
        }
    }

    /// Returns the [`TxEip1559`] variant if the transaction is an EIP-1559 transaction.
    pub const fn as_eip1559(&self) -> Option<&Signed<TxEip1559>> {
        match self {
            Self::Eip1559(tx) => Some(tx),
            _ => None,
        }
    }

    /// Returns the [`TxDeposit`] variant if the transaction is a deposit transaction.
    pub const fn as_deposit(&self) -> Option<&Sealed<TxDeposit>> {
        match self {
            Self::Deposit(tx) => Some(tx),
            _ => None,
        }
    }

    /// Returns the [`TxPostExec`] variant if the transaction is a post-exec transaction.
    pub const fn as_post_exec(&self) -> Option<&Sealed<TxPostExec>> {
        match self {
            Self::PostExec(tx) => Some(tx),
            _ => None,
        }
    }

    /// Return the reference to signature.
    ///
    /// Returns `None` for unsigned variants: [`TxDeposit`] and [`TxPostExec`].
    pub const fn signature(&self) -> Option<&Signature> {
        match self {
            Self::Legacy(tx) => Some(tx.signature()),
            Self::Eip2930(tx) => Some(tx.signature()),
            Self::Eip1559(tx) => Some(tx.signature()),
            Self::Eip7702(tx) => Some(tx.signature()),
            Self::Deposit(_) | Self::PostExec(_) => None,
        }
    }

    /// Return the [`OpTxType`] of the inner txn.
    pub const fn tx_type(&self) -> OpTxType {
        match self {
            Self::Legacy(_) => OpTxType::Legacy,
            Self::Eip2930(_) => OpTxType::Eip2930,
            Self::Eip1559(_) => OpTxType::Eip1559,
            Self::Eip7702(_) => OpTxType::Eip7702,
            Self::Deposit(_) => OpTxType::Deposit,
            Self::PostExec(_) => OpTxType::PostExec,
        }
    }

    /// Returns the inner transaction hash.
    pub fn hash(&self) -> &B256 {
        match self {
            Self::Legacy(tx) => tx.hash(),
            Self::Eip1559(tx) => tx.hash(),
            Self::Eip2930(tx) => tx.hash(),
            Self::Eip7702(tx) => tx.hash(),
            Self::Deposit(tx) => tx.hash_ref(),
            Self::PostExec(tx) => tx.hash_ref(),
        }
    }

    /// Returns the inner transaction hash.
    pub fn tx_hash(&self) -> B256 {
        *self.hash()
    }

    /// Return the length of the inner txn, including type byte length
    pub fn eip2718_encoded_length(&self) -> usize {
        match self {
            Self::Legacy(t) => t.eip2718_encoded_length(),
            Self::Eip2930(t) => t.eip2718_encoded_length(),
            Self::Eip1559(t) => t.eip2718_encoded_length(),
            Self::Eip7702(t) => t.eip2718_encoded_length(),
            Self::Deposit(t) => t.eip2718_encoded_length(),
            Self::PostExec(t) => t.eip2718_encoded_length(),
        }
    }
}

impl TxHashRef for OpTxEnvelope {
    fn tx_hash(&self) -> &B256 {
        Self::hash(self)
    }
}

#[cfg(feature = "k256")]
impl alloy_consensus::transaction::SignerRecoverable for OpTxEnvelope {
    fn recover_signer(
        &self,
    ) -> Result<alloy_primitives::Address, alloy_consensus::crypto::RecoveryError> {
        let signature_hash = match self {
            Self::Legacy(tx) => tx.signature_hash(),
            Self::Eip2930(tx) => tx.signature_hash(),
            Self::Eip1559(tx) => tx.signature_hash(),
            Self::Eip7702(tx) => tx.signature_hash(),
            // Optimism's Deposit transaction does not have a signature. Directly return the
            // `from` address.
            Self::Deposit(tx) => return Ok(tx.from),
            // Post-exec transactions are unsigned synthetic system transactions. They use a
            // canonical zero-address signer rather than a cryptographic signature.
            Self::PostExec(tx) => return Ok(tx.inner().signer_address()),
        };
        let signature = match self {
            Self::Legacy(tx) => tx.signature(),
            Self::Eip2930(tx) => tx.signature(),
            Self::Eip1559(tx) => tx.signature(),
            Self::Eip7702(tx) => tx.signature(),
            // Deposit and PostExec are unsigned and handled via early return above.
            Self::Deposit(_) | Self::PostExec(_) => {
                unreachable!("non-signed transactions should not be handled here")
            }
        };
        alloy_consensus::crypto::secp256k1::recover_signer(signature, signature_hash)
    }

    fn recover_signer_unchecked(
        &self,
    ) -> Result<alloy_primitives::Address, alloy_consensus::crypto::RecoveryError> {
        let signature_hash = match self {
            Self::Legacy(tx) => tx.signature_hash(),
            Self::Eip2930(tx) => tx.signature_hash(),
            Self::Eip1559(tx) => tx.signature_hash(),
            Self::Eip7702(tx) => tx.signature_hash(),
            // Optimism's Deposit transaction does not have a signature. Directly return the
            // `from` address.
            Self::Deposit(tx) => return Ok(tx.from),
            // Post-exec transactions are unsigned synthetic system transactions. They use a
            // canonical zero-address signer rather than a cryptographic signature.
            Self::PostExec(tx) => return Ok(tx.inner().signer_address()),
        };
        let signature = match self {
            Self::Legacy(tx) => tx.signature(),
            Self::Eip2930(tx) => tx.signature(),
            Self::Eip1559(tx) => tx.signature(),
            Self::Eip7702(tx) => tx.signature(),
            // Deposit and PostExec are unsigned and handled via early return above.
            Self::Deposit(_) | Self::PostExec(_) => unreachable!(),
        };
        alloy_consensus::crypto::secp256k1::recover_signer_unchecked(signature, signature_hash)
    }

    fn recover_unchecked_with_buf(
        &self,
        buf: &mut Vec<u8>,
    ) -> Result<alloy_primitives::Address, alloy_consensus::crypto::RecoveryError> {
        match self {
            Self::Legacy(tx) => {
                alloy_consensus::transaction::SignerRecoverable::recover_unchecked_with_buf(tx, buf)
            }
            Self::Eip2930(tx) => {
                alloy_consensus::transaction::SignerRecoverable::recover_unchecked_with_buf(tx, buf)
            }
            Self::Eip1559(tx) => {
                alloy_consensus::transaction::SignerRecoverable::recover_unchecked_with_buf(tx, buf)
            }
            Self::Eip7702(tx) => {
                alloy_consensus::transaction::SignerRecoverable::recover_unchecked_with_buf(tx, buf)
            }
            // Deposit and PostExec are unsigned; return their canonical signer directly.
            Self::Deposit(tx) => Ok(tx.from),
            Self::PostExec(tx) => Ok(tx.inner().signer_address()),
        }
    }
}
