//! Contains the `[OpSpecId]` type and its implementation.
use core::str::FromStr;
use revm::primitives::hardfork::{SpecId, UnknownHardfork};

/// Optimism spec id.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[allow(non_camel_case_types)]
pub enum OpSpecId {
    /// Bedrock spec id.
    BEDROCK = 100,
    /// Regolith spec id.
    REGOLITH,
    /// Canyon spec id.
    CANYON,
    /// Ecotone spec id.
    ECOTONE,
    /// Fjord spec id.
    FJORD,
    /// Granite spec id.
    GRANITE,
    /// Holocene spec id.
    HOLOCENE,
    /// Isthmus spec id.
    ISTHMUS,
    /// Jovian spec id.
    #[default]
    JOVIAN,
    /// Karst spec id.
    KARST,
    /// Interop spec id.
    INTEROP,
}

impl OpSpecId {
    /// Converts the [`OpSpecId`] into a [`SpecId`].
    pub const fn into_eth_spec(self) -> SpecId {
        match self {
            Self::BEDROCK | Self::REGOLITH => SpecId::MERGE,
            Self::CANYON => SpecId::SHANGHAI,
            Self::ECOTONE | Self::FJORD | Self::GRANITE | Self::HOLOCENE => SpecId::CANCUN,
            Self::ISTHMUS | Self::JOVIAN | Self::INTEROP => SpecId::PRAGUE,
            Self::KARST => SpecId::OSAKA,
        }
    }

    /// Checks if the [`OpSpecId`] is enabled in the other [`OpSpecId`].
    pub const fn is_enabled_in(self, other: Self) -> bool {
        other as u8 <= self as u8
    }
}

impl From<OpSpecId> for SpecId {
    fn from(spec: OpSpecId) -> Self {
        spec.into_eth_spec()
    }
}

impl FromStr for OpSpecId {
    type Err = UnknownHardfork;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            name::BEDROCK => Ok(Self::BEDROCK),
            name::REGOLITH => Ok(Self::REGOLITH),
            name::CANYON => Ok(Self::CANYON),
            name::ECOTONE => Ok(Self::ECOTONE),
            name::FJORD => Ok(Self::FJORD),
            name::GRANITE => Ok(Self::GRANITE),
            name::HOLOCENE => Ok(Self::HOLOCENE),
            name::ISTHMUS => Ok(Self::ISTHMUS),
            name::JOVIAN => Ok(Self::JOVIAN),
            name::KARST => Ok(Self::KARST),
            name::INTEROP => Ok(Self::INTEROP),
            _ => Err(UnknownHardfork),
        }
    }
}

impl From<OpSpecId> for &'static str {
    fn from(spec_id: OpSpecId) -> Self {
        match spec_id {
            OpSpecId::BEDROCK => name::BEDROCK,
            OpSpecId::REGOLITH => name::REGOLITH,
            OpSpecId::CANYON => name::CANYON,
            OpSpecId::ECOTONE => name::ECOTONE,
            OpSpecId::FJORD => name::FJORD,
            OpSpecId::GRANITE => name::GRANITE,
            OpSpecId::HOLOCENE => name::HOLOCENE,
            OpSpecId::ISTHMUS => name::ISTHMUS,
            OpSpecId::JOVIAN => name::JOVIAN,
            OpSpecId::KARST => name::KARST,
            OpSpecId::INTEROP => name::INTEROP,
        }
    }
}

/// String identifiers for Optimism hardforks
pub mod name {
    /// Bedrock spec name.
    pub const BEDROCK: &str = "Bedrock";
    /// Regolith spec name.
    pub const REGOLITH: &str = "Regolith";
    /// Canyon spec name.
    pub const CANYON: &str = "Canyon";
    /// Ecotone spec name.
    pub const ECOTONE: &str = "Ecotone";
    /// Fjord spec name.
    pub const FJORD: &str = "Fjord";
    /// Granite spec name.
    pub const GRANITE: &str = "Granite";
    /// Holocene spec name.
    pub const HOLOCENE: &str = "Holocene";
    /// Isthmus spec name.
    pub const ISTHMUS: &str = "Isthmus";
    /// Jovian spec name.
    pub const JOVIAN: &str = "Jovian";
    /// Karst spec name.
    pub const KARST: &str = "Karst";
    /// Interop spec name.
    pub const INTEROP: &str = "Interop";
}
