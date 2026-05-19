use foundry_compilers_artifacts_solc::{
    EvmVersion, output_selection::OutputSelection, serde_helpers,
};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

pub const VYPER_SEARCH_PATHS: Version = VYPER_0_4;
pub const VYPER_BERLIN: Version = Version::new(0, 3, 0);
pub const VYPER_PARIS: Version = Version::new(0, 3, 7);
pub const VYPER_SHANGHAI: Version = Version::new(0, 3, 8);
pub const VYPER_CANCUN: Version = Version::new(0, 3, 8);
pub const VYPER_PRAGUE: Version = Version::new(0, 4, 3);

const VYPER_0_4: Version = Version::new(0, 4, 0);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VyperOptimizationMode {
    Gas,
    Codesize,
    None,
    #[serde(rename = "O1", alias = "1", alias = "o1")]
    O1,
    #[serde(rename = "O2", alias = "2", alias = "o2")]
    O2,
    #[serde(rename = "O3", alias = "3", alias = "o3")]
    O3,
    #[serde(rename = "Os", alias = "s", alias = "os")]
    Os,
}

/// Vyper parses `optimize` and `optLevel` through the same optimization level parser.
pub type VyperOptimizationLevel = VyperOptimizationMode;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VyperVenomSettings {
    #[serde(alias = "disable_inlining", skip_serializing_if = "Option::is_none")]
    pub disable_inlining: Option<bool>,
    #[serde(rename = "disableCSE", alias = "disable_cse", skip_serializing_if = "Option::is_none")]
    pub disable_cse: Option<bool>,
    #[serde(
        rename = "disableSCCP",
        alias = "disable_sccp",
        skip_serializing_if = "Option::is_none"
    )]
    pub disable_sccp: Option<bool>,
    #[serde(alias = "disable_load_elimination", skip_serializing_if = "Option::is_none")]
    pub disable_load_elimination: Option<bool>,
    #[serde(alias = "disable_dead_store_elimination", skip_serializing_if = "Option::is_none")]
    pub disable_dead_store_elimination: Option<bool>,
    #[serde(alias = "disable_algebraic_optimization", skip_serializing_if = "Option::is_none")]
    pub disable_algebraic_optimization: Option<bool>,
    #[serde(alias = "disable_branch_optimization", skip_serializing_if = "Option::is_none")]
    pub disable_branch_optimization: Option<bool>,
    #[serde(alias = "disable_assert_elimination", skip_serializing_if = "Option::is_none")]
    pub disable_assert_elimination: Option<bool>,
    #[serde(
        rename = "disableMem2Var",
        alias = "disable_mem2var",
        skip_serializing_if = "Option::is_none"
    )]
    pub disable_mem2var: Option<bool>,
    #[serde(
        rename = "disableSimplifyCFG",
        alias = "disable_simplify_cfg",
        skip_serializing_if = "Option::is_none"
    )]
    pub disable_simplify_cfg: Option<bool>,
    #[serde(alias = "disable_remove_unused_variables", skip_serializing_if = "Option::is_none")]
    pub disable_remove_unused_variables: Option<bool>,
    #[serde(alias = "inline_threshold", skip_serializing_if = "Option::is_none")]
    pub inline_threshold: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VyperSettings {
    #[serde(
        default,
        with = "serde_helpers::display_from_str_opt",
        skip_serializing_if = "Option::is_none"
    )]
    pub evm_version: Option<EvmVersion>,
    /// Optimization mode
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optimize: Option<VyperOptimizationMode>,
    /// Numeric optimization level
    #[serde(rename = "optLevel", alias = "opt_level", skip_serializing_if = "Option::is_none")]
    pub opt_level: Option<VyperOptimizationLevel>,
    /// Whether or not the bytecode should include Vyper's signature
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytecode_metadata: Option<bool>,
    pub output_selection: OutputSelection,
    #[serde(rename = "search_paths", skip_serializing_if = "Option::is_none")]
    pub search_paths: Option<BTreeSet<PathBuf>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental_codegen: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<bool>,
    /// Vyper standard JSON intentionally keeps this key snake_case.
    #[serde(
        rename = "enable_decimals",
        alias = "enableDecimals",
        skip_serializing_if = "Option::is_none"
    )]
    pub enable_decimals: Option<bool>,
    #[serde(
        rename = "venomExperimental",
        alias = "venom_experimental",
        skip_serializing_if = "Option::is_none"
    )]
    pub venom_experimental: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub venom: Option<VyperVenomSettings>,
}

impl VyperSettings {
    pub fn strip_prefix(&mut self, base: &Path) {
        self.output_selection = OutputSelection(
            std::mem::take(&mut self.output_selection.0)
                .into_iter()
                .map(|(file, selection)| {
                    (
                        Path::new(&file)
                            .strip_prefix(base)
                            .map(|p| p.display().to_string())
                            .unwrap_or(file),
                        selection,
                    )
                })
                .collect(),
        );
        self.search_paths = self.search_paths.as_ref().map(|paths| {
            paths.iter().map(|p| p.strip_prefix(base).unwrap_or(p.as_path()).into()).collect()
        });
    }

    /// Sanitize the output selection.
    #[allow(clippy::collapsible_if)]
    pub fn sanitize_output_selection(&mut self, version: &Version) {
        for selection in self.output_selection.0.values_mut() {
            for selection in selection.values_mut() {
                // During caching we prune output selection for some of the sources, however, Vyper
                // will reject `[]` as an output selection, so we are adding "abi" as a default
                // output selection which is cheap to be produced.
                if selection.is_empty() {
                    selection.push("abi".to_string())
                }

                // Unsupported selections.
                #[rustfmt::skip]
                selection.retain(|selection| {
                    if *version < VYPER_0_4 {
                        if matches!(
                            selection.as_str(),
                            | "evm.bytecode.sourceMap" | "evm.deployedBytecode.sourceMap"
                        ) {
                            return false;
                        }
                    }

                    if matches!(
                        selection.as_str(),
                        | "evm.bytecode.sourceMap" | "evm.deployedBytecode.sourceMap"
                        // https://github.com/vyperlang/vyper/issues/4389
                        | "evm.bytecode.linkReferences" | "evm.deployedBytecode.linkReferences"
                        | "evm.deployedBytecode.immutableReferences"
                    ) {
                        return false;
                    }

                    true
                });
            }
        }
    }

    /// Sanitize the settings based on the compiler version.
    pub fn sanitize(&mut self, version: &Version) {
        if version < &VYPER_SEARCH_PATHS {
            self.search_paths = None;
        }

        self.sanitize_output_selection(version);
        self.normalize_evm_version(version);
    }

    /// Sanitize the settings based on the compiler version.
    pub fn sanitized(mut self, version: &Version) -> Self {
        self.sanitize(version);
        self
    }

    /// Adjusts the EVM version based on the compiler version.
    pub fn normalize_evm_version(&mut self, version: &Version) {
        if let Some(evm_version) = &mut self.evm_version {
            *evm_version = if *evm_version >= EvmVersion::Prague && *version >= VYPER_PRAGUE {
                EvmVersion::Prague
            } else if *evm_version >= EvmVersion::Cancun && *version >= VYPER_CANCUN {
                EvmVersion::Cancun
            } else if *evm_version >= EvmVersion::Shanghai && *version >= VYPER_SHANGHAI {
                EvmVersion::Shanghai
            } else if *evm_version >= EvmVersion::Paris && *version >= VYPER_PARIS {
                EvmVersion::Paris
            } else if *evm_version >= EvmVersion::Berlin && *version >= VYPER_BERLIN {
                EvmVersion::Berlin
            } else {
                *evm_version
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serializes_extended_vyper_settings() {
        let settings = VyperSettings {
            opt_level: Some(VyperOptimizationLevel::Os),
            debug: Some(true),
            enable_decimals: Some(true),
            venom_experimental: Some(true),
            venom: Some(VyperVenomSettings {
                disable_inlining: Some(true),
                disable_cse: Some(true),
                disable_sccp: Some(false),
                disable_load_elimination: Some(true),
                disable_dead_store_elimination: Some(false),
                disable_algebraic_optimization: Some(true),
                disable_branch_optimization: Some(false),
                disable_assert_elimination: Some(true),
                disable_mem2var: Some(false),
                disable_simplify_cfg: Some(true),
                disable_remove_unused_variables: Some(false),
                inline_threshold: Some(15),
            }),
            ..Default::default()
        };

        let value = serde_json::to_value(settings).unwrap();

        assert_eq!(value["optLevel"], json!("Os"));
        assert_eq!(value["debug"], json!(true));
        assert_eq!(value["enable_decimals"], json!(true));
        assert_eq!(value["venomExperimental"], json!(true));
        assert_eq!(value["venom"]["disableInlining"], json!(true));
        assert_eq!(value["venom"]["disableCSE"], json!(true));
        assert_eq!(value["venom"]["disableSCCP"], json!(false));
        assert_eq!(value["venom"]["disableMem2Var"], json!(false));
        assert_eq!(value["venom"]["disableSimplifyCFG"], json!(true));
        assert_eq!(value["venom"]["disableAssertElimination"], json!(true));
        assert_eq!(value["venom"]["inlineThreshold"], json!(15));
    }

    #[test]
    fn deserializes_optimization_level_aliases() {
        assert_eq!(
            serde_json::from_value::<VyperOptimizationMode>(json!("1")).unwrap(),
            VyperOptimizationMode::O1
        );
        assert_eq!(
            serde_json::from_value::<VyperOptimizationMode>(json!("O2")).unwrap(),
            VyperOptimizationMode::O2
        );
        assert_eq!(
            serde_json::from_value::<VyperOptimizationMode>(json!("o3")).unwrap(),
            VyperOptimizationMode::O3
        );
        assert_eq!(serde_json::to_value(VyperOptimizationMode::O1).unwrap(), json!("O1"));
        assert_eq!(
            serde_json::from_value::<VyperOptimizationLevel>(json!("s")).unwrap(),
            VyperOptimizationLevel::Os
        );
        assert_eq!(serde_json::to_value(VyperOptimizationLevel::O3).unwrap(), json!("O3"));
    }

    #[test]
    fn deserializes_snake_case_config_aliases() {
        let value = json!({
            "outputSelection": {},
            "opt_level": "3",
            "enableDecimals": true,
            "venom_experimental": true,
            "venom": {
                "disable_cse": true,
                "disable_sccp": false,
                "disable_mem2var": true,
                "disable_simplify_cfg": false,
                "inline_threshold": 15
            }
        });

        let settings = serde_json::from_value::<VyperSettings>(value).unwrap();

        assert_eq!(settings.opt_level, Some(VyperOptimizationLevel::O3));
        assert_eq!(settings.enable_decimals, Some(true));
        assert_eq!(settings.venom_experimental, Some(true));
        let venom = settings.venom.unwrap();
        assert_eq!(venom.disable_cse, Some(true));
        assert_eq!(venom.disable_sccp, Some(false));
        assert_eq!(venom.disable_mem2var, Some(true));
        assert_eq!(venom.disable_simplify_cfg, Some(false));
        assert_eq!(venom.inline_threshold, Some(15));
    }
}
