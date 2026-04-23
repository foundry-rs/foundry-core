//! Contains the `[OpHaltReason]` type.
use revm::context_interface::result::HaltReason;

/// Optimism halt reason.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum OpHaltReason {
    /// Base halt reason.
    Base(HaltReason),
    /// Failed deposit halt reason.
    FailedDeposit,
}

impl From<HaltReason> for OpHaltReason {
    fn from(value: HaltReason) -> Self {
        Self::Base(value)
    }
}

impl TryFrom<OpHaltReason> for HaltReason {
    type Error = OpHaltReason;

    fn try_from(value: OpHaltReason) -> Result<Self, OpHaltReason> {
        match value {
            OpHaltReason::Base(reason) => Ok(reason),
            OpHaltReason::FailedDeposit => Err(value),
        }
    }
}
