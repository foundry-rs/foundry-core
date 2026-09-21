use super::{
    CompilationError, Compiler, CompilerInput, CompilerOutput, CompilerSettings, CompilerVersion,
    Language, ParsedSource,
    fe::{Fe, FeInput, FeLanguage, FeParsedSource, FeParser, FeSettings},
    restrictions::CompilerSettingsRestrictions,
    solc::{SOLC_EXTENSIONS, SolcCompiler, SolcSettings, SolcVersionedInput},
    vyper::{
        VYPER_EXTENSIONS, Vyper, VyperLanguage, input::VyperVersionedInput,
        parser::VyperParsedSource,
    },
};
use crate::{
    SourceParser,
    artifacts::vyper::{VyperCompilationError, VyperSettings},
    parser::VyperParser,
    resolver::parse::{SolData, SolParser},
    settings::VyperRestrictions,
    solc::SolcRestrictions,
};
use foundry_compilers_artifacts::{
    Contract, Error, Severity, SolcLanguage,
    error::SourceLocation,
    output_selection::OutputSelection,
    remappings::Remapping,
    sources::{Source, Sources},
};
use foundry_compilers_core::error::{Result, SolcError};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    collections::BTreeSet,
    fmt,
    path::{Path, PathBuf},
};

/// Compiler capable of compiling Solidity, Vyper and Fe sources.
#[derive(Clone, Debug)]
pub struct MultiCompiler {
    pub solc: Option<SolcCompiler>,
    pub vyper: Option<Vyper>,
    pub fe: Option<Fe>,
}

impl Default for MultiCompiler {
    fn default() -> Self {
        let vyper = Vyper::new("vyper").ok();

        #[cfg(feature = "svm-solc")]
        let solc = Some(SolcCompiler::AutoDetect);
        #[cfg(not(feature = "svm-solc"))]
        let solc = crate::solc::Solc::new("solc").map(SolcCompiler::Specific).ok();

        Self { solc, vyper, fe: Fe::new("fe").ok() }
    }
}

impl MultiCompiler {
    pub fn new(solc: Option<SolcCompiler>, vyper_path: Option<PathBuf>) -> Result<Self> {
        let vyper = vyper_path.map(Vyper::new).transpose()?;
        Ok(Self { solc, vyper, fe: Fe::new("fe").ok() })
    }
}

/// Languages supported by the [MultiCompiler].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MultiCompilerLanguage {
    Solc(SolcLanguage),
    Vyper(VyperLanguage),
    Fe(FeLanguage),
}

impl Default for MultiCompilerLanguage {
    fn default() -> Self {
        Self::Solc(SolcLanguage::Solidity)
    }
}

impl MultiCompilerLanguage {
    pub const fn is_vyper(&self) -> bool {
        matches!(self, Self::Vyper(_))
    }

    pub const fn is_solc(&self) -> bool {
        matches!(self, Self::Solc(_))
    }
}

impl From<SolcLanguage> for MultiCompilerLanguage {
    fn from(language: SolcLanguage) -> Self {
        Self::Solc(language)
    }
}

impl From<VyperLanguage> for MultiCompilerLanguage {
    fn from(language: VyperLanguage) -> Self {
        Self::Vyper(language)
    }
}

impl Language for MultiCompilerLanguage {
    const FILE_EXTENSIONS: &'static [&'static str] = &["sol", "vy", "vyi", "yul", "fe"];
}

impl fmt::Display for MultiCompilerLanguage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Solc(lang) => lang.fmt(f),
            Self::Vyper(lang) => lang.fmt(f),
            Self::Fe(lang) => lang.fmt(f),
        }
    }
}

/// Source parser for the [`MultiCompiler`]. Recognizes Solc, Vyper and Fe sources.
#[derive(Clone, Debug)]
pub struct MultiCompilerParser {
    solc: SolParser,
    vyper: VyperParser,
    fe: FeParser,
}

impl MultiCompilerParser {
    /// Returns the parser used to parse Solc sources.
    pub const fn solc(&self) -> &SolParser {
        &self.solc
    }

    /// Returns the parser used to parse Solc sources.
    pub const fn solc_mut(&mut self) -> &mut SolParser {
        &mut self.solc
    }

    /// Returns the parser used to parse Vyper sources.
    pub const fn vyper(&self) -> &VyperParser {
        &self.vyper
    }

    /// Returns the parser used to parse Vyper sources.
    pub const fn vyper_mut(&mut self) -> &mut VyperParser {
        &mut self.vyper
    }
}

/// Source parser for the [MultiCompiler]. Recognizes Solc, Vyper and Fe sources.
#[derive(Clone, Debug)]
pub enum MultiCompilerParsedSource {
    Solc(SolData),
    Vyper(VyperParsedSource),
    Fe(FeParsedSource),
}

impl From<SolData> for MultiCompilerParsedSource {
    fn from(data: SolData) -> Self {
        Self::Solc(data)
    }
}

impl From<VyperParsedSource> for MultiCompilerParsedSource {
    fn from(data: VyperParsedSource) -> Self {
        Self::Vyper(data)
    }
}

impl MultiCompilerParsedSource {
    const fn solc(&self) -> Option<&SolData> {
        match self {
            Self::Solc(parsed) => Some(parsed),
            _ => None,
        }
    }

    const fn vyper(&self) -> Option<&VyperParsedSource> {
        match self {
            Self::Vyper(parsed) => Some(parsed),
            _ => None,
        }
    }
}

/// Compilation error which may occur when compiling Solidity, Vyper or Fe sources.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum MultiCompilerError {
    Solc(Error),
    Vyper(VyperCompilationError),
    Fe(Error),
}

impl fmt::Display for MultiCompilerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Solc(error) => error.fmt(f),
            Self::Vyper(error) => error.fmt(f),
            Self::Fe(error) => error.fmt(f),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MultiCompilerRestrictions {
    pub solc: SolcRestrictions,
    pub vyper: VyperRestrictions,
}

impl CompilerSettingsRestrictions for MultiCompilerRestrictions {
    fn merge(self, other: Self) -> Option<Self> {
        Some(Self { solc: self.solc.merge(other.solc)?, vyper: self.vyper.merge(other.vyper)? })
    }
}

/// Settings for the [MultiCompiler]. Includes settings for Solc, Vyper and Fe compilers.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiCompilerSettings {
    pub solc: SolcSettings,
    pub vyper: VyperSettings,
    #[serde(default)]
    pub fe: FeSettings,
}

impl CompilerSettings for MultiCompilerSettings {
    type Restrictions = MultiCompilerRestrictions;

    fn can_use_cached(&self, other: &Self) -> bool {
        self.solc.can_use_cached(&other.solc)
            && self.vyper.can_use_cached(&other.vyper)
            && self.fe == other.fe
    }

    fn update_output_selection(&mut self, mut f: impl FnMut(&mut OutputSelection)) {
        self.solc.update_output_selection(&mut f);
        self.vyper.update_output_selection(f);
    }

    fn with_allow_paths(self, allowed_paths: &BTreeSet<PathBuf>) -> Self {
        Self {
            solc: self.solc.with_allow_paths(allowed_paths),
            vyper: self.vyper.with_allow_paths(allowed_paths),
            fe: self.fe,
        }
    }

    fn with_base_path(self, base_path: &Path) -> Self {
        Self {
            solc: self.solc.with_base_path(base_path),
            vyper: self.vyper.with_base_path(base_path),
            fe: FeSettings { base_path: base_path.to_path_buf(), ..self.fe },
        }
    }

    fn with_include_paths(self, include_paths: &BTreeSet<PathBuf>) -> Self {
        Self {
            solc: self.solc.with_include_paths(include_paths),
            vyper: self.vyper.with_include_paths(include_paths),
            fe: self.fe,
        }
    }

    fn with_remappings(self, remappings: &[Remapping]) -> Self {
        Self {
            solc: self.solc.with_remappings(remappings),
            vyper: self.vyper.with_remappings(remappings),
            fe: self.fe,
        }
    }

    fn satisfies_restrictions(&self, restrictions: &Self::Restrictions) -> bool {
        self.solc.satisfies_restrictions(&restrictions.solc)
            && self.vyper.satisfies_restrictions(&restrictions.vyper)
    }
}

impl From<MultiCompilerSettings> for SolcSettings {
    fn from(settings: MultiCompilerSettings) -> Self {
        settings.solc
    }
}

impl From<MultiCompilerSettings> for VyperSettings {
    fn from(settings: MultiCompilerSettings) -> Self {
        settings.vyper
    }
}

/// Input for the [MultiCompiler]. Solc, Vyper or Fe input.
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum MultiCompilerInput {
    Solc(Box<SolcVersionedInput>),
    Vyper(VyperVersionedInput),
    Fe(FeInput),
}

impl CompilerInput for MultiCompilerInput {
    type Language = MultiCompilerLanguage;
    type Settings = MultiCompilerSettings;

    fn build(
        sources: Sources,
        settings: Self::Settings,
        language: Self::Language,
        version: Version,
    ) -> Self {
        match language {
            MultiCompilerLanguage::Fe(_) => Self::Fe(FeInput::new(sources, settings.fe, version)),
            MultiCompilerLanguage::Solc(language) => Self::Solc(Box::new(
                SolcVersionedInput::build(sources, settings.solc, language, version),
            )),
            MultiCompilerLanguage::Vyper(language) => {
                Self::Vyper(VyperVersionedInput::build(sources, settings.vyper, language, version))
            }
        }
    }

    fn compiler_name(&self) -> Cow<'static, str> {
        match self {
            Self::Solc(input) => input.compiler_name(),
            Self::Vyper(input) => input.compiler_name(),
            Self::Fe(input) => input.compiler_name(),
        }
    }

    fn settings_summary(&self) -> Option<String> {
        match self {
            Self::Solc(input) => input.settings_summary(),
            Self::Vyper(input) => input.settings_summary(),
            Self::Fe(input) => input.settings_summary(),
        }
    }

    fn language(&self) -> Self::Language {
        match self {
            Self::Solc(input) => MultiCompilerLanguage::Solc(input.language()),
            Self::Vyper(input) => MultiCompilerLanguage::Vyper(input.language()),
            Self::Fe(_) => MultiCompilerLanguage::Fe(FeLanguage::Fe),
        }
    }

    fn strip_prefix(&mut self, base: &Path) {
        match self {
            Self::Solc(input) => input.strip_prefix(base),
            Self::Vyper(input) => input.strip_prefix(base),
            Self::Fe(input) => input.strip_prefix(base),
        }
    }

    fn version(&self) -> &Version {
        match self {
            Self::Solc(input) => input.version(),
            Self::Vyper(input) => input.version(),
            Self::Fe(input) => &input.version,
        }
    }

    fn sources(&self) -> impl Iterator<Item = (&Path, &Source)> {
        let ret: Box<dyn Iterator<Item = _>> = match self {
            Self::Solc(input) => Box::new(input.sources()),
            Self::Vyper(input) => Box::new(input.sources()),
            Self::Fe(input) => Box::new(input.sources()),
        };

        ret
    }
}

impl Compiler for MultiCompiler {
    type Input = MultiCompilerInput;
    type CompilationError = MultiCompilerError;
    type Parser = MultiCompilerParser;
    type Settings = MultiCompilerSettings;
    type Language = MultiCompilerLanguage;
    type CompilerContract = Contract;

    fn compile(
        &self,
        input: &Self::Input,
    ) -> Result<CompilerOutput<Self::CompilationError, Self::CompilerContract>> {
        match input {
            MultiCompilerInput::Fe(input) => self.fe.as_ref().ok_or_else(|| SolcError::msg("Fe compiler is not available; configure [profile.default.fe].path with Fe 26.3 or newer"))?.compile(input).map(|res| res.map_err(MultiCompilerError::Fe)),
            MultiCompilerInput::Solc(input) => {
                if let Some(solc) = &self.solc {
                    Compiler::compile(solc, input).map(|res| res.map_err(MultiCompilerError::Solc))
                } else {
                    Err(SolcError::msg("solc compiler is not available"))
                }
            }
            MultiCompilerInput::Vyper(input) => {
                if let Some(vyper) = &self.vyper {
                    Compiler::compile(vyper, input)
                        .map(|res| res.map_err(MultiCompilerError::Vyper))
                } else {
                    Err(SolcError::msg("vyper compiler is not available"))
                }
            }
        }
    }

    fn available_versions(&self, language: &Self::Language) -> Vec<CompilerVersion> {
        match language {
            MultiCompilerLanguage::Fe(_) => self
                .fe
                .as_ref()
                .map(|fe| vec![CompilerVersion::Installed(fe.version.clone())])
                .unwrap_or_default(),
            MultiCompilerLanguage::Solc(language) => {
                self.solc.as_ref().map(|s| s.available_versions(language)).unwrap_or_default()
            }
            MultiCompilerLanguage::Vyper(language) => {
                self.vyper.as_ref().map(|v| v.available_versions(language)).unwrap_or_default()
            }
        }
    }
}

impl SourceParser for MultiCompilerParser {
    type ParsedSource = MultiCompilerParsedSource;

    fn new(config: &crate::ProjectPathsConfig) -> Self {
        Self {
            solc: SolParser::new(config),
            vyper: VyperParser::new(config),
            fe: FeParser::new(config),
        }
    }

    fn read(&mut self, path: &Path) -> Result<crate::resolver::Node<Self::ParsedSource>> {
        Ok(match guess_lang(path)? {
            MultiCompilerLanguage::Fe(_) => {
                self.fe.read(path)?.map_data(MultiCompilerParsedSource::Fe)
            }
            MultiCompilerLanguage::Solc(_) => {
                self.solc.read(path)?.map_data(MultiCompilerParsedSource::Solc)
            }
            MultiCompilerLanguage::Vyper(_) => {
                self.vyper.read(path)?.map_data(MultiCompilerParsedSource::Vyper)
            }
        })
    }

    fn parse_sources(
        &mut self,
        sources: &mut Sources,
    ) -> Result<Vec<(PathBuf, crate::resolver::Node<Self::ParsedSource>)>> {
        let mut vyper = Sources::new();
        let mut fe = Sources::new();
        sources.retain(|path, source| {
            if let Ok(lang) = guess_lang(path) {
                match lang {
                    MultiCompilerLanguage::Solc(_) => {}
                    MultiCompilerLanguage::Fe(_) => {
                        fe.insert(path.clone(), source.clone());
                        return false;
                    }
                    MultiCompilerLanguage::Vyper(_) => {
                        vyper.insert(path.clone(), source.clone());
                        return false;
                    }
                }
            }
            true
        });

        let fe_nodes = self.fe.parse_sources(&mut fe)?;
        let solc_nodes = self.solc.parse_sources(sources)?;
        let vyper_nodes = self.vyper.parse_sources(&mut vyper)?;
        Ok(solc_nodes
            .into_iter()
            .map(|(k, v)| (k, v.map_data(MultiCompilerParsedSource::Solc)))
            .chain(
                fe_nodes.into_iter().map(|(k, v)| (k, v.map_data(MultiCompilerParsedSource::Fe))),
            )
            .chain(
                vyper_nodes
                    .into_iter()
                    .map(|(k, v)| (k, v.map_data(MultiCompilerParsedSource::Vyper))),
            )
            .collect())
    }

    fn finalize_imports(
        &mut self,
        all_nodes: &mut Vec<crate::resolver::Node<Self::ParsedSource>>,
        include_paths: &BTreeSet<PathBuf>,
    ) -> Result<()> {
        // Must maintain original order.
        let mut solc_nodes = Vec::new();
        let mut vyper_nodes = Vec::new();
        let mut fe_nodes = Vec::new();
        let mut order = Vec::new();
        for node in std::mem::take(all_nodes) {
            order.push(node.data.language());
            match node.data {
                MultiCompilerParsedSource::Fe(_) => fe_nodes.push(node),
                MultiCompilerParsedSource::Solc(_) => {
                    solc_nodes.push(node.map_data(|data| match data {
                        MultiCompilerParsedSource::Solc(data) => data,
                        _ => unreachable!(),
                    }));
                }
                MultiCompilerParsedSource::Vyper(_) => {
                    vyper_nodes.push(node.map_data(|data| match data {
                        MultiCompilerParsedSource::Vyper(data) => data,
                        _ => unreachable!(),
                    }));
                }
            }
        }

        self.solc.finalize_imports(&mut solc_nodes, include_paths)?;
        self.vyper.finalize_imports(&mut vyper_nodes, include_paths)?;

        // Assume that the order was not changed by the parsers.
        let mut solc_nodes = solc_nodes.into_iter();
        let mut vyper_nodes = vyper_nodes.into_iter();
        let mut fe_nodes = fe_nodes.into_iter();
        for lang in order {
            match lang {
                MultiCompilerLanguage::Fe(_) => all_nodes.push(fe_nodes.next().unwrap()),
                MultiCompilerLanguage::Solc(_) => {
                    all_nodes.push(solc_nodes.next().unwrap().map_data(Into::into));
                }
                MultiCompilerLanguage::Vyper(_) => {
                    all_nodes.push(vyper_nodes.next().unwrap().map_data(Into::into));
                }
            }
        }
        assert!(solc_nodes.next().is_none());
        assert!(vyper_nodes.next().is_none());

        Ok(())
    }

    fn solar_compiler(&self) -> Option<&solar::sema::Compiler> {
        self.solc.solar_compiler()
    }
}

impl ParsedSource for MultiCompilerParsedSource {
    type Language = MultiCompilerLanguage;

    fn parse(content: &str, file: &Path) -> Result<Self> {
        match guess_lang(file)? {
            MultiCompilerLanguage::Fe(_) => FeParsedSource::parse(content, file).map(Self::Fe),
            MultiCompilerLanguage::Solc(_) => {
                <SolData as ParsedSource>::parse(content, file).map(Self::Solc)
            }
            MultiCompilerLanguage::Vyper(_) => {
                VyperParsedSource::parse(content, file).map(Self::Vyper)
            }
        }
    }

    fn version_req(&self) -> Option<&semver::VersionReq> {
        match self {
            Self::Solc(parsed) => parsed.version_req(),
            Self::Vyper(parsed) => parsed.version_req(),
            Self::Fe(parsed) => parsed.version_req(),
        }
    }

    fn contract_names(&self) -> &[String] {
        match self {
            Self::Solc(parsed) => parsed.contract_names(),
            Self::Vyper(parsed) => parsed.contract_names(),
            Self::Fe(parsed) => parsed.contract_names(),
        }
    }

    fn language(&self) -> Self::Language {
        match self {
            Self::Solc(parsed) => MultiCompilerLanguage::Solc(parsed.language()),
            Self::Vyper(parsed) => MultiCompilerLanguage::Vyper(parsed.language()),
            Self::Fe(parsed) => MultiCompilerLanguage::Fe(parsed.language()),
        }
    }

    fn resolve_imports<C>(
        &self,
        paths: &crate::ProjectPathsConfig<C>,
        include_paths: &mut BTreeSet<PathBuf>,
    ) -> Result<Vec<PathBuf>> {
        match self {
            Self::Solc(parsed) => parsed.resolve_imports(paths, include_paths),
            Self::Vyper(parsed) => parsed.resolve_imports(paths, include_paths),
            Self::Fe(parsed) => parsed.resolve_imports(paths, include_paths),
        }
    }

    fn compilation_dependencies<'a>(
        &self,
        imported_nodes: impl Iterator<Item = (&'a Path, &'a Self)>,
    ) -> impl Iterator<Item = &'a Path>
    where
        Self: 'a,
    {
        match self {
            Self::Fe(_) => imported_nodes.map(|(path, _)| path).collect::<Vec<_>>(),
            Self::Solc(parsed) => parsed
                .compilation_dependencies(
                    imported_nodes.filter_map(|(path, node)| node.solc().map(|node| (path, node))),
                )
                .collect::<Vec<_>>(),
            Self::Vyper(parsed) => parsed
                .compilation_dependencies(
                    imported_nodes.filter_map(|(path, node)| node.vyper().map(|node| (path, node))),
                )
                .collect::<Vec<_>>(),
        }
        .into_iter()
    }
}

fn guess_lang(path: &Path) -> Result<MultiCompilerLanguage> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .ok_or_else(|| SolcError::msg("failed to resolve file extension"))?;
    if extension == "fe" || path.file_name().is_some_and(|n| n == "fe.toml") {
        Ok(MultiCompilerLanguage::Fe(FeLanguage::Fe))
    } else if SOLC_EXTENSIONS.contains(&extension) {
        Ok(MultiCompilerLanguage::Solc(match extension {
            "sol" => SolcLanguage::Solidity,
            "yul" => SolcLanguage::Yul,
            _ => unreachable!(),
        }))
    } else if VYPER_EXTENSIONS.contains(&extension) {
        Ok(MultiCompilerLanguage::Vyper(VyperLanguage::default()))
    } else {
        Err(SolcError::msg("unexpected file extension"))
    }
}

impl CompilationError for MultiCompilerError {
    fn is_warning(&self) -> bool {
        match self {
            Self::Solc(error) => error.is_warning(),
            Self::Vyper(error) => error.is_warning(),
            Self::Fe(error) => error.is_warning(),
        }
    }
    fn is_error(&self) -> bool {
        match self {
            Self::Solc(error) => error.is_error(),
            Self::Vyper(error) => error.is_error(),
            Self::Fe(error) => error.is_error(),
        }
    }

    fn source_location(&self) -> Option<SourceLocation> {
        match self {
            Self::Solc(error) => error.source_location(),
            Self::Vyper(error) => error.source_location(),
            Self::Fe(error) => error.source_location(),
        }
    }

    fn severity(&self) -> Severity {
        match self {
            Self::Solc(error) => error.severity(),
            Self::Vyper(error) => error.severity(),
            Self::Fe(error) => error.severity(),
        }
    }

    fn error_code(&self) -> Option<u64> {
        match self {
            Self::Solc(error) => error.error_code(),
            Self::Vyper(error) => error.error_code(),
            Self::Fe(error) => error.error_code(),
        }
    }
}
