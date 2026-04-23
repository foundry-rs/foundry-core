//! Contains Optimism specific precompiles.
#[cfg(not(feature = "std"))]
use std::{boxed::Box, string::String};

use crate::OpSpecId;
use revm::{
    context::Cfg,
    context_interface::ContextTr,
    handler::{EthPrecompiles, PrecompileProvider},
    interpreter::{CallInputs, InterpreterResult},
    precompile::{
        self, EthPrecompileResult, Precompile, PrecompileHalt, PrecompileId, Precompiles, bn254,
        eth_precompile_fn, secp256r1,
    },
    primitives::{Address, OnceLock, hardfork::SpecId},
};

/// Optimism precompile provider
#[derive(Debug, Clone)]
pub struct OpPrecompiles {
    /// Inner precompile provider is same as Ethereums.
    inner: EthPrecompiles,
    /// Spec id of the precompile provider.
    spec: OpSpecId,
}

impl OpPrecompiles {
    /// Create a new precompile provider with the given `OpSpec`.
    #[inline]
    pub fn new_with_spec(spec: OpSpecId) -> Self {
        let precompiles = match spec {
            spec @ (OpSpecId::BEDROCK
            | OpSpecId::REGOLITH
            | OpSpecId::CANYON
            | OpSpecId::ECOTONE) => Precompiles::new(spec.into_eth_spec().into()),
            OpSpecId::FJORD => fjord(),
            OpSpecId::GRANITE | OpSpecId::HOLOCENE => granite(),
            OpSpecId::ISTHMUS => isthmus(),
            OpSpecId::INTEROP | OpSpecId::KARST | OpSpecId::JOVIAN => jovian(),
        };

        Self { inner: EthPrecompiles { precompiles, spec: SpecId::default() }, spec }
    }

    /// Precompiles getter.
    #[inline]
    pub const fn precompiles(&self) -> &'static Precompiles {
        self.inner.precompiles
    }
}

/// Returns precompiles for Fjord spec.
pub fn fjord() -> &'static Precompiles {
    static INSTANCE: OnceLock<Precompiles> = OnceLock::new();
    INSTANCE.get_or_init(|| {
        let mut precompiles = Precompiles::cancun().clone();
        // RIP-7212: secp256r1 P256verify
        precompiles.extend([secp256r1::P256VERIFY]);
        precompiles
    })
}

/// Returns precompiles for Granite spec.
pub fn granite() -> &'static Precompiles {
    static INSTANCE: OnceLock<Precompiles> = OnceLock::new();
    INSTANCE.get_or_init(|| {
        let mut precompiles = fjord().clone();
        // Restrict bn254Pairing input size
        precompiles.extend([bn254_pair::GRANITE]);
        precompiles
    })
}

/// Returns precompiles for isthmus spec.
pub fn isthmus() -> &'static Precompiles {
    static INSTANCE: OnceLock<Precompiles> = OnceLock::new();
    INSTANCE.get_or_init(|| {
        let mut precompiles = granite().clone();
        // Prague bls12 precompiles
        precompiles.extend(precompile::bls12_381::precompiles());
        // Isthmus bls12 precompile modifications
        precompiles.extend([
            bls12_381::ISTHMUS_G1_MSM,
            bls12_381::ISTHMUS_G2_MSM,
            bls12_381::ISTHMUS_PAIRING,
        ]);
        precompiles
    })
}

/// Returns precompiles for jovian spec.
pub fn jovian() -> &'static Precompiles {
    static INSTANCE: OnceLock<Precompiles> = OnceLock::new();
    INSTANCE.get_or_init(|| {
        let mut precompiles = isthmus().clone();

        let mut to_remove = Precompiles::default();
        to_remove.extend([
            bn254::pair::ISTANBUL,
            bls12_381::ISTHMUS_G1_MSM,
            bls12_381::ISTHMUS_G2_MSM,
            bls12_381::ISTHMUS_PAIRING,
        ]);

        // Replace the 4 variable-input precompiles with Jovian versions (reduced limits)
        precompiles.difference(&to_remove);

        precompiles.extend([
            bn254_pair::JOVIAN,
            bls12_381::JOVIAN_G1_MSM,
            bls12_381::JOVIAN_G2_MSM,
            bls12_381::JOVIAN_PAIRING,
        ]);

        precompiles
    })
}

impl<CTX> PrecompileProvider<CTX> for OpPrecompiles
where
    CTX: ContextTr<Cfg: Cfg<Spec = OpSpecId>>,
{
    type Output = InterpreterResult;

    #[inline]
    fn set_spec(&mut self, spec: <CTX::Cfg as Cfg>::Spec) -> bool {
        if spec == self.spec {
            return false;
        }
        *self = Self::new_with_spec(spec);
        true
    }

    #[inline]
    fn run(
        &mut self,
        context: &mut CTX,
        inputs: &CallInputs,
    ) -> Result<Option<Self::Output>, String> {
        self.inner.run(context, inputs)
    }

    #[inline]
    fn warm_addresses(&self) -> Box<impl Iterator<Item = Address>> {
        self.inner.warm_addresses()
    }

    #[inline]
    fn contains(&self, address: &Address) -> bool {
        self.inner.contains(address)
    }
}

impl Default for OpPrecompiles {
    fn default() -> Self {
        Self::new_with_spec(OpSpecId::JOVIAN)
    }
}

/// Bn254 pair precompile.
pub mod bn254_pair {
    use super::*;

    /// Max input size for the bn254 pair precompile.
    pub const GRANITE_MAX_INPUT_SIZE: usize = 112687;

    /// Run the bn254 pair precompile with Optimism input limit.
    pub fn run_pair_granite(input: &[u8], gas_limit: u64) -> EthPrecompileResult {
        if input.len() > GRANITE_MAX_INPUT_SIZE {
            return Err(PrecompileHalt::Bn254PairLength);
        }
        bn254::run_pair(
            input,
            bn254::pair::ISTANBUL_PAIR_PER_POINT,
            bn254::pair::ISTANBUL_PAIR_BASE,
            gas_limit,
        )
    }

    eth_precompile_fn!(run_pair_granite_wrapper, run_pair_granite);

    /// Bn254 pair precompile.
    pub const GRANITE: Precompile =
        Precompile::new(PrecompileId::Bn254Pairing, bn254::pair::ADDRESS, run_pair_granite_wrapper);

    /// Max input size for the bn254 pair precompile.
    pub const JOVIAN_MAX_INPUT_SIZE: usize = 81_984;

    /// Run the bn254 pair precompile with Optimism input limit.
    pub fn run_pair_jovian(input: &[u8], gas_limit: u64) -> EthPrecompileResult {
        if input.len() > JOVIAN_MAX_INPUT_SIZE {
            return Err(PrecompileHalt::Bn254PairLength);
        }
        bn254::run_pair(
            input,
            bn254::pair::ISTANBUL_PAIR_PER_POINT,
            bn254::pair::ISTANBUL_PAIR_BASE,
            gas_limit,
        )
    }

    eth_precompile_fn!(run_pair_jovian_wrapper, run_pair_jovian);

    /// Bn254 pair precompile.
    pub const JOVIAN: Precompile =
        Precompile::new(PrecompileId::Bn254Pairing, bn254::pair::ADDRESS, run_pair_jovian_wrapper);
}

/// `Bls12_381` precompile.
pub mod bls12_381 {
    use super::*;
    use revm::precompile::bls12_381_const::{G1_MSM_ADDRESS, G2_MSM_ADDRESS, PAIRING_ADDRESS};

    /// Max input size for the g1 msm precompile.
    pub const ISTHMUS_G1_MSM_MAX_INPUT_SIZE: usize = 513760;

    /// The maximum input size for the BLS12-381 g1 msm operation after the Jovian Hardfork.
    pub const JOVIAN_G1_MSM_MAX_INPUT_SIZE: usize = 288_960;

    /// Max input size for the g2 msm precompile.
    pub const ISTHMUS_G2_MSM_MAX_INPUT_SIZE: usize = 488448;

    /// Max input size for the g2 msm precompile after the Jovian Hardfork.
    pub const JOVIAN_G2_MSM_MAX_INPUT_SIZE: usize = 278_784;

    /// Max input size for the pairing precompile.
    pub const ISTHMUS_PAIRING_MAX_INPUT_SIZE: usize = 235008;

    /// Max input size for the pairing precompile after the Jovian Hardfork.
    pub const JOVIAN_PAIRING_MAX_INPUT_SIZE: usize = 156_672;

    /// Run the g1 msm precompile with Optimism input limit.
    pub fn run_g1_msm_isthmus(input: &[u8], gas_limit: u64) -> EthPrecompileResult {
        if input.len() > ISTHMUS_G1_MSM_MAX_INPUT_SIZE {
            return Err(PrecompileHalt::Other(
                "G1MSM input length too long for OP Stack input size limitation after the Isthmus Hardfork".into(),
            ));
        }
        precompile::bls12_381::g1_msm::g1_msm(input, gas_limit)
    }

    eth_precompile_fn!(run_g1_msm_isthmus_wrapper, run_g1_msm_isthmus);

    /// G1 msm precompile.
    pub const ISTHMUS_G1_MSM: Precompile =
        Precompile::new(PrecompileId::Bls12G1Msm, G1_MSM_ADDRESS, run_g1_msm_isthmus_wrapper);

    /// Run the g1 msm precompile with Optimism input limit.
    pub fn run_g1_msm_jovian(input: &[u8], gas_limit: u64) -> EthPrecompileResult {
        if input.len() > JOVIAN_G1_MSM_MAX_INPUT_SIZE {
            return Err(PrecompileHalt::Other(
                "G1MSM input length too long for OP Stack input size limitation after the Jovian Hardfork".into(),
            ));
        }
        precompile::bls12_381::g1_msm::g1_msm(input, gas_limit)
    }

    eth_precompile_fn!(run_g1_msm_jovian_wrapper, run_g1_msm_jovian);

    /// G1 msm precompile after the Jovian Hardfork.
    pub const JOVIAN_G1_MSM: Precompile =
        Precompile::new(PrecompileId::Bls12G1Msm, G1_MSM_ADDRESS, run_g1_msm_jovian_wrapper);

    /// Run the g2 msm precompile with Optimism input limit.
    pub fn run_g2_msm_isthmus(input: &[u8], gas_limit: u64) -> EthPrecompileResult {
        if input.len() > ISTHMUS_G2_MSM_MAX_INPUT_SIZE {
            return Err(PrecompileHalt::Other(
                "G2MSM input length too long for OP Stack input size limitation".into(),
            ));
        }
        precompile::bls12_381::g2_msm::g2_msm(input, gas_limit)
    }

    eth_precompile_fn!(run_g2_msm_isthmus_wrapper, run_g2_msm_isthmus);

    /// G2 msm precompile.
    pub const ISTHMUS_G2_MSM: Precompile =
        Precompile::new(PrecompileId::Bls12G2Msm, G2_MSM_ADDRESS, run_g2_msm_isthmus_wrapper);

    /// Run the g2 msm precompile with Optimism input limit after the Jovian Hardfork.
    pub fn run_g2_msm_jovian(input: &[u8], gas_limit: u64) -> EthPrecompileResult {
        if input.len() > JOVIAN_G2_MSM_MAX_INPUT_SIZE {
            return Err(PrecompileHalt::Other(
                "G2MSM input length too long for OP Stack input size limitation after the Jovian Hardfork".into(),
            ));
        }
        precompile::bls12_381::g2_msm::g2_msm(input, gas_limit)
    }

    eth_precompile_fn!(run_g2_msm_jovian_wrapper, run_g2_msm_jovian);

    /// G2 msm precompile after the Jovian Hardfork.
    pub const JOVIAN_G2_MSM: Precompile =
        Precompile::new(PrecompileId::Bls12G2Msm, G2_MSM_ADDRESS, run_g2_msm_jovian_wrapper);

    /// Run the pairing precompile with Optimism input limit.
    pub fn run_pair_isthmus(input: &[u8], gas_limit: u64) -> EthPrecompileResult {
        if input.len() > ISTHMUS_PAIRING_MAX_INPUT_SIZE {
            return Err(PrecompileHalt::Other(
                "Pairing input length too long for OP Stack input size limitation".into(),
            ));
        }
        precompile::bls12_381::pairing::pairing(input, gas_limit)
    }

    eth_precompile_fn!(run_pair_isthmus_wrapper, run_pair_isthmus);

    /// Pairing precompile.
    pub const ISTHMUS_PAIRING: Precompile =
        Precompile::new(PrecompileId::Bls12Pairing, PAIRING_ADDRESS, run_pair_isthmus_wrapper);

    /// Run the pairing precompile with Optimism input limit after the Jovian Hardfork.
    pub fn run_pair_jovian(input: &[u8], gas_limit: u64) -> EthPrecompileResult {
        if input.len() > JOVIAN_PAIRING_MAX_INPUT_SIZE {
            return Err(PrecompileHalt::Other(
                "Pairing input length too long for OP Stack input size limitation after the Jovian Hardfork".into(),
            ));
        }
        precompile::bls12_381::pairing::pairing(input, gas_limit)
    }

    eth_precompile_fn!(run_pair_jovian_wrapper, run_pair_jovian);

    /// Pairing precompile after the Jovian Hardfork.
    pub const JOVIAN_PAIRING: Precompile =
        Precompile::new(PrecompileId::Bls12Pairing, PAIRING_ADDRESS, run_pair_jovian_wrapper);
}
