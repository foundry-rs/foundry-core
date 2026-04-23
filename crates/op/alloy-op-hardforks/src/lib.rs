//! OP Stack hardfork definitions.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![no_std]

extern crate alloc;
use alloc::vec::Vec;
use alloy_chains::{Chain, NamedChain};
use alloy_hardforks::{EthereumHardfork, hardfork};
pub use alloy_hardforks::{EthereumHardforks, ForkCondition};
use alloy_primitives::U256;
use core::ops::Index;

pub mod optimism;
pub use optimism::{mainnet as op_mainnet, mainnet::*, sepolia as op_sepolia, sepolia::*};

pub mod base;
pub use base::{mainnet as base_mainnet, mainnet::*, sepolia as base_sepolia, sepolia::*};

hardfork!(
    /// The name of an optimism hardfork.
    ///
    /// When building a list of hardforks for a chain, it's still expected to zip with
    /// [`EthereumHardfork`].
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    #[derive(Default)]
    OpHardfork {
        /// Bedrock: <https://blog.oplabs.co/introducing-optimism-bedrock>.
        Bedrock,
        /// Regolith: <https://github.com/ethereum-optimism/specs/blob/main/specs/protocol/superchain-upgrades.md#regolith>.
        Regolith,
        /// <https://github.com/ethereum-optimism/specs/blob/main/specs/protocol/superchain-upgrades.md#canyon>.
        Canyon,
        /// Ecotone: <https://github.com/ethereum-optimism/specs/blob/main/specs/protocol/superchain-upgrades.md#ecotone>.
        Ecotone,
        /// Fjord: <https://github.com/ethereum-optimism/specs/blob/main/specs/protocol/superchain-upgrades.md#fjord>
        Fjord,
        /// Granite: <https://github.com/ethereum-optimism/specs/blob/main/specs/protocol/superchain-upgrades.md#granite>
        Granite,
        /// Holocene: <https://github.com/ethereum-optimism/specs/blob/main/specs/protocol/superchain-upgrades.md#holocene>
        Holocene,
        /// Isthmus: <https://github.com/ethereum-optimism/specs/blob/main/specs/protocol/isthmus/overview.md>
        Isthmus,
        /// Jovian: <https://github.com/ethereum-optimism/specs/tree/main/specs/protocol/jovian>
        #[default]
        Jovian,
        /// Karst: <https://github.com/ethereum-optimism/specs/tree/main/specs/protocol/karst>
        Karst,
        /// TODO: add interop hardfork overview when available
        Interop,
    }
);

impl OpHardfork {
    /// Reverse lookup to find the hardfork given a chain ID and block timestamp.
    /// Returns the active hardfork at the given timestamp for the specified OP chain.
    pub fn from_chain_and_timestamp(chain: Chain, timestamp: u64) -> Option<Self> {
        let named = chain.named()?;

        match named {
            NamedChain::Optimism => Some(match timestamp {
                _i if timestamp < OP_MAINNET_CANYON_TIMESTAMP => Self::Regolith,
                _i if timestamp < OP_MAINNET_ECOTONE_TIMESTAMP => Self::Canyon,
                _i if timestamp < OP_MAINNET_FJORD_TIMESTAMP => Self::Ecotone,
                _i if timestamp < OP_MAINNET_GRANITE_TIMESTAMP => Self::Fjord,
                _i if timestamp < OP_MAINNET_HOLOCENE_TIMESTAMP => Self::Granite,
                _i if timestamp < OP_MAINNET_ISTHMUS_TIMESTAMP => Self::Holocene,
                _i if timestamp < OP_MAINNET_JOVIAN_TIMESTAMP => Self::Isthmus,
                _ => Self::Jovian,
            }),
            NamedChain::OptimismSepolia => Some(match timestamp {
                _i if timestamp < OP_SEPOLIA_CANYON_TIMESTAMP => Self::Regolith,
                _i if timestamp < OP_SEPOLIA_ECOTONE_TIMESTAMP => Self::Canyon,
                _i if timestamp < OP_SEPOLIA_FJORD_TIMESTAMP => Self::Ecotone,
                _i if timestamp < OP_SEPOLIA_GRANITE_TIMESTAMP => Self::Fjord,
                _i if timestamp < OP_SEPOLIA_HOLOCENE_TIMESTAMP => Self::Granite,
                _i if timestamp < OP_SEPOLIA_ISTHMUS_TIMESTAMP => Self::Holocene,
                _i if timestamp < OP_SEPOLIA_JOVIAN_TIMESTAMP => Self::Isthmus,
                _ => Self::Jovian,
            }),
            NamedChain::Base => Some(match timestamp {
                _i if timestamp < BASE_MAINNET_CANYON_TIMESTAMP => Self::Regolith,
                _i if timestamp < BASE_MAINNET_ECOTONE_TIMESTAMP => Self::Canyon,
                _i if timestamp < BASE_MAINNET_FJORD_TIMESTAMP => Self::Ecotone,
                _i if timestamp < BASE_MAINNET_GRANITE_TIMESTAMP => Self::Fjord,
                _i if timestamp < BASE_MAINNET_HOLOCENE_TIMESTAMP => Self::Granite,
                _i if timestamp < BASE_MAINNET_ISTHMUS_TIMESTAMP => Self::Holocene,
                _i if timestamp < BASE_MAINNET_JOVIAN_TIMESTAMP => Self::Isthmus,
                _ => Self::Jovian,
            }),
            NamedChain::BaseSepolia => Some(match timestamp {
                _i if timestamp < BASE_SEPOLIA_CANYON_TIMESTAMP => Self::Regolith,
                _i if timestamp < BASE_SEPOLIA_ECOTONE_TIMESTAMP => Self::Canyon,
                _i if timestamp < BASE_SEPOLIA_FJORD_TIMESTAMP => Self::Ecotone,
                _i if timestamp < BASE_SEPOLIA_GRANITE_TIMESTAMP => Self::Fjord,
                _i if timestamp < BASE_SEPOLIA_HOLOCENE_TIMESTAMP => Self::Granite,
                _i if timestamp < BASE_SEPOLIA_ISTHMUS_TIMESTAMP => Self::Holocene,
                _i if timestamp < BASE_SEPOLIA_JOVIAN_TIMESTAMP => Self::Isthmus,
                _ => Self::Jovian,
            }),
            _ => None,
        }
    }

    /// Optimism mainnet list of hardforks.
    pub const fn op_mainnet() -> [(Self, ForkCondition); 9] {
        [
            (Self::Bedrock, ForkCondition::Block(OP_MAINNET_BEDROCK_BLOCK)),
            (Self::Regolith, ForkCondition::Timestamp(OP_MAINNET_REGOLITH_TIMESTAMP)),
            (Self::Canyon, ForkCondition::Timestamp(OP_MAINNET_CANYON_TIMESTAMP)),
            (Self::Ecotone, ForkCondition::Timestamp(OP_MAINNET_ECOTONE_TIMESTAMP)),
            (Self::Fjord, ForkCondition::Timestamp(OP_MAINNET_FJORD_TIMESTAMP)),
            (Self::Granite, ForkCondition::Timestamp(OP_MAINNET_GRANITE_TIMESTAMP)),
            (Self::Holocene, ForkCondition::Timestamp(OP_MAINNET_HOLOCENE_TIMESTAMP)),
            (Self::Isthmus, ForkCondition::Timestamp(OP_MAINNET_ISTHMUS_TIMESTAMP)),
            (Self::Jovian, ForkCondition::Timestamp(OP_MAINNET_JOVIAN_TIMESTAMP)),
        ]
    }

    /// Optimism Sepolia list of hardforks.
    pub const fn op_sepolia() -> [(Self, ForkCondition); 9] {
        [
            (Self::Bedrock, ForkCondition::Block(OP_SEPOLIA_BEDROCK_BLOCK)),
            (Self::Regolith, ForkCondition::Timestamp(OP_SEPOLIA_REGOLITH_TIMESTAMP)),
            (Self::Canyon, ForkCondition::Timestamp(OP_SEPOLIA_CANYON_TIMESTAMP)),
            (Self::Ecotone, ForkCondition::Timestamp(OP_SEPOLIA_ECOTONE_TIMESTAMP)),
            (Self::Fjord, ForkCondition::Timestamp(OP_SEPOLIA_FJORD_TIMESTAMP)),
            (Self::Granite, ForkCondition::Timestamp(OP_SEPOLIA_GRANITE_TIMESTAMP)),
            (Self::Holocene, ForkCondition::Timestamp(OP_SEPOLIA_HOLOCENE_TIMESTAMP)),
            (Self::Isthmus, ForkCondition::Timestamp(OP_SEPOLIA_ISTHMUS_TIMESTAMP)),
            (Self::Jovian, ForkCondition::Timestamp(OP_SEPOLIA_JOVIAN_TIMESTAMP)),
        ]
    }

    /// Base mainnet list of hardforks.
    pub const fn base_mainnet() -> [(Self, ForkCondition); 9] {
        [
            (Self::Bedrock, ForkCondition::Block(BASE_MAINNET_BEDROCK_BLOCK)),
            (Self::Regolith, ForkCondition::Timestamp(BASE_MAINNET_REGOLITH_TIMESTAMP)),
            (Self::Canyon, ForkCondition::Timestamp(BASE_MAINNET_CANYON_TIMESTAMP)),
            (Self::Ecotone, ForkCondition::Timestamp(BASE_MAINNET_ECOTONE_TIMESTAMP)),
            (Self::Fjord, ForkCondition::Timestamp(BASE_MAINNET_FJORD_TIMESTAMP)),
            (Self::Granite, ForkCondition::Timestamp(BASE_MAINNET_GRANITE_TIMESTAMP)),
            (Self::Holocene, ForkCondition::Timestamp(BASE_MAINNET_HOLOCENE_TIMESTAMP)),
            (Self::Isthmus, ForkCondition::Timestamp(BASE_MAINNET_ISTHMUS_TIMESTAMP)),
            (Self::Jovian, ForkCondition::Timestamp(BASE_MAINNET_JOVIAN_TIMESTAMP)),
        ]
    }

    /// Base Sepolia list of hardforks.
    pub const fn base_sepolia() -> [(Self, ForkCondition); 9] {
        [
            (Self::Bedrock, ForkCondition::Block(BASE_SEPOLIA_BEDROCK_BLOCK)),
            (Self::Regolith, ForkCondition::Timestamp(BASE_SEPOLIA_REGOLITH_TIMESTAMP)),
            (Self::Canyon, ForkCondition::Timestamp(BASE_SEPOLIA_CANYON_TIMESTAMP)),
            (Self::Ecotone, ForkCondition::Timestamp(BASE_SEPOLIA_ECOTONE_TIMESTAMP)),
            (Self::Fjord, ForkCondition::Timestamp(BASE_SEPOLIA_FJORD_TIMESTAMP)),
            (Self::Granite, ForkCondition::Timestamp(BASE_SEPOLIA_GRANITE_TIMESTAMP)),
            (Self::Holocene, ForkCondition::Timestamp(BASE_SEPOLIA_HOLOCENE_TIMESTAMP)),
            (Self::Isthmus, ForkCondition::Timestamp(BASE_SEPOLIA_ISTHMUS_TIMESTAMP)),
            (Self::Jovian, ForkCondition::Timestamp(BASE_SEPOLIA_JOVIAN_TIMESTAMP)),
        ]
    }

    /// Returns index of `self` in sorted canonical array.
    pub const fn idx(&self) -> usize {
        *self as usize
    }
}

/// Extends [`EthereumHardforks`] with optimism helper methods.
#[auto_impl::auto_impl(&, Arc)]
pub trait OpHardforks: EthereumHardforks {
    /// Retrieves [`ForkCondition`] by an [`OpHardfork`]. If `fork` is not present, returns
    /// [`ForkCondition::Never`].
    fn op_fork_activation(&self, fork: OpHardfork) -> ForkCondition;

    /// Convenience method to check if [`OpHardfork::Bedrock`] is active at a given block
    /// number.
    fn is_bedrock_active_at_block(&self, block_number: u64) -> bool {
        self.op_fork_activation(OpHardfork::Bedrock).active_at_block(block_number)
    }

    /// Returns `true` if [`Regolith`](OpHardfork::Regolith) is active at given block
    /// timestamp.
    fn is_regolith_active_at_timestamp(&self, timestamp: u64) -> bool {
        self.op_fork_activation(OpHardfork::Regolith).active_at_timestamp(timestamp)
    }

    /// Returns `true` if [`Canyon`](OpHardfork::Canyon) is active at given block timestamp.
    fn is_canyon_active_at_timestamp(&self, timestamp: u64) -> bool {
        self.op_fork_activation(OpHardfork::Canyon).active_at_timestamp(timestamp)
    }

    /// Returns `true` if [`Ecotone`](OpHardfork::Ecotone) is active at given block timestamp.
    fn is_ecotone_active_at_timestamp(&self, timestamp: u64) -> bool {
        self.op_fork_activation(OpHardfork::Ecotone).active_at_timestamp(timestamp)
    }

    /// Returns `true` if [`Fjord`](OpHardfork::Fjord) is active at given block timestamp.
    fn is_fjord_active_at_timestamp(&self, timestamp: u64) -> bool {
        self.op_fork_activation(OpHardfork::Fjord).active_at_timestamp(timestamp)
    }

    /// Returns `true` if [`Granite`](OpHardfork::Granite) is active at given block timestamp.
    fn is_granite_active_at_timestamp(&self, timestamp: u64) -> bool {
        self.op_fork_activation(OpHardfork::Granite).active_at_timestamp(timestamp)
    }

    /// Returns `true` if [`Holocene`](OpHardfork::Holocene) is active at given block
    /// timestamp.
    fn is_holocene_active_at_timestamp(&self, timestamp: u64) -> bool {
        self.op_fork_activation(OpHardfork::Holocene).active_at_timestamp(timestamp)
    }

    /// Returns `true` if [`Isthmus`](OpHardfork::Isthmus) is active at given block
    /// timestamp.
    fn is_isthmus_active_at_timestamp(&self, timestamp: u64) -> bool {
        self.op_fork_activation(OpHardfork::Isthmus).active_at_timestamp(timestamp)
    }

    /// Returns `true` if [`Jovian`](OpHardfork::Jovian) is active at given block
    /// timestamp.
    fn is_jovian_active_at_timestamp(&self, timestamp: u64) -> bool {
        self.op_fork_activation(OpHardfork::Jovian).active_at_timestamp(timestamp)
    }

    /// Returns `true` if [`Karst`](OpHardfork::Karst) is active at given block
    /// timestamp.
    fn is_karst_active_at_timestamp(&self, timestamp: u64) -> bool {
        self.op_fork_activation(OpHardfork::Karst).active_at_timestamp(timestamp)
    }

    /// Returns `true` if [`Interop`](OpHardfork::Interop) is active at given block
    /// timestamp.
    fn is_interop_active_at_timestamp(&self, timestamp: u64) -> bool {
        self.op_fork_activation(OpHardfork::Interop).active_at_timestamp(timestamp)
    }
}

/// A type allowing to configure activation [`ForkCondition`]s for a given list of
/// [`OpHardfork`]s.
///
/// Zips together [`EthereumHardfork`]s and [`OpHardfork`]s. Optimism hard forks, at least,
/// whenever Ethereum hard forks. When Ethereum hard forks, a new [`OpHardfork`] piggybacks on top
/// of the new [`EthereumHardfork`] to include (or to noop) the L1 changes on L2.
///
/// Optimism can also hard fork independently of Ethereum. The relation between Ethereum and
/// Optimism hard forks is described by predicate [`EthereumHardfork`] `=>` [`OpHardfork`], since
/// an OP chain can undergo an [`OpHardfork`] without an [`EthereumHardfork`], but not the other
/// way around.
#[derive(Debug, Clone)]
pub struct OpChainHardforks {
    /// Ordered list of OP hardfork activations.
    forks: Vec<(OpHardfork, ForkCondition)>,
}

impl OpChainHardforks {
    /// Creates a new [`OpChainHardforks`] with the given list of forks. The input list is sorted
    /// w.r.t. the hardcoded canonicity of [`OpHardfork`]s.
    pub fn new(forks: impl IntoIterator<Item = (OpHardfork, ForkCondition)>) -> Self {
        let mut forks = forks.into_iter().collect::<Vec<_>>();
        forks.sort();
        Self { forks }
    }

    /// Creates a new [`OpChainHardforks`] with OP mainnet configuration.
    pub fn op_mainnet() -> Self {
        Self::new(OpHardfork::op_mainnet())
    }

    /// Creates a new [`OpChainHardforks`] with OP Sepolia configuration.
    pub fn op_sepolia() -> Self {
        Self::new(OpHardfork::op_sepolia())
    }

    /// Creates a new [`OpChainHardforks`] with Base mainnet configuration.
    pub fn base_mainnet() -> Self {
        Self::new(OpHardfork::base_mainnet())
    }

    /// Creates a new [`OpChainHardforks`] with Base Sepolia configuration.
    pub fn base_sepolia() -> Self {
        Self::new(OpHardfork::base_sepolia())
    }

    /// Returns `true` if this is an OP mainnet instance.
    pub fn is_op_mainnet(&self) -> bool {
        self[OpHardfork::Bedrock] == ForkCondition::Block(OP_MAINNET_BEDROCK_BLOCK)
    }
}

impl EthereumHardforks for OpChainHardforks {
    fn ethereum_fork_activation(&self, fork: EthereumHardfork) -> ForkCondition {
        use EthereumHardfork::{Cancun, Osaka, Prague, Shanghai};
        use OpHardfork::{Canyon, Ecotone, Isthmus, Karst};

        if self.forks.is_empty() {
            return ForkCondition::Never;
        }

        let forks_len = self.forks.len();
        // check index out of bounds
        match fork {
            Shanghai if forks_len <= Canyon.idx() => ForkCondition::Never,
            Cancun if forks_len <= Ecotone.idx() => ForkCondition::Never,
            Prague if forks_len <= Isthmus.idx() => ForkCondition::Never,
            Osaka if forks_len <= Karst.idx() => ForkCondition::Never,
            _ => self[fork],
        }
    }
}

impl OpHardforks for OpChainHardforks {
    fn op_fork_activation(&self, fork: OpHardfork) -> ForkCondition {
        // check index out of bounds
        if self.forks.len() <= fork.idx() {
            return ForkCondition::Never;
        }
        self[fork]
    }
}

impl Index<OpHardfork> for OpChainHardforks {
    type Output = ForkCondition;

    fn index(&self, hf: OpHardfork) -> &Self::Output {
        use OpHardfork::{
            Bedrock, Canyon, Ecotone, Fjord, Granite, Holocene, Interop, Isthmus, Jovian, Karst,
            Regolith,
        };

        match hf {
            Bedrock => &self.forks[Bedrock.idx()].1,
            Regolith => &self.forks[Regolith.idx()].1,
            Canyon => &self.forks[Canyon.idx()].1,
            Ecotone => &self.forks[Ecotone.idx()].1,
            Fjord => &self.forks[Fjord.idx()].1,
            Granite => &self.forks[Granite.idx()].1,
            Holocene => &self.forks[Holocene.idx()].1,
            Isthmus => &self.forks[Isthmus.idx()].1,
            Jovian => &self.forks[Jovian.idx()].1,
            Karst => &self.forks[Karst.idx()].1,
            Interop => &self.forks[Interop.idx()].1,
        }
    }
}

impl Index<EthereumHardfork> for OpChainHardforks {
    type Output = ForkCondition;

    fn index(&self, hf: EthereumHardfork) -> &Self::Output {
        use EthereumHardfork::{
            Amsterdam, ArrowGlacier, Berlin, Bpo1, Bpo2, Bpo3, Bpo4, Bpo5, Byzantium, Cancun,
            Constantinople, Dao, Frontier, GrayGlacier, Homestead, Istanbul, London, MuirGlacier,
            Osaka, Paris, Petersburg, Prague, Shanghai, SpuriousDragon, Tangerine,
        };
        use OpHardfork::{Bedrock, Canyon, Ecotone, Isthmus, Karst};

        match hf {
            // Dao Hardfork is not needed for OpChainHardforks
            Dao | Bpo1 | Bpo2 | Bpo3 | Bpo4 | Bpo5 | Amsterdam => &ForkCondition::Never,
            Berlin if self.is_op_mainnet() => &ForkCondition::Block(OP_MAINNET_BERLIN_BLOCK),
            Frontier | Homestead | Tangerine | SpuriousDragon | Byzantium | Constantinople
            | Petersburg | Istanbul | MuirGlacier | Berlin => &ForkCondition::ZERO_BLOCK,
            London | ArrowGlacier | GrayGlacier => &self[Bedrock],
            Paris if self.is_op_mainnet() => &ForkCondition::TTD {
                activation_block_number: OP_MAINNET_BEDROCK_BLOCK,
                fork_block: Some(OP_MAINNET_BEDROCK_BLOCK),
                total_difficulty: U256::ZERO,
            },
            Paris => &ForkCondition::TTD {
                activation_block_number: 0,
                fork_block: Some(0),
                total_difficulty: U256::ZERO,
            },
            Shanghai => &self[Canyon],
            Cancun => &self[Ecotone],
            Prague => &self[Isthmus],
            Osaka => &self[Karst],
            _ => unreachable!(),
        }
    }
}
