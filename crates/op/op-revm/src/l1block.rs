//! Contains the `[L1BlockInfo]` type and its implementation.
use crate::{
    OpSpecId,
    constants::{
        BASE_FEE_SCALAR_OFFSET, BLOB_BASE_FEE_SCALAR_OFFSET, DA_FOOTPRINT_GAS_SCALAR_OFFSET,
        DA_FOOTPRINT_GAS_SCALAR_SLOT, ECOTONE_L1_BLOB_BASE_FEE_SLOT, ECOTONE_L1_FEE_SCALARS_SLOT,
        EMPTY_SCALARS, L1_BASE_FEE_SLOT, L1_BLOCK_CONTRACT, L1_OVERHEAD_SLOT, L1_SCALAR_SLOT,
        NON_ZERO_BYTE_COST, OPERATOR_FEE_CONSTANT_OFFSET, OPERATOR_FEE_JOVIAN_MULTIPLIER,
        OPERATOR_FEE_SCALAR_DECIMAL, OPERATOR_FEE_SCALAR_OFFSET, OPERATOR_FEE_SCALARS_SLOT,
    },
    transaction::{OpTxTr, estimate_tx_compressed_size},
};
use revm::{
    context_interface::cfg::gas::{NON_ZERO_BYTE_MULTIPLIER_ISTANBUL, STANDARD_TOKEN_COST},
    database_interface::Database,
    interpreter::{Gas, gas::get_tokens_in_calldata_istanbul},
    primitives::U256,
};

/// L1 block info
///
/// We can extract L1 epoch data from each L2 block, by looking at the `setL1BlockValues`
/// transaction data. This data is then used to calculate the L1 cost of a transaction.
///
/// Here is the format of the `setL1BlockValues` transaction data:
///
/// setL1BlockValues(uint64 _number, uint64 _timestamp, uint256 _basefee, bytes32 _hash,
/// uint64 _sequenceNumber, bytes32 _batcherHash, uint256 _l1FeeOverhead, uint256 _l1FeeScalar)
///
/// For now, we only care about the fields necessary for L1 cost calculation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct L1BlockInfo {
    /// The L2 block number. If not same as the one in the context,
    /// `L1BlockInfo` is not valid and will be reloaded from the database.
    pub l2_block: Option<U256>,
    /// The base fee of the L1 origin block.
    pub l1_base_fee: U256,
    /// The current L1 fee overhead. None if Ecotone is activated.
    pub l1_fee_overhead: Option<U256>,
    /// The current L1 fee scalar.
    pub l1_base_fee_scalar: U256,
    /// The current L1 blob base fee. None if Ecotone is not activated, except if
    /// `empty_ecotone_scalars` is `true`.
    pub l1_blob_base_fee: Option<U256>,
    /// The current L1 blob base fee scalar. None if Ecotone is not activated.
    pub l1_blob_base_fee_scalar: Option<U256>,
    /// The operator fee scalar. None if Isthmus is not activated.
    pub operator_fee_scalar: Option<U256>,
    /// The operator fee constant. None if Isthmus is not activated.
    pub operator_fee_constant: Option<U256>,
    /// Da footprint gas scalar. Used to set the DA footprint block limit on the L2. Always null
    /// prior to the Jovian hardfork.
    pub da_footprint_gas_scalar: Option<u16>,
    /// True if Ecotone is activated, but the L1 fee scalars have not yet been set.
    pub empty_ecotone_scalars: bool,
    /// Last calculated l1 fee cost. Uses as a cache between validation and pre execution stages.
    pub tx_l1_cost: Option<U256>,
}

impl L1BlockInfo {
    /// Fetch the DA footprint gas scalar from the database.
    pub fn fetch_da_footprint_gas_scalar<DB: Database>(db: &mut DB) -> Result<u16, DB::Error> {
        let da_footprint_gas_scalar_slot =
            db.storage(L1_BLOCK_CONTRACT, DA_FOOTPRINT_GAS_SCALAR_SLOT)?.to_be_bytes::<32>();

        // Extract the first 2 bytes directly as a u16 in big-endian format
        let bytes = [
            da_footprint_gas_scalar_slot[DA_FOOTPRINT_GAS_SCALAR_OFFSET],
            da_footprint_gas_scalar_slot[DA_FOOTPRINT_GAS_SCALAR_OFFSET + 1],
        ];
        Ok(u16::from_be_bytes(bytes))
    }

    /// Try to fetch the L1 block info from the database, post-Jovian.
    fn try_fetch_jovian<DB: Database>(&mut self, db: &mut DB) -> Result<(), DB::Error> {
        self.da_footprint_gas_scalar = Some(Self::fetch_da_footprint_gas_scalar(db)?);

        Ok(())
    }

    /// Try to fetch the L1 block info from the database, post-Isthmus.
    fn try_fetch_isthmus<DB: Database>(&mut self, db: &mut DB) -> Result<(), DB::Error> {
        // Post-isthmus L1 block info
        let operator_fee_scalars =
            db.storage(L1_BLOCK_CONTRACT, OPERATOR_FEE_SCALARS_SLOT)?.to_be_bytes::<32>();

        // The `operator_fee_scalar` is stored as a big endian u32 at
        // OPERATOR_FEE_SCALAR_OFFSET.
        self.operator_fee_scalar = Some(U256::from_be_slice(
            operator_fee_scalars[OPERATOR_FEE_SCALAR_OFFSET..OPERATOR_FEE_SCALAR_OFFSET + 4]
                .as_ref(),
        ));
        // The `operator_fee_constant` is stored as a big endian u64 at
        // OPERATOR_FEE_CONSTANT_OFFSET.
        self.operator_fee_constant = Some(U256::from_be_slice(
            operator_fee_scalars[OPERATOR_FEE_CONSTANT_OFFSET..OPERATOR_FEE_CONSTANT_OFFSET + 8]
                .as_ref(),
        ));

        Ok(())
    }

    /// Try to fetch the L1 block info from the database, post-Ecotone.
    fn try_fetch_ecotone<DB: Database>(&mut self, db: &mut DB) -> Result<(), DB::Error> {
        self.l1_blob_base_fee = Some(db.storage(L1_BLOCK_CONTRACT, ECOTONE_L1_BLOB_BASE_FEE_SLOT)?);

        let l1_fee_scalars =
            db.storage(L1_BLOCK_CONTRACT, ECOTONE_L1_FEE_SCALARS_SLOT)?.to_be_bytes::<32>();

        self.l1_base_fee_scalar = U256::from_be_slice(
            l1_fee_scalars[BASE_FEE_SCALAR_OFFSET..BASE_FEE_SCALAR_OFFSET + 4].as_ref(),
        );

        let l1_blob_base_fee = U256::from_be_slice(
            l1_fee_scalars[BLOB_BASE_FEE_SCALAR_OFFSET..BLOB_BASE_FEE_SCALAR_OFFSET + 4].as_ref(),
        );
        self.l1_blob_base_fee_scalar = Some(l1_blob_base_fee);

        // Check if the L1 fee scalars are empty. If so, we use the Bedrock cost function.
        // The L1 fee overhead is only necessary if `empty_ecotone_scalars` is true, as it was
        // deprecated in Ecotone.
        self.empty_ecotone_scalars = l1_blob_base_fee.is_zero()
            && l1_fee_scalars[BASE_FEE_SCALAR_OFFSET..BLOB_BASE_FEE_SCALAR_OFFSET + 4]
                == EMPTY_SCALARS;
        self.l1_fee_overhead = self
            .empty_ecotone_scalars
            .then(|| db.storage(L1_BLOCK_CONTRACT, L1_OVERHEAD_SLOT))
            .transpose()?;

        Ok(())
    }

    /// Try to fetch the L1 block info from the database.
    pub fn try_fetch<DB: Database>(
        db: &mut DB,
        l2_block: U256,
        spec_id: OpSpecId,
    ) -> Result<Self, DB::Error> {
        // Ensure the L1 Block account is loaded into the cache.
        let _ = db.basic(L1_BLOCK_CONTRACT)?;

        let mut out = Self {
            l2_block: Some(l2_block),
            l1_base_fee: db.storage(L1_BLOCK_CONTRACT, L1_BASE_FEE_SLOT)?,
            ..Default::default()
        };

        // Post-Ecotone
        if !spec_id.is_enabled_in(OpSpecId::ECOTONE) {
            out.l1_base_fee_scalar = db.storage(L1_BLOCK_CONTRACT, L1_SCALAR_SLOT)?;
            out.l1_fee_overhead = Some(db.storage(L1_BLOCK_CONTRACT, L1_OVERHEAD_SLOT)?);

            return Ok(out);
        }

        out.try_fetch_ecotone(db)?;

        // Post-Isthmus L1 block info
        if spec_id.is_enabled_in(OpSpecId::ISTHMUS) {
            out.try_fetch_isthmus(db)?;
        }

        // Pre-Jovian
        if spec_id.is_enabled_in(OpSpecId::JOVIAN) {
            out.try_fetch_jovian(db)?;
        }

        Ok(out)
    }

    /// Calculate the operator fee for executing this transaction.
    ///
    /// Introduced in isthmus. Prior to isthmus, the operator fee is always zero.
    pub fn operator_fee_charge(&self, input: &[u8], gas_limit: U256, spec_id: OpSpecId) -> U256 {
        // If the input is a deposit transaction or empty, the default value is zero.
        if input.is_empty() || input.first() == Some(&0x7E) {
            return U256::ZERO;
        }

        self.operator_fee_charge_inner(gas_limit, spec_id)
    }

    /// Calculate the operator fee for the given `gas`.
    fn operator_fee_charge_inner(&self, gas: U256, spec_id: OpSpecId) -> U256 {
        let operator_fee_scalar =
            self.operator_fee_scalar.expect("Missing operator fee scalar for isthmus L1 Block");
        let operator_fee_constant =
            self.operator_fee_constant.expect("Missing operator fee constant for isthmus L1 Block");

        let product = if spec_id.is_enabled_in(OpSpecId::JOVIAN) {
            gas.saturating_mul(operator_fee_scalar)
                .saturating_mul(U256::from(OPERATOR_FEE_JOVIAN_MULTIPLIER))
        } else {
            gas.saturating_mul(operator_fee_scalar) / U256::from(OPERATOR_FEE_SCALAR_DECIMAL)
        };

        product.saturating_add(operator_fee_constant)
    }

    /// Calculate the operator fee for executing this transaction.
    ///
    /// Introduced in isthmus. Prior to isthmus, the operator fee is always zero.
    pub fn operator_fee_refund(&self, gas: &Gas, spec_id: OpSpecId) -> U256 {
        if !spec_id.is_enabled_in(OpSpecId::ISTHMUS) {
            return U256::ZERO;
        }

        let operator_cost_gas_limit =
            self.operator_fee_charge_inner(U256::from(gas.limit()), spec_id);
        let operator_cost_gas_used = self.operator_fee_charge_inner(
            U256::from(gas.limit() - (gas.remaining() + gas.refunded() as u64)),
            spec_id,
        );

        operator_cost_gas_limit.saturating_sub(operator_cost_gas_used)
    }

    /// Calculate the data gas for posting the transaction on L1. Calldata costs 16 gas per byte
    /// after compression.
    ///
    /// Prior to fjord, calldata costs 16 gas per non-zero byte and 4 gas per zero byte.
    ///
    /// Prior to regolith, an extra 68 non-zero bytes were included in the rollup data costs to
    /// account for the empty signature.
    pub fn data_gas(&self, input: &[u8], spec_id: OpSpecId) -> U256 {
        if spec_id.is_enabled_in(OpSpecId::FJORD) {
            let estimated_size = self.tx_estimated_size_fjord(input);

            return estimated_size
                .saturating_mul(U256::from(NON_ZERO_BYTE_COST))
                .wrapping_div(U256::from(1_000_000));
        };

        // tokens in calldata where non-zero bytes are priced 4 times higher than zero bytes (Same
        // as in Istanbul).
        let mut tokens_in_transaction_data = get_tokens_in_calldata_istanbul(input);

        // Prior to regolith, an extra 68 non zero bytes were included in the rollup data costs.
        if !spec_id.is_enabled_in(OpSpecId::REGOLITH) {
            tokens_in_transaction_data += 68 * NON_ZERO_BYTE_MULTIPLIER_ISTANBUL;
        }

        U256::from(tokens_in_transaction_data.saturating_mul(STANDARD_TOKEN_COST))
    }

    // Calculate the estimated compressed transaction size in bytes, scaled by 1e6.
    // This value is computed based on the following formula:
    // max(minTransactionSize, intercept + fastlzCoef*fastlzSize)
    fn tx_estimated_size_fjord(&self, input: &[u8]) -> U256 {
        U256::from(estimate_tx_compressed_size(input))
    }

    /// Clears the cached L1 cost of the transaction.
    pub const fn clear_tx_l1_cost(&mut self) {
        self.tx_l1_cost = None;
    }

    /// Calculate additional transaction cost with `OpTxTr`.
    ///
    /// Internally calls [`L1BlockInfo::tx_cost`].
    pub fn tx_cost_with_tx(&mut self, tx: impl OpTxTr, spec: OpSpecId) -> Option<U256> {
        // account for additional cost of l1 fee and operator fee
        let enveloped_tx = tx.enveloped_tx()?;
        let gas_limit = U256::from(tx.gas_limit());
        Some(self.tx_cost(enveloped_tx, gas_limit, spec))
    }

    /// Calculate additional transaction cost.
    #[inline]
    pub fn tx_cost(&mut self, enveloped_tx: &[u8], gas_limit: U256, spec: OpSpecId) -> U256 {
        // compute L1 cost
        let mut additional_cost = self.calculate_tx_l1_cost(enveloped_tx, spec);

        // compute operator fee
        if spec.is_enabled_in(OpSpecId::ISTHMUS) {
            let operator_fee_charge = self.operator_fee_charge(enveloped_tx, gas_limit, spec);
            additional_cost = additional_cost.saturating_add(operator_fee_charge);
        }

        additional_cost
    }

    /// Calculate the gas cost of a transaction based on L1 block data posted on L2, depending on
    /// the [`OpSpecId`] passed.
    pub fn calculate_tx_l1_cost(&mut self, input: &[u8], spec_id: OpSpecId) -> U256 {
        if let Some(tx_l1_cost) = self.tx_l1_cost {
            return tx_l1_cost;
        }
        // If the input is a deposit transaction or empty, the default value is zero.
        let tx_l1_cost = if input.is_empty() || input.first() == Some(&0x7E) {
            return U256::ZERO;
        } else if spec_id.is_enabled_in(OpSpecId::FJORD) {
            self.calculate_tx_l1_cost_fjord(input)
        } else if spec_id.is_enabled_in(OpSpecId::ECOTONE) {
            self.calculate_tx_l1_cost_ecotone(input, spec_id)
        } else {
            self.calculate_tx_l1_cost_bedrock(input, spec_id)
        };

        self.tx_l1_cost = Some(tx_l1_cost);
        tx_l1_cost
    }

    /// Calculate the gas cost of a transaction based on L1 block data posted on L2, pre-Ecotone.
    fn calculate_tx_l1_cost_bedrock(&self, input: &[u8], spec_id: OpSpecId) -> U256 {
        let rollup_data_gas_cost = self.data_gas(input, spec_id);
        rollup_data_gas_cost
            .saturating_add(self.l1_fee_overhead.unwrap_or_default())
            .saturating_mul(self.l1_base_fee)
            .saturating_mul(self.l1_base_fee_scalar)
            .wrapping_div(U256::from(1_000_000))
    }

    /// Calculate the gas cost of a transaction based on L1 block data posted on L2, post-Ecotone.
    ///
    /// [`OpSpecId::ECOTONE`] L1 cost function:
    /// `(calldataGas/16)*(l1BaseFee*16*l1BaseFeeScalar + l1BlobBaseFee*l1BlobBaseFeeScalar)/1e6`
    ///
    /// We divide "calldataGas" by 16 to change from units of calldata gas to "estimated # of bytes
    /// when compressed". Known as "compressedTxSize" in the spec.
    ///
    /// Function is actually computed as follows for better precision under integer arithmetic:
    /// `calldataGas*(l1BaseFee*16*l1BaseFeeScalar + l1BlobBaseFee*l1BlobBaseFeeScalar)/16e6`
    fn calculate_tx_l1_cost_ecotone(&self, input: &[u8], spec_id: OpSpecId) -> U256 {
        // There is an edgecase where, for the very first Ecotone block (unless it is activated at
        // Genesis), we must use the Bedrock cost function. To determine if this is the
        // case, we can check if the Ecotone parameters are unset.
        if self.empty_ecotone_scalars {
            return self.calculate_tx_l1_cost_bedrock(input, spec_id);
        }

        let rollup_data_gas_cost = self.data_gas(input, spec_id);
        let l1_fee_scaled = self.calculate_l1_fee_scaled_ecotone();

        l1_fee_scaled
            .saturating_mul(rollup_data_gas_cost)
            .wrapping_div(U256::from(1_000_000 * NON_ZERO_BYTE_COST))
    }

    /// Calculate the gas cost of a transaction based on L1 block data posted on L2, post-Fjord.
    ///
    /// [`OpSpecId::FJORD`] L1 cost function:
    /// `estimatedSize*(baseFeeScalar*l1BaseFee*16 + blobFeeScalar*l1BlobBaseFee)/1e12`
    fn calculate_tx_l1_cost_fjord(&self, input: &[u8]) -> U256 {
        let l1_fee_scaled = self.calculate_l1_fee_scaled_ecotone();
        if l1_fee_scaled.is_zero() {
            return U256::ZERO;
        }

        let estimated_size = self.tx_estimated_size_fjord(input);

        estimated_size.saturating_mul(l1_fee_scaled).wrapping_div(U256::from(1_000_000_000_000u64))
    }

    // l1BaseFee*16*l1BaseFeeScalar + l1BlobBaseFee*l1BlobBaseFeeScalar
    fn calculate_l1_fee_scaled_ecotone(&self) -> U256 {
        let calldata_cost_per_byte = self
            .l1_base_fee
            .saturating_mul(U256::from(NON_ZERO_BYTE_COST))
            .saturating_mul(self.l1_base_fee_scalar);
        let blob_cost_per_byte = self
            .l1_blob_base_fee
            .unwrap_or_default()
            .saturating_mul(self.l1_blob_base_fee_scalar.unwrap_or_default());

        calldata_cost_per_byte.saturating_add(blob_cost_per_byte)
    }
}
