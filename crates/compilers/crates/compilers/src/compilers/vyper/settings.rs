use std::{collections::BTreeSet, path::PathBuf};

pub use crate::artifacts::vyper::VyperSettings;
use crate::{
    compilers::{CompilerSettings, restrictions::CompilerSettingsRestrictions},
    solc::Restriction,
};
use foundry_compilers_artifacts::{EvmVersion, output_selection::OutputSelection};

#[derive(Clone, Copy, Debug, Default)]
pub struct VyperRestrictions {
    pub evm_version: Restriction<EvmVersion>,
}

impl CompilerSettingsRestrictions for VyperRestrictions {
    fn merge(self, other: Self) -> Option<Self> {
        Some(Self { evm_version: self.evm_version.merge(other.evm_version)? })
    }
}

impl CompilerSettings for VyperSettings {
    type Restrictions = VyperRestrictions;

    fn update_output_selection(&mut self, mut f: impl FnMut(&mut OutputSelection)) {
        f(&mut self.output_selection);
    }

    fn can_use_cached(&self, other: &Self) -> bool {
        let Self {
            evm_version,
            optimize,
            opt_level,
            bytecode_metadata,
            output_selection,
            search_paths,
            experimental_codegen,
            debug,
            enable_decimals,
            venom_experimental,
            venom,
        } = self;
        evm_version == &other.evm_version
            && optimize == &other.optimize
            && opt_level == &other.opt_level
            && bytecode_metadata == &other.bytecode_metadata
            && output_selection.is_subset_of(&other.output_selection)
            && search_paths == &other.search_paths
            && experimental_codegen == &other.experimental_codegen
            && debug == &other.debug
            && enable_decimals == &other.enable_decimals
            && venom_experimental == &other.venom_experimental
            && venom == &other.venom
    }

    fn with_include_paths(mut self, include_paths: &BTreeSet<PathBuf>) -> Self {
        self.search_paths = Some(include_paths.clone());
        self
    }

    fn satisfies_restrictions(&self, restrictions: &Self::Restrictions) -> bool {
        restrictions.evm_version.satisfies(self.evm_version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifacts::vyper::{VyperOptimizationLevel, VyperVenomSettings};

    #[test]
    fn cache_key_includes_extended_vyper_settings() {
        let base = VyperSettings::default();

        let mut changed = base.clone();
        changed.opt_level = Some(VyperOptimizationLevel::O3);
        assert!(!base.can_use_cached(&changed));

        let mut changed = base.clone();
        changed.debug = Some(true);
        assert!(!base.can_use_cached(&changed));

        let mut changed = base.clone();
        changed.enable_decimals = Some(true);
        assert!(!base.can_use_cached(&changed));

        let mut changed = base.clone();
        changed.venom_experimental = Some(true);
        assert!(!base.can_use_cached(&changed));

        let mut changed = base;
        changed.venom = Some(VyperVenomSettings { disable_cse: Some(true), ..Default::default() });
        assert!(!VyperSettings::default().can_use_cached(&changed));
    }
}
