//! Fe compiler integration using the public artifact-producing CLI.
//!
//! Fe compilation units are ingots, not individual module files. The resolver conservatively
//! connects every module and manifest in a local dependency closure so Forge's cache observes
//! changes to any input. Compilation uses a temporary snapshot of those inputs.

use super::{CompilerOutput, Language, ParsedSource, SourceParser};
use crate::{ProjectPathsConfig, resolver::Node};
use foundry_compilers_artifacts::{
    Contract, Error, EvmVersion, SourceFile,
    sources::{Source, Sources},
};
use foundry_compilers_core::error::{Result, SolcError};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn err(error: impl fmt::Display) -> SolcError {
    SolcError::msg(error.to_string())
}

/// Fe source language.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FeLanguage {
    #[default]
    Fe,
}
impl Language for FeLanguage {
    const FILE_EXTENSIONS: &'static [&'static str] = &["fe"];
}
impl fmt::Display for FeLanguage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Fe")
    }
}

/// Fe build settings. The executable identity participates in the build cache.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FeSettings {
    pub optimize: String,
    pub evm_version: EvmVersion,
    pub compiler_identity: String,
    pub base_path: PathBuf,
}
impl Default for FeSettings {
    fn default() -> Self {
        Self {
            optimize: "1".into(),
            evm_version: EvmVersion::Osaka,
            compiler_identity: String::new(),
            base_path: PathBuf::new(),
        }
    }
}

/// Serializable snapshot of sources and manifests passed to Fe.
#[derive(Clone, Debug, Serialize)]
pub struct FeInput {
    pub language: FeLanguage,
    pub sources: Sources,
    pub settings: FeSettings,
    pub version: Version,
}
impl FeInput {
    pub const fn new(sources: Sources, settings: FeSettings, version: Version) -> Self {
        Self { language: FeLanguage::Fe, sources, settings, version }
    }
    pub fn compiler_name(&self) -> Cow<'static, str> {
        "Fe".into()
    }
    pub fn settings_summary(&self) -> Option<String> {
        Some(format!("optimizer {}", self.settings.optimize))
    }
    pub fn sources(&self) -> impl Iterator<Item = (&Path, &Source)> {
        self.sources.iter().map(|(p, s)| (p.as_path(), s))
    }
    pub fn strip_prefix(&mut self, base: &Path) {
        self.settings.base_path = base.to_path_buf();
        self.sources = std::mem::take(&mut self.sources)
            .into_iter()
            .map(|(p, s)| (p.strip_prefix(base).unwrap_or(&p).to_path_buf(), s))
            .collect();
    }
}

/// Installed Fe executable. No implicit compiler downloads are performed.
#[derive(Clone, Debug)]
pub struct Fe {
    pub path: PathBuf,
    pub version: Version,
    pub identity: String,
}
impl Fe {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let output = Command::new(&path).arg("--version").output().map_err(err)?;
        if !output.status.success() {
            return Err(err("failed to query Fe compiler version"));
        }
        let identity = String::from_utf8(output.stdout).map_err(err)?.trim().to_string();
        let mut words = identity.split_whitespace();
        if words.next() != Some("fe") {
            return Err(err(format!("unexpected Fe version: {identity}")));
        }
        let version =
            Version::parse(words.next().ok_or_else(|| err("missing Fe version"))?).map_err(err)?;
        if version < Version::new(26, 3, 0) {
            return Err(err("Foundry requires Fe 26.3.0 or newer"));
        }
        Ok(Self { path, version, identity })
    }

    pub fn compile(&self, input: &FeInput) -> Result<CompilerOutput<Error, Contract>> {
        if input.version != self.version {
            return Err(err("Fe compiler version differs from compilation input"));
        }
        if !["0", "1", "2", "s"].contains(&input.settings.optimize.as_str()) {
            return Err(err("Fe optimizer must be 0, 1, 2, or s"));
        }
        if input.settings.evm_version != EvmVersion::Osaka {
            return Err(err("Fe currently targets Osaka; set evm_version = \"osaka\""));
        }
        let temp = tempfile::tempdir().map_err(err)?;
        let base = if input.settings.base_path.is_absolute() {
            input.settings.base_path.clone()
        } else {
            std::env::current_dir().map_err(err)?.join(&input.settings.base_path)
        };
        let sources = input
            .sources
            .iter()
            .map(|(path, source)| (base.join(path), source))
            .collect::<BTreeMap<_, _>>();
        // Mirroring absolute paths preserves relative dependency references, including ../.
        let mirror = |path: &Path| -> Result<PathBuf> {
            let mut result = temp.path().join("sources");
            for component in path.components() {
                match component {
                    std::path::Component::Normal(part) => result.push(part),
                    std::path::Component::RootDir => {}
                    _ => {
                        return Err(err(format!(
                            "unsupported non-normalized Fe path: {}",
                            path.display()
                        )));
                    }
                }
            }
            Ok(result)
        };
        for (path, source) in &sources {
            let dest = mirror(path)?;
            fs::create_dir_all(dest.parent().unwrap()).map_err(err)?;
            let mut content = source.content.to_string();
            if path.file_name().is_some_and(|n| n == "fe.toml") {
                let mut manifest: toml::Value = toml::from_str(&content).map_err(err)?;
                if let Some(deps) =
                    manifest.get_mut("dependencies").and_then(toml::Value::as_table_mut)
                {
                    for (_, dep) in deps.iter_mut() {
                        if let Some(path) = dep.get_mut("path") {
                            let original = Path::new(
                                path.as_str().ok_or_else(|| err("invalid Fe dependency path"))?,
                            );
                            if original.is_absolute() {
                                *path = toml::Value::String(
                                    mirror(original)?.to_string_lossy().into_owned(),
                                );
                            }
                        }
                    }
                }
                content = toml::to_string(&manifest).map_err(err)?;
            }
            fs::write(dest, content).map_err(err)?;
        }
        let mut units = BTreeSet::new();
        for path in sources.keys().filter(|p| p.extension().is_some_and(|e| e == "fe")) {
            let ingot = path.ancestors().skip(1).find(|p| sources.contains_key(&p.join("fe.toml")));
            units.insert((ingot.unwrap_or(path).to_path_buf(), ingot.is_some()));
        }
        let mut output = CompilerOutput {
            errors: vec![],
            contracts: BTreeMap::new(),
            sources: BTreeMap::new(),
            metadata: BTreeMap::new(),
            build_info: None,
        };
        for (id, path) in input.sources.keys().enumerate() {
            output.sources.insert(path.clone(), SourceFile { id: id as u32, ast: None });
        }
        for (index, (unit, is_ingot)) in units.into_iter().enumerate() {
            let out = temp.path().join(format!("out-{index}"));
            let mut command = Command::new(&self.path);
            command
                .arg("build")
                .arg(mirror(&unit)?)
                .args(["--emit", "bytecode,runtime-bytecode,abi,metadata", "--out-dir"])
                .arg(&out)
                .arg("-O")
                .arg(&input.settings.optimize);
            if !is_ingot {
                command.arg("--standalone");
            }
            let result = command.output().map_err(err)?;
            if !result.status.success() {
                // Library-only ingots still contribute sources to the dependency graph.
                if String::from_utf8_lossy(&result.stderr).trim()
                    == "Error: No contracts found to build"
                {
                    continue;
                }
                let message = format!(
                    "Fe compilation failed for {}:\n{}\n{}",
                    unit.display(),
                    String::from_utf8_lossy(&result.stdout),
                    String::from_utf8_lossy(&result.stderr)
                )
                .replace(&temp.path().join("sources").to_string_lossy().to_string(), "");
                output.errors.push(serde_json::from_value(serde_json::json!({"type":"FeCompilerError", "component":"fe", "severity":"error", "message":message, "formattedMessage":message})).map_err(err)?);
                continue;
            }
            let warnings = String::from_utf8_lossy(&result.stderr);
            if !warnings.trim().is_empty() {
                let message = warnings
                    .replace(&temp.path().join("sources").to_string_lossy().to_string(), "");
                output.errors.push(
                    serde_json::from_value(serde_json::json!({
                        "type": "FeCompilerWarning", "component": "fe", "severity": "warning",
                        "message": message, "formattedMessage": message
                    }))
                    .map_err(err)?,
                );
            }
            for entry in fs::read_dir(&out).map_err(err)? {
                let path = entry.map_err(err)?.path();
                let Some(stem) = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .and_then(|n| n.strip_suffix(".metadata.json"))
                else {
                    continue;
                };
                let metadata_text = fs::read_to_string(&path).map_err(err)?;
                let metadata: serde_json::Value =
                    serde_json::from_str(&metadata_text).map_err(err)?;
                let target = metadata["settings"]["compilationTarget"]
                    .as_object()
                    .ok_or_else(|| err("Fe metadata has no compilationTarget"))?;
                if target.len() != 1 {
                    return Err(err("Fe metadata must have exactly one compilation target"));
                }
                let (source, name) = target.iter().next().unwrap();
                let source_path = if is_ingot { unit.join(source) } else { unit.clone() };
                if !sources.contains_key(&source_path) {
                    return Err(err(format!(
                        "Fe emitted a contract for an untracked source: {}",
                        source_path.display()
                    )));
                }
                let key = source_path.strip_prefix(&base).unwrap_or(&source_path).to_path_buf();
                let bytecode = fs::read_to_string(out.join(format!("{stem}.bin"))).map_err(err)?;
                let runtime =
                    fs::read_to_string(out.join(format!("{stem}.runtime.bin"))).map_err(err)?;
                // Released Fe 26.3 omits `anonymous: false` for events. Normalize only
                // the ABI consumed by Alloy; preserve the original metadata byte-for-byte.
                let mut abi = metadata["output"]["abi"].clone();
                if let Some(entries) = abi.as_array_mut() {
                    for entry in entries {
                        if entry["type"] == "event" {
                            entry
                                .as_object_mut()
                                .unwrap()
                                .entry("anonymous")
                                .or_insert(false.into());
                        }
                    }
                }
                let contract = serde_json::from_value(serde_json::json!({
                    "abi":abi, "metadata":metadata_text,
                    "evm":{"bytecode":{"object":bytecode.trim()}, "deployedBytecode":{"object":runtime.trim()}}
                })).map_err(err)?;
                output.contracts.entry(key).or_default().insert(
                    name.as_str().ok_or_else(|| err("invalid Fe contract name"))?.to_string(),
                    contract,
                );
            }
        }
        Ok(output)
    }
}

/// Parser for Fe compilation units, conservatively tracking all local ingot inputs.
#[derive(Clone, Debug)]
pub struct FeParser {
    root: PathBuf,
}
impl SourceParser for FeParser {
    type ParsedSource = FeParsedSource;
    fn new(config: &ProjectPathsConfig) -> Self {
        Self { root: config.root.clone() }
    }
    fn read(&mut self, path: &Path) -> Result<Node<FeParsedSource>> {
        let mut node = Node::<FeParsedSource>::read(path)?;
        node.data.root = Some(self.root.clone());
        Ok(node)
    }
    fn parse_sources(
        &mut self,
        sources: &mut Sources,
    ) -> Result<Vec<(PathBuf, Node<FeParsedSource>)>> {
        sources
            .iter()
            .map(|(path, source)| {
                let mut data = FeParsedSource::parse(source.as_ref(), path)?;
                data.root = Some(self.root.clone());
                Ok((path.clone(), Node::new(path.clone(), source.clone(), data)))
            })
            .collect()
    }
}
#[derive(Clone, Debug)]
pub struct FeParsedSource {
    path: PathBuf,
    root: Option<PathBuf>,
}
impl ParsedSource for FeParsedSource {
    type Language = FeLanguage;
    fn parse(_content: &str, file: &Path) -> Result<Self> {
        Ok(Self { path: file.to_path_buf(), root: None })
    }
    fn version_req(&self) -> Option<&VersionReq> {
        None
    }
    fn contract_names(&self) -> &[String] {
        &[]
    }
    fn language(&self) -> FeLanguage {
        FeLanguage::Fe
    }
    fn resolve_imports<C>(
        &self,
        paths: &ProjectPathsConfig<C>,
        _include: &mut BTreeSet<PathBuf>,
    ) -> Result<Vec<PathBuf>> {
        let root = self.root.as_deref().unwrap_or(&paths.root);
        let mut inputs = BTreeSet::new();
        for ancestor in self.path.ancestors().skip(1) {
            if ancestor.join("fe.toml").is_file() {
                collect_ingot(ancestor, &mut BTreeSet::new(), &mut inputs)?;
                break;
            }
            if ancestor == root {
                break;
            }
        }
        inputs.remove(&self.path);
        Ok(inputs.into_iter().collect())
    }
    fn compilation_dependencies<'a>(
        &self,
        imported: impl Iterator<Item = (&'a Path, &'a Self)>,
    ) -> impl Iterator<Item = &'a Path>
    where
        Self: 'a,
    {
        imported.map(|(path, _)| path)
    }
}
fn collect_ingot(
    root: &Path,
    visited: &mut BTreeSet<PathBuf>,
    inputs: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    let root = fs::canonicalize(root).map_err(err)?;
    if !visited.insert(root.clone()) {
        return Ok(());
    }
    let manifest_path = root.join("fe.toml");
    let manifest: toml::Value =
        toml::from_str(&fs::read_to_string(&manifest_path).map_err(err)?).map_err(err)?;
    if manifest.get("workspace").is_some() {
        return Err(err(
            "Fe workspace manifests are not yet supported by Foundry; use an ingot with local path dependencies",
        ));
    }
    // Ancestor workspace configuration could affect an ingot and must not be silently omitted.
    for ancestor in root.ancestors().skip(1) {
        if ancestor.join("fe.toml").is_file() {
            let parent: toml::Value =
                toml::from_str(&fs::read_to_string(ancestor.join("fe.toml")).map_err(err)?)
                    .map_err(err)?;
            if let Some(members) = parent
                .get("workspace")
                .and_then(|w| w.get("members"))
                .and_then(toml::Value::as_array)
            {
                for member in members {
                    let member = member
                        .as_str()
                        .or_else(|| member.get("path").and_then(toml::Value::as_str))
                        .ok_or_else(|| err("invalid Fe workspace member"))?;
                    if member.contains(['*', '?', '['])
                        || fs::canonicalize(ancestor.join(member)).ok().as_ref() == Some(&root)
                    {
                        return Err(err("Fe workspace members are not yet supported by Foundry"));
                    }
                }
            }
            break;
        }
    }
    inputs.insert(manifest_path);
    for entry in walkdir::WalkDir::new(root.join("src")) {
        let entry = entry.map_err(err)?;
        if entry.file_type().is_symlink() {
            return Err(err("symlinked Fe source inputs are not supported"));
        }
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|e| e == "fe") {
            inputs.insert(entry.path().to_path_buf());
        }
    }
    if let Some(deps) = manifest.get("dependencies").and_then(toml::Value::as_table) {
        for (name, dep) in deps {
            let path = dep.get("path").and_then(toml::Value::as_str).ok_or_else(|| {
                err(format!("Foundry currently requires a local path for Fe dependency {name}"))
            })?;
            collect_ingot(&root.join(path), visited, inputs)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_tracks_transitive_sources_and_manifests() {
        let temp = tempfile::tempdir().unwrap();
        for (name, dependency) in
            [("app", Some("middle")), ("middle", Some("leaf")), ("leaf", None)]
        {
            let root = temp.path().join(name);
            fs::create_dir_all(root.join("src")).unwrap();
            let mut manifest = format!("[ingot]\nname = '{name}'\nversion = '0.1.0'\n");
            if let Some(dep) = dependency {
                manifest.push_str(&format!("[dependencies]\n{dep} = {{ path = '../{dep}' }}\n"));
            }
            fs::write(root.join("fe.toml"), manifest).unwrap();
            fs::write(root.join("src/lib.fe"), "").unwrap();
        }
        let mut inputs = BTreeSet::new();
        collect_ingot(&temp.path().join("app"), &mut BTreeSet::new(), &mut inputs).unwrap();
        assert_eq!(inputs.len(), 6);
        assert!(inputs.contains(&temp.path().join("leaf/fe.toml")));
        assert!(inputs.contains(&temp.path().join("leaf/src/lib.fe")));
        fs::write(temp.path().join("leaf/src/added.fe"), "").unwrap();
        let mut updated = BTreeSet::new();
        collect_ingot(&temp.path().join("app"), &mut BTreeSet::new(), &mut updated).unwrap();
        assert_eq!(updated.len(), 7);
    }

    #[test]
    fn rejects_untracked_remote_dependencies() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("src")).unwrap();
        fs::write(temp.path().join("fe.toml"), "[ingot]\nname = 'app'\nversion = '0.1.0'\n[dependencies]\nremote = { git = 'https://example.org/remote' }\n").unwrap();
        let error =
            collect_ingot(temp.path(), &mut BTreeSet::new(), &mut BTreeSet::new()).unwrap_err();
        assert!(error.to_string().contains("local path for Fe dependency remote"));
    }

    #[test]
    fn compiler_identity_invalidates_settings() {
        use crate::compilers::{CompilerSettings, multi::MultiCompilerSettings};
        let settings = MultiCompilerSettings::default();
        let mut changed = settings.clone();
        changed.fe.compiler_identity = "fe 26.3.0 (different-commit)".into();
        assert!(!settings.can_use_cached(&changed));
    }

    #[test]
    #[ignore = "requires FE_TEST_BIN pointing to Fe 26.3 or newer"]
    fn real_project_cache_observes_dependency_and_manifest_changes() {
        use crate::{
            ProjectBuilder,
            compilers::multi::{MultiCompiler, MultiCompilerSettings},
        };
        let compiler = Fe::new(std::env::var_os("FE_TEST_BIN").expect("set FE_TEST_BIN")).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("src/app/src")).unwrap();
        fs::create_dir_all(root.join("lib/helper/src")).unwrap();
        fs::write(root.join("src/app/fe.toml"), "[ingot]\nname = 'app'\nversion = '0.1.0'\n[dependencies]\nhelper = { path = '../../lib/helper' }\n").unwrap();
        fs::write(root.join("lib/helper/fe.toml"), "[ingot]\nname = 'helper'\nversion = '0.1.0'\n")
            .unwrap();
        fs::write(root.join("lib/helper/src/lib.fe"), "pub fn value() -> u256 { 42 }\n").unwrap();
        fs::write(root.join("src/app/src/lib.fe"), "use helper::value\npub msg Msg {\n    #[selector = sol(\"number()\")]\n    Number -> u256,\n}\npub contract Counter {\n    recv Msg {\n        Number -> u256 { value() }\n    }\n}\n").unwrap();
        let paths = ProjectPathsConfig::builder()
            .root(root)
            .sources(root.join("src"))
            .artifacts(root.join("out"))
            .cache(root.join("cache.json"))
            .build()
            .unwrap();
        let mut settings = MultiCompilerSettings::default();
        settings.fe.compiler_identity = compiler.identity.clone();
        let project = ProjectBuilder::<MultiCompiler>::default()
            .paths(paths)
            .settings(settings)
            .build(MultiCompiler { solc: None, vyper: None, fe: Some(compiler) })
            .unwrap();
        let first = project.compile().unwrap();
        first.assert_success();
        assert!(!first.is_unchanged());
        let cached = project.compile().unwrap();
        cached.assert_success();
        assert!(cached.is_unchanged());
        let artifact_path = root.join("out/lib.fe/Counter.json");
        let original: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&artifact_path).unwrap()).unwrap();
        fs::write(root.join("lib/helper/src/lib.fe"), "pub fn value() -> u256 { 43 }\n").unwrap();
        let changed = project.compile().unwrap();
        changed.assert_success();
        assert!(!changed.is_unchanged());
        let updated: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&artifact_path).unwrap()).unwrap();
        assert_ne!(original["deployedBytecode"], updated["deployedBytecode"]);
        fs::write(root.join("lib/helper/fe.toml"), "[ingot]\nname = 'helper'\nversion = '0.1.1'\n")
            .unwrap();
        let changed = project.compile().unwrap();
        changed.assert_success();
        assert!(!changed.is_unchanged());
        assert!(project.compile().unwrap().is_unchanged());
    }

    #[test]
    #[ignore = "requires FE_TEST_BIN pointing to Fe 26.3 or newer"]
    fn real_compiler_artifacts_and_diagnostics() {
        let compiler = Fe::new(std::env::var_os("FE_TEST_BIN").expect("set FE_TEST_BIN")).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let source = "pub msg CounterMsg {\n    #[selector = sol(\"number()\")]\n    Number -> u256,\n}\npub contract Counter {\n    recv CounterMsg {\n        Number -> u256 { 42 }\n    }\n}\n";
        let settings = FeSettings { base_path: temp.path().to_path_buf(), ..Default::default() };
        let sources = [(PathBuf::from("Counter.fe"), Source::new(source))].into_iter().collect();
        let input = FeInput::new(sources, settings, compiler.version.clone());
        let output = compiler.compile(&input).unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        let contract = &output.contracts[Path::new("Counter.fe")]["Counter"];
        assert!(contract.abi.as_ref().unwrap().functions.contains_key("number"));
        let metadata = contract.metadata.as_ref().unwrap().raw_json().unwrap();
        assert_eq!(metadata["language"], "Fe");
        assert_eq!(metadata["sources"]["Counter.fe"]["content"], source);
        assert_eq!(metadata["settings"]["optimizer"]["level"], "1");
        let mut broken = input.clone();
        broken.sources =
            [(PathBuf::from("Counter.fe"), Source::new("pub contract {"))].into_iter().collect();
        let output = compiler.compile(&broken).unwrap();
        assert!(!output.errors.is_empty());
        assert!(output.contracts.is_empty());
        let mut wrong_evm = input;
        wrong_evm.settings.evm_version = EvmVersion::Cancun;
        assert!(compiler.compile(&wrong_evm).unwrap_err().to_string().contains("Osaka"));
    }
}
