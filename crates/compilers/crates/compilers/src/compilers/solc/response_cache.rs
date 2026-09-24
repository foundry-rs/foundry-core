//! Optional response caching for self-contained, native Solc invocations.

use super::Solc;
use foundry_compilers_artifacts::{CompilerOutput, SolcInput};
use foundry_compilers_core::error::Result;
use std::path::Path;

#[cfg(feature = "svm-solc")]
use super::compiler::RELEASES;
#[cfg(feature = "svm-solc")]
use alloy_primitives::{B256, keccak256, map::HashSet};
#[cfg(feature = "svm-solc")]
use sha2::{Digest, Sha256};
#[cfg(feature = "svm-solc")]
use std::{fs, io::Write};

impl Solc {
    pub(super) fn compile_cached_response(
        &self,
        input: &SolcInput,
        cache_file: &Path,
    ) -> Result<CompilerOutput> {
        #[cfg(feature = "svm-solc")]
        if let Some(key) = self.response_cache_key(input) {
            if let Some(output) = read_response(cache_file, key) {
                return Ok(output);
            }
            let response = self.compile_output(input)?;
            let output = serde_json::from_slice::<CompilerOutput>(&response)?;
            // A successful exit can still contain compiler errors. Exact string comparisons
            // also reject sources loaded through the filesystem callback, including aliases
            // that collapse to the same Path on the host filesystem.
            let input_names =
                input.sources.keys().filter_map(|path| path.to_str()).collect::<HashSet<_>>();
            if !output.errors.iter().any(|error| error.is_error())
                && !output.sources.is_empty()
                && output.sources.len() == input.sources.len()
                && output
                    .sources
                    .keys()
                    .chain(output.contracts.keys())
                    .all(|name| input_names.contains(name.as_str()))
            {
                let _ = write_response(cache_file, key, &response);
            }
            return Ok(output);
        }
        #[cfg(not(feature = "svm-solc"))]
        let _ = cache_file;
        self.compile(input)
    }

    #[cfg(feature = "svm-solc")]
    fn response_cache_key(&self, input: &SolcInput) -> Option<B256> {
        // Wrappers and SMT solvers can depend on untracked executables, files, environment,
        // and timeouts. Only native release binaries with ordinary arguments are eligible.
        if !self.extra_args.is_empty()
            || input.settings.model_checker.is_some()
            || input.sources.values().any(|source| source.content.contains("SMTChecker"))
            || !self.solc.is_absolute()
            || !RELEASES.2
        {
            return None;
        }
        let expected = RELEASES.0.get_checksum(&self.version_short())?;
        let binary = fs::read(&self.solc).ok()?;
        let digest = Sha256::digest(&binary);
        if digest.as_slice() != expected.as_slice() {
            return None;
        }
        let cwd = std::env::current_dir().ok()?;
        let identity = serde_json::to_vec(&(1u32, self, cwd, digest.as_slice(), input)).ok()?;
        Some(keccak256(identity))
    }
}

#[cfg(feature = "svm-solc")]
const MAGIC: &[u8] = b"foundry-solc-response-v1\0";

#[cfg(feature = "svm-solc")]
fn read_response(path: &Path, key: B256) -> Option<CompilerOutput> {
    let contents = fs::read(path).ok()?;
    let contents = contents.strip_prefix(MAGIC)?;
    let stored_key = contents.get(..32)?;
    let checksum = contents.get(32..64)?;
    let response = contents.get(64..)?;
    if stored_key != key.as_slice() || checksum != keccak256(response).as_slice() {
        return None;
    }
    serde_json::from_slice(response).ok()
}

#[cfg(feature = "svm-solc")]
fn write_response(path: &Path, key: B256, response: &[u8]) -> std::io::Result<()> {
    let Some(parent) = path.parent() else { return Ok(()) };
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(MAGIC)?;
    file.write_all(key.as_slice())?;
    file.write_all(keccak256(response).as_slice())?;
    file.write_all(response)?;
    file.persist(path).map_err(|err| err.error)?;
    Ok(())
}

#[cfg(all(test, feature = "svm-solc"))]
mod tests {
    use super::*;
    use foundry_compilers_artifacts::{EvmVersion, Source};
    use semver::Version;

    #[test]
    fn response_cache_reuses_and_invalidates_complete_output() {
        let solc = Solc::find_or_install(&Version::new(0, 8, 19)).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("response");
        let mut input = SolcInput::default();
        input.settings.evm_version = Some(EvmVersion::Paris);
        input.sources.insert(
            "A.sol".into(),
            Source::new("contract A { function f() public returns (uint) { return 1; } }"),
        );
        let expected = solc.compile(&input).unwrap();
        assert!(!expected.errors.is_empty());
        assert!(!expected.errors.iter().any(|error| error.is_error()), "{:?}", expected.errors);
        assert_eq!(solc.compile_cached_response(&input, &path).unwrap(), expected);
        let key = solc.response_cache_key(&input).expect("native release compiler");
        assert_eq!(read_response(&path, key).unwrap(), expected);
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(solc.compile_cached_response(&input, &path).unwrap(), expected);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);

        input.sources.insert("A.sol".into(), Source::new("contract B {}"));
        assert_ne!(solc.response_cache_key(&input).unwrap(), key);
        let changed = solc.compile(&input).unwrap();
        assert_eq!(solc.compile_cached_response(&input, &path).unwrap(), changed);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);

        fs::write(&path, b"interrupted or corrupted write").unwrap();
        assert_eq!(solc.compile_cached_response(&input, &path).unwrap(), changed);
        assert!(read_response(&path, solc.response_cache_key(&input).unwrap()).is_some());
    }

    #[test]
    fn response_cache_does_not_reuse_callback_sources_or_errors() {
        let mut solc = Solc::find_or_install(&Version::new(0, 8, 19)).unwrap();
        let directory = tempfile::tempdir().unwrap();
        solc.base_path = Some(directory.path().to_path_buf());
        let path = directory.path().join("response");
        let mut input = SolcInput::default();
        input.settings.evm_version = Some(EvmVersion::Paris);
        input.sources.insert("A.sol".into(), Source::new("import './B.sol'; contract A is B {}"));
        let error = solc.compile_cached_response(&input, &path).unwrap();
        assert!(error.errors.iter().any(|error| error.is_error()));
        assert!(!path.exists());

        fs::write(directory.path().join("B.sol"), "contract B {}").unwrap();
        let output = solc.compile_cached_response(&input, &path).unwrap();
        assert!(!output.errors.iter().any(|error| error.is_error()));
        assert!(!path.exists());
        fs::write(directory.path().join("B.sol"), "contract C {}").unwrap();
        let output = solc.compile_cached_response(&input, &path).unwrap();
        assert!(output.errors.iter().any(|error| error.is_error()));
        assert!(!path.exists());
    }

    #[test]
    fn response_cache_bypasses_untracked_execution_inputs() {
        let mut solc = Solc::find_or_install(&Version::new(0, 8, 19)).unwrap();
        let mut input = SolcInput::default();
        input.settings.evm_version = Some(EvmVersion::Paris);
        assert!(solc.response_cache_key(&input).is_some());
        solc.extra_args.push("--some-wrapper-option".into());
        assert!(solc.response_cache_key(&input).is_none());
        solc.extra_args.clear();
        input.settings.model_checker = Some(Default::default());
        assert!(solc.response_cache_key(&input).is_none());
        input.settings.model_checker = None;
        input
            .sources
            .insert("A.sol".into(), Source::new("pragma experimental SMTChecker; contract A {}"));
        assert!(solc.response_cache_key(&input).is_none());
        input.sources.clear();
        let directory = tempfile::tempdir().unwrap();
        solc.solc = directory.path().join("solc");
        fs::write(&solc.solc, b"a different executable with the same reported version").unwrap();
        assert!(solc.response_cache_key(&input).is_none());
    }
}
