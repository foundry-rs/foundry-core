//! Vyper artifact types.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod settings;
pub use settings::{
    VyperOptimizationLevel, VyperOptimizationMode, VyperSettings, VyperVenomSettings,
};

mod error;
pub use error::VyperCompilationError;

mod input;
pub use input::VyperInput;

mod output;
pub use output::VyperOutput;
