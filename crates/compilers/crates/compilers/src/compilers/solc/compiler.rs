use crate::{compilers::compiler_path::resolve_and_approve, resolver::parse::SolData};
use foundry_compilers_artifacts::{CompilerOutput, SolcInput, sources::Source};
use foundry_compilers_core::{
    error::{Result, SolcError},
    utils::{SUPPORTS_BASE_PATH, SUPPORTS_INCLUDE_PATH},
};
use itertools::Itertools;
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    collections::BTreeSet,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    str::FromStr,
};

/// Extensions acceptable by solc compiler.
pub const SOLC_EXTENSIONS: &[&str] = &["sol", "yul"];

/// take the lock in tests, we use this to enforce that
/// a test does not run while a compiler version is being installed
///
/// This ensures that only one thread installs a missing `solc` exe.
/// Instead of taking this lock in `Solc::blocking_install`, the lock should be taken before
/// installation is detected.
#[cfg(feature = "svm-solc")]
#[cfg(any(test, feature = "test-utils"))]
#[macro_export]
macro_rules! take_solc_installer_lock {
    ($lock:ident) => {
        let lock_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".lock");
        let lock_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .unwrap();
        let mut lock = fd_lock::RwLock::new(lock_file);
        let $lock = lock.write().unwrap();
    };
}

/// A list of upstream Solc releases, used to check which version
/// we should download.
/// The boolean value marks whether there was an error accessing the release list
#[cfg(feature = "svm-solc")]
pub static RELEASES: std::sync::LazyLock<(svm::Releases, Vec<Version>, bool)> =
    std::sync::LazyLock::new(|| {
        match serde_json::from_str::<svm::Releases>(svm_builds::RELEASE_LIST_JSON) {
            Ok(releases) => {
                let sorted_versions = releases.clone().into_versions();
                (releases, sorted_versions, true)
            }
            Err(err) => {
                error!("failed to deserialize SVM static RELEASES JSON: {err}");
                Default::default()
            }
        }
    });

/// Abstraction over `solc` command line utility
///
/// Supports sync and async functions.
///
/// By default the solc path is configured as follows, with descending priority:
///   1. `SOLC_PATH` environment variable
///   2. [svm](https://github.com/roynalnaruto/svm-rs)'s  `global_version` (set via `svm use
///      <version>`), stored at `<svm_home>/.global_version`
///   3. `solc` otherwise
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Solc {
    /// Path to the `solc` executable
    pub solc: PathBuf,
    /// Compiler version.
    pub version: Version,
    /// Value for --base-path arg.
    pub base_path: Option<PathBuf>,
    /// Value for --allow-paths arg.
    pub allow_paths: BTreeSet<PathBuf>,
    /// Value for --include-paths arg.
    pub include_paths: BTreeSet<PathBuf>,
    /// Additional arbitrary arguments.
    pub extra_args: Vec<String>,
}

impl Solc {
    /// A new instance which points to `solc`. Invokes `solc --version` to determine the version.
    ///
    /// Returns error if `solc` is not found in the system or if the version cannot be retrieved.
    #[instrument(name = "Solc::new", skip_all)]
    pub fn new(path: impl Into<PathBuf>) -> Result<Self> {
        Self::new_with_args(path, Vec::<String>::new())
    }

    /// Creates a new instance after resolving `path` to an exact executable and passing that path
    /// to the process-wide approval handler. The resolved path is used for both the version probe
    /// and later compilations.
    pub fn new_with_approval(path: impl Into<PathBuf>) -> Result<Self> {
        Self::new_with_args_and_approval(path, Vec::<String>::new())
    }

    /// A new instance which points to `solc` with additional cli arguments. Invokes `solc
    /// --version` to determine the version.
    ///
    /// Returns error if `solc` is not found in the system or if the version cannot be retrieved.
    pub fn new_with_args(
        path: impl Into<PathBuf>,
        extra_args: impl IntoIterator<Item: Into<String>>,
    ) -> Result<Self> {
        let path = path.into();
        let extra_args = extra_args.into_iter().map(Into::into).collect::<Vec<_>>();
        let version = Self::version_with_args(path.clone(), &extra_args)?;
        Ok(Self::_new(path, version, extra_args))
    }

    /// Creates a new instance with additional CLI arguments after resolving and approving its
    /// exact executable path with the process-wide approval handler.
    pub fn new_with_args_and_approval(
        path: impl Into<PathBuf>,
        extra_args: impl IntoIterator<Item: Into<String>>,
    ) -> Result<Self> {
        let path = resolve_and_approve(path.into())?;
        Self::new_with_args(path, extra_args)
    }

    /// A new instance which points to `solc` with the given version
    pub fn new_with_version(path: impl Into<PathBuf>, version: Version) -> Self {
        Self::_new(path.into(), version, Default::default())
    }

    fn _new(path: PathBuf, version: Version, extra_args: Vec<String>) -> Self {
        let this = Self {
            solc: path,
            version,
            base_path: None,
            allow_paths: Default::default(),
            include_paths: Default::default(),
            extra_args,
        };
        this.debug_assert();
        this
    }

    fn debug_assert(&self) {
        if !cfg!(debug_assertions) {
            return;
        }
        if let Ok(v) = Self::version_with_args(&self.solc, &self.extra_args) {
            assert_eq!(v.major, self.version.major);
            assert_eq!(v.minor, self.version.minor);
            assert_eq!(v.patch, self.version.patch);
        }
    }

    /// Parses the given source looking for the `pragma` definition and
    /// returns the corresponding SemVer version requirement.
    pub fn source_version_req(source: &Source) -> Result<VersionReq> {
        Ok(SolData::parse_version_pragma(&source.content).ok_or(SolcError::PragmaNotFound)??)
    }

    /// Given a Solidity source, it detects the latest compiler version which can be used
    /// to build it, and returns it.
    ///
    /// If the required compiler version is not installed, it also proceeds to install it.
    #[cfg(feature = "svm-solc")]
    pub fn detect_version(source: &Source) -> Result<Version> {
        // detects the required solc version
        let sol_version = Self::source_version_req(source)?;
        Self::ensure_installed(&sol_version)
    }

    /// Given a Solidity version requirement, it detects the latest compiler version which can be
    /// used to build it, and returns it.
    ///
    /// If the required compiler version is not installed, it also proceeds to install it.
    #[cfg(feature = "svm-solc")]
    pub fn ensure_installed(sol_version: &VersionReq) -> Result<Version> {
        #[cfg(test)]
        take_solc_installer_lock!(_lock);

        // load the local / remote versions
        let versions = Self::installed_versions();

        let local_versions = Self::find_matching_installation(&versions, sol_version);
        let remote_versions = Self::find_matching_installation(&RELEASES.1, sol_version);

        // if there's a better upstream version than the one we have, install it
        Ok(match (local_versions, remote_versions) {
            (Some(local), None) => local,
            (Some(local), Some(remote)) => {
                if remote > local {
                    Self::blocking_install(&remote)?;
                    remote
                } else {
                    local
                }
            }
            (None, Some(version)) => {
                Self::blocking_install(&version)?;
                version
            }
            // do nothing otherwise
            _ => return Err(SolcError::VersionNotFound),
        })
    }

    /// Assuming the `versions` array is sorted, it returns the first element which satisfies
    /// the provided [`VersionReq`]
    pub fn find_matching_installation(
        versions: &[Version],
        required_version: &VersionReq,
    ) -> Option<Version> {
        // iterate in reverse to find the last match
        versions.iter().rev().find(|version| required_version.matches(version)).cloned()
    }

    /// Returns the path for a [svm](https://github.com/roynalnaruto/svm-rs) installed version.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use foundry_compilers::solc::Solc;
    /// use semver::Version;
    ///
    /// let solc = Solc::find_svm_installed_version(&Version::new(0, 8, 9))?;
    /// assert_eq!(solc, Some(Solc::new("~/.svm/0.8.9/solc-0.8.9")?));
    ///
    /// Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    #[instrument(skip_all)]
    #[cfg(feature = "svm-solc")]
    pub fn find_svm_installed_version(version: &Version) -> Result<Option<Self>> {
        let version = if version.pre.is_empty() {
            Version::new(version.major, version.minor, version.patch)
        } else {
            // Preserve version if it is a prerelease.
            version.clone()
        };
        let solc = svm::version_binary(&version.to_string());
        if !solc.is_file() {
            return Ok(None);
        }
        Ok(Some(Self::new_with_version(&solc, version)))
    }

    /// Returns the directory in which [svm](https://github.com/roynalnaruto/svm-rs) stores all versions
    ///
    /// This will be:
    /// - `~/.svm` on unix, if it exists
    /// - $XDG_DATA_HOME (~/.local/share/svm) if the svm folder does not exist.
    #[cfg(feature = "svm-solc")]
    pub fn svm_home() -> Option<PathBuf> {
        Some(svm::data_dir().to_path_buf())
    }

    /// Returns the `semver::Version` [svm](https://github.com/roynalnaruto/svm-rs)'s `.global_version` is currently set to.
    ///  `global_version` is configured with (`svm use <version>`)
    ///
    /// This will read the version string (eg: "0.8.9") that the  `~/.svm/.global_version` file
    /// contains
    #[cfg(feature = "svm-solc")]
    pub fn svm_global_version() -> Option<Version> {
        svm::get_global_version().ok().flatten()
    }

    /// Returns the list of all solc instances installed at `SVM_HOME`
    #[cfg(feature = "svm-solc")]
    pub fn installed_versions() -> Vec<Version> {
        svm::installed_versions().unwrap_or_default()
    }

    /// Returns the list of all versions that are available to download
    #[cfg(feature = "svm-solc")]
    pub fn released_versions() -> Vec<Version> {
        RELEASES.1.clone()
    }

    /// Installs the provided version of Solc in the machine under the svm dir and returns the
    /// [Solc] instance pointing to the installation.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use foundry_compilers::{solc::Solc, utils::ISTANBUL_SOLC};
    ///
    /// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
    /// let solc = Solc::install(&ISTANBUL_SOLC).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "svm-solc")]
    #[instrument(name = "Solc::install", skip_all)]
    pub async fn install(version: &Version) -> std::result::Result<Self, svm::SvmError> {
        trace!("installing solc version \"{}\"", version);
        crate::report::solc_installation_start(version);
        match svm::install(version).await {
            Ok(path) => {
                crate::report::solc_installation_success(version);
                Ok(Self::new_with_version(path, version.clone()))
            }
            Err(err) => {
                crate::report::solc_installation_error(version, &err.to_string());
                Err(err)
            }
        }
    }

    /// Blocking version of `Self::install`
    #[cfg(feature = "svm-solc")]
    #[instrument(name = "Solc::blocking_install", skip_all)]
    pub fn blocking_install(version: &Version) -> std::result::Result<Self, svm::SvmError> {
        use foundry_compilers_core::utils::RuntimeOrHandle;

        #[cfg(test)]
        crate::take_solc_installer_lock!(_lock);

        let version = if version.pre.is_empty() {
            Version::new(version.major, version.minor, version.patch)
        } else {
            // Preserve version if it is a prerelease.
            version.clone()
        };

        trace!("blocking installing solc version \"{}\"", version);
        crate::report::solc_installation_start(&version);
        // The async version `svm::install` is used instead of `svm::blocking_install`
        // because the underlying `reqwest::blocking::Client` does not behave well
        // inside of a Tokio runtime. See: https://github.com/seanmonstar/reqwest/issues/1017
        match RuntimeOrHandle::new().block_on(svm::install(&version)) {
            Ok(path) => {
                crate::report::solc_installation_success(&version);
                Ok(Self::new_with_version(path, version.clone()))
            }
            Err(err) => {
                crate::report::solc_installation_error(&version, &err.to_string());
                Err(err)
            }
        }
    }

    /// Verify that the checksum for this version of solc is correct. We check against the SHA256
    /// checksum from the build information published by [binaries.soliditylang.org](https://binaries.soliditylang.org/)
    #[cfg(feature = "svm-solc")]
    #[instrument(name = "Solc::verify_checksum", skip_all)]
    pub fn verify_checksum(&self) -> Result<()> {
        let version = self.version_short();
        let mut version_path = svm::version_path(version.to_string().as_str());
        version_path.push(format!("solc-{}", version.to_string().as_str()));
        trace!(target:"solc", "reading solc binary for checksum {:?}", version_path);
        let content =
            std::fs::read(&version_path).map_err(|err| SolcError::io(err, version_path.clone()))?;

        if !RELEASES.2 {
            // we skip checksum verification because the underlying request to fetch release info
            // failed so we have nothing to compare against
            return Ok(());
        }

        #[cfg(windows)]
        {
            // Prior to 0.7.2, binaries are released as exe files which are hard to verify: <https://github.com/foundry-rs/foundry/issues/5601>
            // <https://binaries.soliditylang.org/windows-amd64/list.json>
            const V0_7_2: Version = Version::new(0, 7, 2);
            if version < V0_7_2 {
                return Ok(());
            }
        }

        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(content);
        let checksum_calc = &hasher.finalize()[..];

        let checksum_found = &RELEASES
            .0
            .get_checksum(&version)
            .ok_or_else(|| SolcError::ChecksumNotFound { version: version.clone() })?;

        if checksum_calc == checksum_found {
            Ok(())
        } else {
            use alloy_primitives::hex;
            let expected = hex::encode(checksum_found);
            let detected = hex::encode(checksum_calc);
            warn!(target: "solc", "checksum mismatch for {:?}, expected {}, but found {} for file {:?}", version, expected, detected, version_path);
            Err(SolcError::ChecksumMismatch { version, expected, detected, file: version_path })
        }
    }

    /// Convenience function for compiling all sources under the given path
    pub fn compile_source(&self, path: &Path) -> Result<CompilerOutput> {
        let mut res: CompilerOutput = Default::default();
        for input in
            SolcInput::resolve_and_build(Source::read_sol_yul_from(path)?, Default::default())
        {
            let input = input.sanitized(&self.version);
            let output = self.compile(&input)?;
            res.merge(output)
        }

        Ok(res)
    }

    /// Same as [`Self::compile()`], but only returns those files which are included in the
    /// `CompilerInput`.
    ///
    /// In other words, this removes those files from the `CompilerOutput` that are __not__ included
    /// in the provided `CompilerInput`.
    ///
    /// # Examples
    pub fn compile_exact(&self, input: &SolcInput) -> Result<CompilerOutput> {
        let mut out = self.compile(input)?;
        out.retain_files(input.sources.keys().map(|p| p.as_path()));
        Ok(out)
    }

    /// Compiles with `--standard-json` and deserializes the output as [`CompilerOutput`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use foundry_compilers::{
    ///     artifacts::{SolcInput, Source},
    ///     compilers::{Compiler, CompilerInput},
    ///     solc::Solc,
    /// };
    ///
    /// let solc = Solc::new("solc")?;
    /// let input = SolcInput::resolve_and_build(
    ///     Source::read_sol_yul_from("./contracts".as_ref()).unwrap(),
    ///     Default::default(),
    /// );
    /// let output = solc.compile(&input)?;
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn compile<T: Serialize>(&self, input: &T) -> Result<CompilerOutput> {
        self.compile_as(input)
    }

    /// Compiles with `--standard-json` and deserializes the output as the given `D`.
    #[instrument(name = "Solc::compile", skip_all)]
    pub fn compile_as<T: Serialize, D: DeserializeOwned>(&self, input: &T) -> Result<D> {
        let output = self.compile_output(input)?;

        // Only run UTF-8 validation once.
        let output = std::str::from_utf8(&output).map_err(|_| SolcError::InvalidUtf8)?;

        Ok(serde_json::from_str(output)?)
    }

    /// Compiles with `--standard-json` and returns the raw `stdout` output.
    #[instrument(name = "Solc::compile_raw", skip_all)]
    pub fn compile_output<T: Serialize>(&self, input: &T) -> Result<Vec<u8>> {
        let mut cmd = self.configure_cmd();

        trace!(input=%serde_json::to_string(input).unwrap_or_else(|e| e.to_string()));
        debug!(?cmd, "compiling");

        let mut child = cmd.spawn().map_err(self.map_io_err())?;
        debug!("spawned");

        {
            let mut stdin = io::BufWriter::new(child.stdin.take().unwrap());
            serde_json::to_writer(&mut stdin, input)?;
            stdin.flush().map_err(self.map_io_err())?;
        }
        debug!("wrote JSON input to stdin");

        let output = child.wait_with_output().map_err(self.map_io_err())?;
        debug!(%output.status, output.stderr = ?String::from_utf8_lossy(&output.stderr), "finished");

        compile_output(output)
    }

    /// Returns the SemVer [`Version`], stripping the pre-release and build metadata.
    pub const fn version_short(&self) -> Version {
        Version::new(self.version.major, self.version.minor, self.version.patch)
    }

    /// Invokes `solc --version` and parses the output as a SemVer [`Version`].
    pub fn version(solc: impl Into<PathBuf>) -> Result<Version> {
        Self::version_with_args(solc, &[])
    }

    /// Invokes `solc --version` and parses the output as a SemVer [`Version`].
    pub fn version_with_args(solc: impl Into<PathBuf>, args: &[String]) -> Result<Version> {
        crate::cache_version(solc.into(), args, |solc| Self::version_impl(solc, args))
    }

    fn version_impl(solc: &Path, args: &[String]) -> Result<Version> {
        let mut cmd = Command::new(solc);
        cmd.args(args)
            .arg("--version")
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .stdout(Stdio::piped());
        debug!(?cmd, "getting Solc version");
        let output = cmd.output().map_err(|e| SolcError::io(e, solc))?;
        trace!(?output);
        let version = version_from_output(output)?;
        debug!(%version);
        Ok(version)
    }

    fn map_io_err(&self) -> impl FnOnce(std::io::Error) -> SolcError + '_ {
        move |err| SolcError::io(err, &self.solc)
    }

    /// Configures [Command] object depending on settings and solc version used.
    /// Some features are only supported by newer versions of solc, so we have to disable them for
    /// older ones.
    pub fn configure_cmd(&self) -> Command {
        let mut cmd = Command::new(&self.solc);
        cmd.stdin(Stdio::piped()).stderr(Stdio::piped()).stdout(Stdio::piped());
        cmd.args(&self.extra_args);

        if !self.allow_paths.is_empty() {
            cmd.arg("--allow-paths");
            cmd.arg(self.allow_paths.iter().map(|p| p.display()).join(","));
        }
        if let Some(base_path) = &self.base_path {
            if SUPPORTS_BASE_PATH.matches(&self.version) {
                if SUPPORTS_INCLUDE_PATH.matches(&self.version) {
                    // `--base-path` and `--include-path` conflict if set to the same path, so
                    // as a precaution, we ensure here that the `--base-path` is not also used
                    // for `--include-path`
                    for path in
                        self.include_paths.iter().filter(|p| p.as_path() != base_path.as_path())
                    {
                        cmd.arg("--include-path").arg(path);
                    }
                }

                cmd.arg("--base-path").arg(base_path);
            }

            cmd.current_dir(base_path);
        }

        cmd.arg("--standard-json");

        cmd
    }

    /// Reads a source unit using the same base and include paths passed to Solc's filesystem
    /// loader.
    pub(super) fn read_source_unit(&self, source_unit: &str) -> Result<String> {
        if self.extra_args.iter().any(|arg| {
            arg == "--base-path"
                || arg.starts_with("--base-path=")
                || arg == "--include-path"
                || arg.starts_with("--include-path=")
        }) {
            return Err(SolcError::msg(
                "cannot recover source-unit content when extra Solc arguments override base or include paths",
            ));
        }

        let source_unit = source_unit.strip_prefix("file://").unwrap_or(source_unit);
        let cwd = std::env::current_dir().map_err(|err| SolcError::io(err, "."))?;
        let child_cwd =
            self.base_path.as_deref().map(|path| absolute_path(&cwd, path)).unwrap_or(cwd);
        let mut roots = Vec::new();
        if let Some(base_path) = &self.base_path
            && SUPPORTS_BASE_PATH.matches(&self.version)
        {
            roots.push(absolute_path(&child_cwd, base_path));
        }
        if let Some(base_path) = &self.base_path
            && SUPPORTS_BASE_PATH.matches(&self.version)
            && SUPPORTS_INCLUDE_PATH.matches(&self.version)
        {
            roots.extend(
                self.include_paths
                    .iter()
                    .filter(|path| path.as_path() != base_path.as_path())
                    .map(|path| absolute_path(&child_cwd, path)),
            );
        }
        let mut candidates = if roots.is_empty() {
            vec![absolute_path(&child_cwd, Path::new(source_unit))]
        } else {
            roots.into_iter().map(|root| append_source_unit(&root, source_unit)).collect()
        };
        candidates.retain(|path| path.is_file());

        let path = match candidates.as_slice() {
            [] => {
                return Err(SolcError::msg(format!(
                    "source unit `{source_unit}` was emitted by Solc but could not be found in the base or include paths"
                )));
            }
            [path] => path,
            _ => {
                return Err(SolcError::msg(format!(
                    "source unit `{source_unit}` is ambiguous; found in {}",
                    candidates.iter().map(|path| path.display()).format(", ")
                )));
            }
        };
        std::fs::read_to_string(path).map_err(|err| SolcError::io(err, path))
    }

    /// Either finds an installed Solc version or installs it if it's not found.
    #[cfg(feature = "svm-solc")]
    pub fn find_or_install(version: &Version) -> Result<Self> {
        let solc = if let Some(solc) = Self::find_svm_installed_version(version)? {
            solc
        } else {
            Self::blocking_install(version)?
        };

        Ok(solc)
    }
}

fn absolute_path(cwd: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() { path.to_path_buf() } else { cwd.join(path) }
}

fn append_source_unit(root: &Path, source_unit: &str) -> PathBuf {
    #[cfg(windows)]
    let source_unit = source_unit.trim_start_matches(['/', '\\']);
    #[cfg(not(windows))]
    let source_unit = source_unit.trim_start_matches('/');

    let mut path = root.as_os_str().to_os_string();
    path.push(std::path::MAIN_SEPARATOR_STR);
    path.push(source_unit);
    path.into()
}

#[cfg(feature = "async")]
impl Solc {
    /// Convenience function for compiling all sources under the given path
    pub async fn async_compile_source(&self, path: &Path) -> Result<CompilerOutput> {
        self.async_compile(&SolcInput::resolve_and_build(
            Source::async_read_all_from(path, SOLC_EXTENSIONS).await?,
            Default::default(),
        ))
        .await
    }

    /// Run `solc --stand-json` and return the `solc`'s output as
    /// `CompilerOutput`
    pub async fn async_compile<T: Serialize>(&self, input: &T) -> Result<CompilerOutput> {
        self.async_compile_as(input).await
    }

    /// Run `solc --stand-json` and return the `solc`'s output as the given json
    /// output
    pub async fn async_compile_as<T: Serialize, D: DeserializeOwned>(
        &self,
        input: &T,
    ) -> Result<D> {
        let output = self.async_compile_output(input).await?;
        Ok(serde_json::from_slice(&output)?)
    }

    pub async fn async_compile_output<T: Serialize>(&self, input: &T) -> Result<Vec<u8>> {
        use tokio::{io::AsyncWriteExt, process::Command};

        let mut cmd: Command = self.configure_cmd().into();
        let mut child = cmd.spawn().map_err(self.map_io_err())?;
        let stdin = child.stdin.as_mut().unwrap();

        let content = serde_json::to_vec(input)?;

        stdin.write_all(&content).await.map_err(self.map_io_err())?;
        stdin.flush().await.map_err(self.map_io_err())?;

        compile_output(child.wait_with_output().await.map_err(self.map_io_err())?)
    }

    pub async fn async_version(solc: &Path) -> Result<Version> {
        let mut cmd = tokio::process::Command::new(solc);
        cmd.arg("--version").stdin(Stdio::piped()).stderr(Stdio::piped()).stdout(Stdio::piped());
        debug!(?cmd, "getting version");
        let output = cmd.output().await.map_err(|e| SolcError::io(e, solc))?;
        let version = version_from_output(output)?;
        debug!(%version);
        Ok(version)
    }

    /// Compiles all `CompilerInput`s with their associated `Solc`.
    ///
    /// This will buffer up to `n` `solc` processes and then return the `CompilerOutput`s in the
    /// order in which they complete. No more than `n` futures will be buffered at any point in
    /// time, and less than `n` may also be buffered depending on the state of each future.
    pub async fn compile_many<I>(jobs: I, n: usize) -> crate::many::CompiledMany
    where
        I: IntoIterator<Item = (Self, SolcInput)>,
    {
        use futures_util::stream::StreamExt;

        let outputs = futures_util::stream::iter(
            jobs.into_iter()
                .map(|(solc, input)| async { (solc.async_compile(&input).await, solc, input) }),
        )
        .buffer_unordered(n)
        .collect::<Vec<_>>()
        .await;

        crate::many::CompiledMany::new(outputs)
    }
}

fn compile_output(output: Output) -> Result<Vec<u8>> {
    if output.status.success() { Ok(output.stdout) } else { Err(SolcError::solc_output(&output)) }
}

fn version_from_output(output: Output) -> Result<Version> {
    if output.status.success() {
        parse_version(&String::from_utf8_lossy(&output.stdout))
    } else {
        Err(SolcError::solc_output(&output))
    }
}

fn parse_version(stdout: &str) -> Result<Version> {
    let version = stdout
        .lines()
        .rev()
        .find_map(|line| {
            let line = line.trim();
            line.strip_prefix("Version: ")
        })
        .ok_or_else(|| SolcError::msg("Version not found in Solc output"))?;
    // NOTE: semver doesn't like `+` in g++ in build metadata which is invalid semver
    Ok(Version::from_str(&version.replace(".g++", ".gcc"))?)
}

impl AsRef<Path> for Solc {
    fn as_ref(&self) -> &Path {
        &self.solc
    }
}

#[cfg(test)]
#[cfg(feature = "svm-solc")]
mod tests {
    use super::*;
    use crate::Artifact;

    #[test]
    fn test_version_parse() {
        let req = SolData::parse_version_req(">=0.6.2 <0.8.21").unwrap();
        let semver_req: VersionReq = ">=0.6.2,<0.8.21".parse().unwrap();
        assert_eq!(req, semver_req);
    }

    #[test]
    fn parses_solc_version_output() {
        let version = parse_version(
            "solc, the solidity compiler commandline interface\n\
             Version: 0.8.35+commit.47b9dedd.Linux.g++\n",
        )
        .unwrap();

        assert_eq!(version, Version::parse("0.8.35+commit.47b9dedd.Linux.gcc").unwrap());
    }

    #[test]
    fn parses_solar_version_output() {
        let version = parse_version(
            "solar the Solidity compiler\n\
             Version: 0.8.36+commit.3140f3e.solar.0.2.0\n",
        )
        .unwrap();

        assert_eq!(version, Version::parse("0.8.36+commit.3140f3e.solar.0.2.0").unwrap());
    }

    fn solc() -> Solc {
        if let Some(solc) = Solc::find_svm_installed_version(&Version::new(0, 8, 18)).unwrap() {
            solc
        } else {
            Solc::blocking_install(&Version::new(0, 8, 18)).unwrap()
        }
    }

    #[test]
    fn solc_version_works() {
        Solc::version(solc().solc).unwrap();
    }

    #[test]
    fn can_parse_version_metadata() {
        let _version = Version::from_str("0.6.6+commit.6c089d02.Linux.gcc").unwrap();
    }

    #[cfg(feature = "async")]
    #[tokio::test(flavor = "multi_thread")]
    async fn async_solc_version_works() {
        Solc::async_version(&solc().solc).await.unwrap();
    }

    #[test]
    fn solc_compile_works() {
        let input = include_str!("../../../../../test-data/in/compiler-in-1.json");
        let input: SolcInput = serde_json::from_str(input).unwrap();
        let out = solc().compile(&input).unwrap();
        let other = solc().compile(&serde_json::json!(input)).unwrap();
        assert_eq!(out, other);
    }

    #[test]
    fn solc_metadata_works() {
        let input = include_str!("../../../../../test-data/in/compiler-in-1.json");
        let mut input: SolcInput = serde_json::from_str(input).unwrap();
        input.settings.push_output_selection("metadata");
        let out = solc().compile(&input).unwrap();
        for (_, c) in out.split().1.contracts_iter() {
            assert!(c.metadata.is_some());
        }
    }

    #[test]
    fn reads_source_units_like_solc() {
        let cwd = std::env::current_dir().unwrap();
        let temp = tempfile::tempdir_in(&cwd).unwrap();
        let relative_base = PathBuf::from(temp.path().file_name().unwrap());
        let effective_base = temp.path().join(&relative_base);
        let effective_include = temp.path().join("include");
        std::fs::create_dir_all(effective_base.join("src")).unwrap();
        std::fs::create_dir_all(effective_include.join("lib")).unwrap();
        std::fs::write(effective_base.join("src/A.sol"), "base").unwrap();
        std::fs::write(effective_include.join("lib/Include.sol"), "include").unwrap();
        let absolute = temp.path().join("Absolute.sol");
        std::fs::write(&absolute, "absolute").unwrap();

        let mut configured_solc = solc();
        configured_solc.base_path = Some(relative_base);
        configured_solc.include_paths.insert(PathBuf::from("include"));

        assert_eq!(configured_solc.read_source_unit("src/A.sol").unwrap(), "base");
        assert_eq!(configured_solc.read_source_unit("lib/Include.sol").unwrap(), "include");
        assert_eq!(configured_solc.read_source_unit("/src//A.sol").unwrap(), "base");
        assert_eq!(configured_solc.read_source_unit("file://src//A.sol").unwrap(), "base");

        let source = absolute.to_str().unwrap();
        assert_eq!(solc().read_source_unit(source).unwrap(), "absolute");
        assert_eq!(solc().read_source_unit(&format!("file://{source}")).unwrap(), "absolute");
        configured_solc.version = Version::new(0, 6, 8);
        assert_eq!(configured_solc.read_source_unit(source).unwrap(), "absolute");

        #[cfg(not(windows))]
        {
            let backslash_dir = append_source_unit(&effective_base, "\\src");
            std::fs::create_dir_all(&backslash_dir).unwrap();
            std::fs::write(backslash_dir.join("A.sol"), "backslash").unwrap();
            configured_solc.version = Version::new(0, 8, 26);
            assert_eq!(configured_solc.read_source_unit("\\src/A.sol").unwrap(), "backslash");
        }
    }

    #[test]
    fn rejects_unreliable_source_unit_resolution() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("base");
        let include = temp.path().join("include");
        std::fs::create_dir_all(base.join("src")).unwrap();
        std::fs::create_dir_all(include.join("src")).unwrap();
        std::fs::write(base.join("src/A.sol"), "base").unwrap();
        std::fs::write(include.join("src/A.sol"), "include").unwrap();

        let mut solc = solc();
        solc.base_path = Some(base);
        solc.include_paths.insert(include);

        let err = solc.read_source_unit("src/A.sol").unwrap_err();
        assert!(err.to_string().contains("source unit `src/A.sol` is ambiguous"));

        solc.extra_args.push("--base-path=other".to_string());
        let err = solc.read_source_unit("src/A.sol").unwrap_err();
        assert!(err.to_string().contains("extra Solc arguments override base or include paths"));
    }

    #[test]
    fn can_compile_with_remapped_links() {
        let input: SolcInput = serde_json::from_str(include_str!(
            "../../../../../test-data/library-remapping-in.json"
        ))
        .unwrap();
        let out = solc().compile(&input).unwrap();
        let (_, mut contracts) = out.split();
        let contract = contracts.remove("LinkTest").unwrap();
        let bytecode = &contract.get_bytecode().unwrap().object;
        assert!(!bytecode.is_unlinked());
    }

    #[test]
    fn can_compile_with_remapped_links_temp_dir() {
        let input: SolcInput = serde_json::from_str(include_str!(
            "../../../../../test-data/library-remapping-in-2.json"
        ))
        .unwrap();
        let out = solc().compile(&input).unwrap();
        let (_, mut contracts) = out.split();
        let contract = contracts.remove("LinkTest").unwrap();
        let bytecode = &contract.get_bytecode().unwrap().object;
        assert!(!bytecode.is_unlinked());
    }

    #[cfg(feature = "async")]
    #[tokio::test(flavor = "multi_thread")]
    async fn async_solc_compile_works() {
        let input = include_str!("../../../../../test-data/in/compiler-in-1.json");
        let input: SolcInput = serde_json::from_str(input).unwrap();
        let out = solc().async_compile(&input).await.unwrap();
        let other = solc().async_compile(&serde_json::json!(input)).await.unwrap();
        assert_eq!(out, other);
    }

    #[cfg(feature = "async")]
    #[tokio::test(flavor = "multi_thread")]
    async fn async_solc_compile_works2() {
        let input = include_str!("../../../../../test-data/in/compiler-in-2.json");
        let input: SolcInput = serde_json::from_str(input).unwrap();
        let out = solc().async_compile(&input).await.unwrap();
        let other = solc().async_compile(&serde_json::json!(input)).await.unwrap();
        assert_eq!(out, other);
        let sync_out = solc().compile(&input).unwrap();
        assert_eq!(out, sync_out);
    }

    #[test]
    fn test_version_req() {
        let versions = ["=0.1.2", "^0.5.6", ">=0.7.1", ">0.8.0"];

        for version in &versions {
            let version_req = SolData::parse_version_req(version).unwrap();
            assert_eq!(version_req, VersionReq::from_str(version).unwrap());
        }

        // Solidity defines version ranges with a space, whereas the semver package
        // requires them to be separated with a comma
        let version_range = ">=0.8.0 <0.9.0";
        let version_req = SolData::parse_version_req(version_range).unwrap();
        assert_eq!(version_req, VersionReq::from_str(">=0.8.0,<0.9.0").unwrap());
    }

    #[test]
    #[cfg(feature = "full")]
    fn test_find_installed_version_path() {
        // This test does not take the lock by default, so we need to manually add it here.
        take_solc_installer_lock!(_lock);
        let version = Version::new(0, 8, 6);
        if svm::installed_versions()
            .map(|versions| !versions.contains(&version))
            .unwrap_or_default()
        {
            Solc::blocking_install(&version).unwrap();
        }
        drop(_lock);
        let res = Solc::find_svm_installed_version(&version).unwrap().unwrap();
        let expected = svm::data_dir().join(version.to_string()).join(format!("solc-{version}"));
        assert_eq!(res.solc, expected);
    }

    #[test]
    #[cfg(feature = "svm-solc")]
    fn can_install_solc_in_tokio_rt() {
        let version = Version::from_str("0.8.6").unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(async { Solc::blocking_install(&version) });
        assert!(result.is_ok());
    }

    #[test]
    fn does_not_find_not_installed_version() {
        let ver = Version::new(1, 1, 1);
        let res = Solc::find_svm_installed_version(&ver).unwrap();
        assert!(res.is_none());
    }
}

#[cfg(all(test, unix))]
mod approval_tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    #[test]
    fn approved_symlink_target_is_used_for_compilation() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first");
        let second = dir.path().join("second");
        let alias = dir.path().join("solc");
        let first_invoked = dir.path().join("first.invoked");
        let second_invoked = dir.path().join("second.invoked");
        for path in [&first, &second] {
            std::fs::write(
                path,
                r#"#!/bin/sh
if [ "$1" = "--version" ]; then
    echo "solc, the solidity compiler commandline interface"
    echo "Version: 0.8.35+commit.69074fbd"
else
    touch "$0.invoked"
    echo '{}'
fi
"#,
            )
            .unwrap();
            let mut permissions = std::fs::metadata(path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(path, permissions).unwrap();
        }
        symlink(&first, &alias).unwrap();

        crate::set_compiler_approval_handler(|_| Ok(()));
        let solc = Solc::new_with_approval(&alias).unwrap();
        assert_eq!(solc.solc, foundry_compilers_core::utils::canonicalize(&first).unwrap());
        std::fs::remove_file(&alias).unwrap();
        symlink(&second, &alias).unwrap();

        solc.compile_output(&serde_json::json!({})).unwrap();
        assert!(first_invoked.exists());
        assert!(!second_invoked.exists());
    }
}
