use super::{
    CompilationError, Compiler, CompilerInput, CompilerOutput, CompilerSettings, CompilerVersion,
    Language, ParsedSource, restrictions::CompilerSettingsRestrictions,
};
use crate::{
    SourceParser,
    resolver::{
        Node,
        parse::{SolData, SolParser},
    },
};
use foundry_compilers_artifacts::{
    BytecodeHash, Contract, Error, EvmVersion, Settings, Severity, SolcInput, SourceUnitName,
    error::SourceLocation,
    output_selection::OutputSelection,
    remappings::Remapping,
    sources::{Source, Sources},
};
use foundry_compilers_core::error::{Result, SolcError, SolcIoError};
use rayon::prelude::*;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, HashSet},
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
};

pub use foundry_compilers_artifacts::SolcLanguage;

mod compiler;
pub use compiler::{SOLC_EXTENSIONS, Solc};

#[derive(Clone, Debug)]
#[cfg_attr(feature = "svm-solc", derive(Default))]
pub enum SolcCompiler {
    #[default]
    #[cfg(feature = "svm-solc")]
    AutoDetect,

    Specific(Solc),
}

impl Language for SolcLanguage {
    const FILE_EXTENSIONS: &'static [&'static str] = SOLC_EXTENSIONS;
}

impl Compiler for SolcCompiler {
    type Input = SolcVersionedInput;
    type CompilationError = Error;
    type Parser = SolParser;
    type Settings = SolcSettings;
    type Language = SolcLanguage;
    type CompilerContract = Contract;

    fn compile(
        &self,
        input: &Self::Input,
    ) -> Result<CompilerOutput<Self::CompilationError, Self::CompilerContract>> {
        let mut solc = match self {
            Self::Specific(solc) => solc.clone(),

            #[cfg(feature = "svm-solc")]
            Self::AutoDetect => Solc::find_or_install(&input.version)?,
        };
        solc.base_path.clone_from(&input.cli_settings.base_path);
        solc.allow_paths.clone_from(&input.cli_settings.allow_paths);
        solc.include_paths.clone_from(&input.cli_settings.include_paths);
        solc.extra_args.extend_from_slice(&input.cli_settings.extra_args);

        let solc_output = solc.compile(&input.input)?;
        compiler_output(&solc, input, solc_output)
    }

    fn available_versions(&self, _language: &Self::Language) -> Vec<CompilerVersion> {
        match self {
            Self::Specific(solc) => vec![CompilerVersion::Installed(Version::new(
                solc.version.major,
                solc.version.minor,
                solc.version.patch,
            ))],

            #[cfg(feature = "svm-solc")]
            Self::AutoDetect => {
                let mut all_versions = Solc::installed_versions()
                    .into_iter()
                    .map(CompilerVersion::Installed)
                    .collect::<Vec<_>>();
                let mut uniques = all_versions
                    .iter()
                    .map(|v| {
                        let v = v.as_ref();
                        (v.major, v.minor, v.patch)
                    })
                    .collect::<std::collections::HashSet<_>>();
                all_versions.extend(
                    Solc::released_versions()
                        .into_iter()
                        .filter(|v| uniques.insert((v.major, v.minor, v.patch)))
                        .map(CompilerVersion::Remote),
                );
                all_versions.sort_unstable();
                all_versions
            }
        }
    }
}

fn compiler_output(
    solc: &Solc,
    input: &SolcVersionedInput,
    output: foundry_compilers_artifacts::CompilerOutput,
) -> Result<CompilerOutput<Error, Contract>> {
    let build_info = lossless_build_info(solc, input, &output)?;
    let foundry_compilers_artifacts::CompilerOutput { errors, sources, contracts } = output;
    Ok(CompilerOutput {
        errors,
        sources: filesystem_projection(&input.input, sources),
        contracts: filesystem_projection(&input.input, contracts),
        metadata: BTreeMap::new(),
        build_info,
    })
}

fn filesystem_projection<T>(
    input: &SolcInput,
    output: BTreeMap<SourceUnitName, T>,
) -> BTreeMap<PathBuf, T> {
    let input_names = input.sources.keys().filter_map(|path| path.to_str()).collect::<HashSet<_>>();
    let (exact, aliases): (Vec<_>, Vec<_>) =
        output.into_iter().partition(|(name, _)| input_names.contains(name.as_str()));
    let mut selected = BTreeMap::new();

    // Insert exact input names last so they deterministically represent path-equivalent source
    // units in the filesystem-oriented output used for artifacts and caching.
    for (name, value) in aliases.into_iter().chain(exact) {
        let path = PathBuf::from(name.as_str());
        selected.remove(&path);
        selected.insert(path, value);
    }

    selected
}

fn lossless_build_info(
    solc: &Solc,
    input: &SolcVersionedInput,
    output: &foundry_compilers_artifacts::CompilerOutput,
) -> Result<Option<Box<super::BuildInfoPayload>>> {
    if !path_projection_is_lossy(&output.sources) && !path_projection_is_lossy(&output.contracts) {
        return Ok(None);
    }

    let mut serialized_input = serde_json::to_value(input)?;
    let serialized_output = serde_json::to_value(output)?;
    let input_sources = serialized_input
        .get_mut("sources")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| SolcError::msg("compiler input does not contain a sources object"))?;
    let input_names = input_sources.keys().cloned().collect::<Vec<_>>();

    // Exact input names use the potentially preprocessed standard JSON content. Path-equivalent
    // aliases were loaded separately by Solc's filesystem callback, so recover their disk content
    // using the same search roots rather than assuming it is identical to the input.
    for output_name in output.sources.keys() {
        if input_sources.contains_key(output_name.as_str()) {
            continue;
        }

        let mut matches =
            input_names.iter().filter(|input_name| Path::new(input_name) == output_name.as_path());
        let Some(_) = matches.next() else { continue };
        if matches.next().is_some() {
            return Err(SolcError::msg(format!(
                "multiple compiler input sources match output source unit `{output_name}`"
            )));
        }
        let content = solc.read_source_unit(output_name.as_str())?;
        input_sources.insert(output_name.to_string(), serde_json::json!({ "content": content }));
    }

    let source_id_to_path = output
        .sources
        .iter()
        .map(|(name, source)| (source.id, PathBuf::from(name.as_str())))
        .collect();
    Ok(Some(Box::new(super::BuildInfoPayload {
        input: serialized_input,
        output: serialized_output,
        source_id_to_path,
    })))
}

fn path_projection_is_lossy<T>(output: &BTreeMap<SourceUnitName, T>) -> bool {
    let mut paths = BTreeSet::new();
    output.keys().any(|name| !paths.insert(name.as_path()))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SolcVersionedInput {
    pub version: Version,
    #[serde(flatten)]
    pub input: SolcInput,
    #[serde(flatten)]
    pub cli_settings: CliSettings,
}

impl CompilerInput for SolcVersionedInput {
    type Settings = SolcSettings;
    type Language = SolcLanguage;

    /// Creates a new [CompilerInput]s with default settings and the given sources
    ///
    /// A [CompilerInput] expects a language setting, supported by solc are solidity or yul.
    /// In case the `sources` is a mix of solidity and yul files, 2 CompilerInputs are returned
    fn build(
        sources: Sources,
        settings: Self::Settings,
        language: Self::Language,
        version: Version,
    ) -> Self {
        let SolcSettings { settings, cli_settings } = settings;
        let input = SolcInput::new(language, sources, settings).sanitized(&version);

        Self { version, input, cli_settings }
    }

    fn language(&self) -> Self::Language {
        self.input.language
    }

    fn version(&self) -> &Version {
        &self.version
    }

    fn sources(&self) -> impl Iterator<Item = (&Path, &Source)> {
        self.input.sources.iter().map(|(path, source)| (path.as_path(), source))
    }

    fn compiler_name(&self) -> Cow<'static, str> {
        // Detect Solar from version build metadata (e.g., "0.8.28+commit.xxx.solar.0.1.8")
        if self.version.build.as_str().contains("solar") { "Solar".into() } else { "Solc".into() }
    }

    fn settings_summary(&self) -> Option<String> {
        let settings = &self.input.settings;
        let optimizer_runs =
            settings.optimizer.runs.map_or_else(|| "default".to_owned(), |runs| runs.to_string());
        let evm_version = settings
            .evm_version
            .map_or_else(|| "default".to_owned(), |version| version.to_string());
        Some(format!(
            "optimizer={}, optimizer_runs={optimizer_runs}, via_ir={}, evm_version={evm_version}",
            settings.optimizer.enabled.unwrap_or(false),
            settings.via_ir.unwrap_or(false)
        ))
    }

    fn strip_prefix(&mut self, base: &Path) {
        self.input.strip_prefix(base);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CliSettings {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub allow_paths: BTreeSet<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub include_paths: BTreeSet<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SolcSettings {
    /// JSON settings expected by Solc
    #[serde(flatten)]
    pub settings: Settings,
    /// Additional CLI args configuration
    #[serde(flatten)]
    pub cli_settings: CliSettings,
}

impl Deref for SolcSettings {
    type Target = Settings;

    fn deref(&self) -> &Self::Target {
        &self.settings
    }
}

impl DerefMut for SolcSettings {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.settings
    }
}

/// Abstraction over min/max restrictions on some value.
#[derive(Debug, Clone, Copy, Eq, Default, PartialEq)]
pub struct Restriction<V> {
    pub min: Option<V>,
    pub max: Option<V>,
}

impl<V: Ord + Copy> Restriction<V> {
    /// Returns true if the given value satisfies the restrictions
    ///
    /// If given None, only returns true if no restrictions are set
    pub fn satisfies(&self, value: Option<V>) -> bool {
        self.min.is_none_or(|min| value.is_some_and(|v| v >= min))
            && self.max.is_none_or(|max| value.is_some_and(|v| v <= max))
    }

    /// Combines two restrictions into a new one
    pub fn merge(self, other: Self) -> Option<Self> {
        let Self { mut min, mut max } = self;
        let Self { min: other_min, max: other_max } = other;

        min = min.map_or(other_min, |this_min| {
            Some(other_min.map_or(this_min, |other_min| this_min.max(other_min)))
        });
        max = max.map_or(other_max, |this_max| {
            Some(other_max.map_or(this_max, |other_max| this_max.min(other_max)))
        });

        if let (Some(min), Some(max)) = (min, max)
            && min > max
        {
            return None;
        }

        Some(Self { min, max })
    }

    pub fn apply(&self, value: Option<V>) -> Option<V> {
        match (value, self.min, self.max) {
            (None, Some(min), _) => Some(min),
            (None, None, Some(max)) => Some(max),
            (Some(cur), Some(min), _) if cur < min => Some(min),
            (Some(cur), _, Some(max)) if cur > max => Some(max),
            _ => value,
        }
    }
}

/// Restrictions on settings for the solc compiler.
#[derive(Debug, Clone, Copy, Default)]
pub struct SolcRestrictions {
    pub evm_version: Restriction<EvmVersion>,
    pub via_ir: Option<bool>,
    pub optimizer_runs: Restriction<usize>,
    pub bytecode_hash: Option<BytecodeHash>,
}

impl CompilerSettingsRestrictions for SolcRestrictions {
    fn merge(self, other: Self) -> Option<Self> {
        if let (Some(via_ir), Some(other_via_ir)) = (self.via_ir, other.via_ir)
            && via_ir != other_via_ir
        {
            return None;
        }

        if let (Some(bytecode_hash), Some(other_bytecode_hash)) =
            (self.bytecode_hash, other.bytecode_hash)
            && bytecode_hash != other_bytecode_hash
        {
            return None;
        }

        Some(Self {
            evm_version: self.evm_version.merge(other.evm_version)?,
            via_ir: self.via_ir.or(other.via_ir),
            optimizer_runs: self.optimizer_runs.merge(other.optimizer_runs)?,
            bytecode_hash: self.bytecode_hash.or(other.bytecode_hash),
        })
    }
}

impl CompilerSettings for SolcSettings {
    type Restrictions = SolcRestrictions;

    fn update_output_selection(&mut self, mut f: impl FnMut(&mut OutputSelection)) {
        f(&mut self.settings.output_selection);
    }

    fn can_use_cached(&self, other: &Self) -> bool {
        let Self {
            settings:
                Settings {
                    stop_after,
                    remappings,
                    optimizer,
                    model_checker,
                    metadata,
                    output_selection,
                    evm_version,
                    via_ir,
                    via_ssa_cfg,
                    experimental,
                    debug,
                    libraries,
                },
            ..
        } = self;

        *stop_after == other.settings.stop_after
            && *remappings == other.settings.remappings
            && *optimizer == other.settings.optimizer
            && *model_checker == other.settings.model_checker
            && *metadata == other.settings.metadata
            && *evm_version == other.settings.evm_version
            && *via_ir == other.settings.via_ir
            && *via_ssa_cfg == other.settings.via_ssa_cfg
            && *experimental == other.settings.experimental
            && *debug == other.settings.debug
            && *libraries == other.settings.libraries
            && output_selection.is_subset_of(&other.settings.output_selection)
    }

    fn with_remappings(mut self, remappings: &[Remapping]) -> Self {
        self.settings.remappings = remappings.to_vec();

        self
    }

    fn with_allow_paths(mut self, allowed_paths: &BTreeSet<PathBuf>) -> Self {
        self.cli_settings.allow_paths.clone_from(allowed_paths);
        self
    }

    fn with_base_path(mut self, base_path: &Path) -> Self {
        self.cli_settings.base_path = Some(base_path.to_path_buf());
        self
    }

    fn with_include_paths(mut self, include_paths: &BTreeSet<PathBuf>) -> Self {
        self.cli_settings.include_paths.clone_from(include_paths);
        self
    }

    fn satisfies_restrictions(&self, restrictions: &Self::Restrictions) -> bool {
        let mut satisfies = true;

        let SolcRestrictions { evm_version, via_ir, optimizer_runs, bytecode_hash } = restrictions;

        satisfies &= evm_version.satisfies(self.evm_version);
        satisfies &= via_ir.is_none_or(|via_ir| via_ir == self.via_ir.unwrap_or_default());
        satisfies &= bytecode_hash.is_none_or(|bytecode_hash| {
            self.metadata.as_ref().and_then(|m| m.bytecode_hash) == Some(bytecode_hash)
        });
        satisfies &= optimizer_runs.satisfies(self.optimizer.runs);

        // Ensure that we either don't have min optimizer runs set or that the optimizer is enabled
        satisfies &= optimizer_runs
            .min
            .is_none_or(|min| min == 0 || self.optimizer.enabled.unwrap_or_default());

        satisfies
    }
}

impl SourceParser for SolParser {
    type ParsedSource = SolData;

    fn new(config: &crate::ProjectPathsConfig) -> Self {
        Self {
            compiler: solar::sema::Compiler::new(Self::session_with_opts(
                solar::sema::interface::config::CompileOpts {
                    include_paths: config.include_paths.iter().cloned().collect(),
                    base_path: Some(config.root.clone()),
                    import_remappings: config
                        .remappings
                        .iter()
                        .map(|r| solar::sema::interface::config::ImportRemapping {
                            context: r.context.clone().unwrap_or_default(),
                            prefix: r.name.clone(),
                            path: r.path.clone(),
                        })
                        .collect(),
                    ..Default::default()
                },
            )),
        }
    }

    fn read(&mut self, path: &Path) -> Result<Node<Self::ParsedSource>> {
        let mut sources = Sources::from_iter([(path.to_path_buf(), Source::read_(path)?)]);
        let nodes = self.parse_sources(&mut sources)?;
        debug_assert_eq!(nodes.len(), 1, "{nodes:#?}");
        Ok(nodes.into_iter().next().unwrap().1)
    }

    fn parse_sources(
        &mut self,
        sources: &mut Sources,
    ) -> Result<Vec<(PathBuf, Node<Self::ParsedSource>)>> {
        self.compiler.enter_mut(|compiler| {
            let mut pcx = compiler.parse();
            pcx.set_resolve_imports(false);
            let files = sources
                .par_iter()
                .map(|(path, source)| {
                    pcx.sess
                        .source_map()
                        .new_source_file(path.clone(), source.content.as_str())
                        .map_err(|e| SolcError::Io(SolcIoError::new(e, path)))
                })
                .collect::<Result<Vec<_>>>()?;
            pcx.add_files(files);
            pcx.parse();

            let parsed = sources.par_iter().map(|(path, source)| {
                let sf = compiler.sess().source_map().get_file(path).unwrap();
                let (_, s) = compiler.gcx().sources.get_file(&sf).unwrap();
                let node = Node::new(
                    path.clone(),
                    source.clone(),
                    SolData::parse_from(compiler.gcx().sess, s),
                );
                (path.clone(), node)
            });
            let parsed = parsed.collect::<Vec<_>>();

            Ok(parsed)
        })
    }

    fn finalize_imports(
        &mut self,
        nodes: &mut Vec<Node<Self::ParsedSource>>,
        include_paths: &BTreeSet<PathBuf>,
    ) -> Result<()> {
        let compiler = &mut self.compiler;
        compiler.sess_mut().opts.include_paths.extend(include_paths.iter().cloned());
        compiler.enter_mut(|compiler| {
            let mut pcx = compiler.parse();
            pcx.set_resolve_imports(true);
            pcx.force_resolve_all_imports();
        });

        // Set error on the first successful source, if any. This doesn't really have to be
        // exact, as long as at least one source has an error set it should be enough.
        if let Some(Err(diag)) = compiler.sess().emitted_errors()
            && let Some(idx) = nodes
                .iter()
                .position(|node| node.data.parse_result.is_ok())
                .or_else(|| nodes.first().map(|_| 0))
        {
            nodes[idx].data.parse_result = Err(diag.to_string());
        }

        for node in nodes.iter() {
            if let Err(e) = &node.data.parse_result {
                debug!("failed parsing:\n{e}");
            }
        }

        Ok(())
    }

    fn solar_compiler(&self) -> Option<&solar::sema::Compiler> {
        Some(&self.compiler)
    }
}

impl ParsedSource for SolData {
    type Language = SolcLanguage;

    fn parse(content: &str, file: &std::path::Path) -> Result<Self> {
        Ok(Self::parse(content, file))
    }

    fn version_req(&self) -> Option<&semver::VersionReq> {
        self.version_req.as_ref()
    }

    fn contract_names(&self) -> &[String] {
        &self.contract_names
    }

    fn language(&self) -> Self::Language {
        if self.is_yul { SolcLanguage::Yul } else { SolcLanguage::Solidity }
    }

    fn resolve_imports<C>(
        &self,
        _paths: &crate::ProjectPathsConfig<C>,
        _include_paths: &mut BTreeSet<PathBuf>,
    ) -> Result<Vec<PathBuf>> {
        Ok(self.imports.iter().map(|i| i.data().path().to_path_buf()).collect())
    }

    fn compilation_dependencies<'a>(
        &self,
        imported_nodes: impl Iterator<Item = (&'a Path, &'a Self)>,
    ) -> impl Iterator<Item = &'a Path>
    where
        Self: 'a,
    {
        imported_nodes.filter_map(|(path, node)| (!node.libraries.is_empty()).then_some(path))
    }
}

impl CompilationError for Error {
    fn is_warning(&self) -> bool {
        self.severity.is_warning()
    }
    fn is_error(&self) -> bool {
        self.severity.is_error()
    }

    fn source_location(&self) -> Option<SourceLocation> {
        self.source_location.clone()
    }

    fn severity(&self) -> Severity {
        self.severity
    }

    fn error_code(&self) -> Option<u64> {
        self.error_code
    }
}

#[cfg(test)]
mod tests {
    use foundry_compilers_artifacts::{
        CompilerOutput, Contract, EvmVersion, Optimizer, Settings, SolcLanguage, SourceFile,
        sources::{Source, Sources},
    };
    use semver::Version;
    use std::{
        collections::BTreeMap,
        path::{Path, PathBuf},
    };

    use crate::{
        AggregatedCompilerOutput,
        buildinfo::RawBuildInfo,
        compilers::{
            CompilerInput,
            solc::{SolcCompiler, SolcSettings, SolcVersionedInput},
        },
    };

    use super::{Solc, compiler_output};

    fn solc() -> Solc {
        Solc::new("solc").unwrap_or_else(|_| Solc::new_with_version("solc", Version::new(0, 8, 26)))
    }

    #[test]
    fn uses_standard_build_info_without_path_equivalent_source_units() {
        let input = SolcVersionedInput::build(
            Sources::from([(PathBuf::from("src/file.sol"), Source::new("contract File {}"))]),
            Default::default(),
            SolcLanguage::Solidity,
            Version::new(0, 8, 26),
        );
        let output = CompilerOutput {
            sources: BTreeMap::from([(
                "src/file.sol".to_string().into(),
                SourceFile { id: 1, ast: None },
            )]),
            contracts: BTreeMap::new(),
            errors: Vec::new(),
        };

        assert!(compiler_output(&solc(), &input, output).unwrap().build_info.is_none());
    }

    #[test]
    fn preserves_lossless_build_info_for_path_equivalent_source_units() {
        let input = SolcVersionedInput::build(
            Sources::from([(PathBuf::from("src/file.sol"), Source::new("contract File {}"))]),
            Default::default(),
            SolcLanguage::Solidity,
            Version::new(0, 8, 26),
        );
        let output = CompilerOutput {
            sources: BTreeMap::from([
                ("src//file.sol".to_string().into(), SourceFile { id: 1, ast: None }),
                ("src/file.sol".to_string().into(), SourceFile { id: 2, ast: None }),
                ("generated.sol".to_string().into(), SourceFile { id: 3, ast: None }),
            ]),
            contracts: BTreeMap::from([
                (
                    "src//file.sol".to_string().into(),
                    BTreeMap::from([(
                        "Alias".to_string(),
                        serde_json::from_str::<Contract>("{}").unwrap(),
                    )]),
                ),
                (
                    "src/file.sol".to_string().into(),
                    BTreeMap::from([(
                        "Exact".to_string(),
                        serde_json::from_str::<Contract>("{}").unwrap(),
                    )]),
                ),
            ]),
            errors: Vec::new(),
        };

        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("src/file.sol");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, "contract File {}").unwrap();
        let mut solc = solc();
        solc.base_path = Some(temp.path().to_path_buf());

        let output = compiler_output(&solc, &input, output).unwrap();
        assert_eq!(output.sources[Path::new("src/file.sol")].id, 2);
        assert_eq!(output.sources[Path::new("generated.sol")].id, 3);

        let build_info = RawBuildInfo::new(&input, &output, true).unwrap();
        let input_sources = &build_info.build_info["input"]["sources"];
        assert_eq!(input_sources["src//file.sol"], input_sources["src/file.sol"]);
        let output_sources = &build_info.build_info["output"]["sources"];
        assert_eq!(output_sources["src//file.sol"]["id"], 1);
        assert_eq!(output_sources["src/file.sol"]["id"], 2);
        assert!(build_info.build_info["output"]["contracts"].get("src//file.sol").is_some());
        assert!(build_info.build_info["output"]["contracts"].get("src/file.sol").is_some());
        assert_eq!(
            build_info.build_context.source_id_to_path,
            BTreeMap::from([
                (1, PathBuf::from("src//file.sol")),
                (2, PathBuf::from("src/file.sol")),
                (3, PathBuf::from("generated.sol")),
            ])
        );
    }

    #[test]
    fn can_parse_declaration_error() {
        let s = r#"{
  "errors": [
    {
      "component": "general",
      "errorCode": "7576",
      "formattedMessage": "DeclarationError: Undeclared identifier. Did you mean \"revert\"?\n  --> /Users/src/utils/UpgradeProxy.sol:35:17:\n   |\n35 |                 revert(\"Transparent ERC1967 proxies do not have upgradeable implementations\");\n   |                 ^^^^^^\n\n",
      "message": "Undeclared identifier. Did you mean \"revert\"?",
      "severity": "error",
      "sourceLocation": {
        "end": 1623,
        "file": "/Users/src/utils/UpgradeProxy.sol",
        "start": 1617
      },
      "type": "DeclarationError"
    }
  ],
  "sources": { }
}"#;

        let out: CompilerOutput = serde_json::from_str(s).unwrap();
        assert_eq!(out.errors.len(), 1);

        let out_converted = crate::compilers::CompilerOutput {
            errors: out.errors,
            contracts: Default::default(),
            sources: Default::default(),
            metadata: Default::default(),
            build_info: None,
        };

        let v = Version::new(0, 8, 12);
        let input = SolcVersionedInput::build(
            Default::default(),
            Default::default(),
            SolcLanguage::Solidity,
            v.clone(),
        );
        let build_info = RawBuildInfo::new(&input, &out_converted, true).unwrap();
        let mut aggregated = AggregatedCompilerOutput::<SolcCompiler>::default();
        aggregated.extend(v, build_info, "default", out_converted);
        assert!(!aggregated.is_unchanged());
    }

    #[test]
    fn test_compiler_name_detection() {
        use std::str::FromStr;

        // Regular solc version
        let solc_version = Version::from_str("0.8.28+commit.2d360a2").unwrap();
        let input = SolcVersionedInput::build(
            Default::default(),
            Default::default(),
            SolcLanguage::Solidity,
            solc_version,
        );
        assert_eq!(input.compiler_name().as_ref(), "Solc");

        // Solar version (contains "solar" in build metadata)
        let solar_version = Version::from_str("0.8.28+commit.2d360a2.solar.0.1.8").unwrap();
        let input = SolcVersionedInput::build(
            Default::default(),
            Default::default(),
            SolcLanguage::Solidity,
            solar_version,
        );
        assert_eq!(input.compiler_name().as_ref(), "Solar");
    }

    #[test]
    fn summarizes_sanitized_compiler_settings() {
        let settings = SolcSettings {
            settings: Settings {
                optimizer: Optimizer { enabled: Some(true), runs: Some(777), details: None },
                via_ir: Some(true),
                evm_version: Some(EvmVersion::Cancun),
                ..Default::default()
            },
            ..Default::default()
        };
        let input = SolcVersionedInput::build(
            Default::default(),
            settings,
            SolcLanguage::Solidity,
            Version::new(0, 7, 4),
        );

        assert_eq!(
            input.settings_summary().as_deref(),
            Some("optimizer=true, optimizer_runs=777, via_ir=false, evm_version=istanbul")
        );
    }
}
