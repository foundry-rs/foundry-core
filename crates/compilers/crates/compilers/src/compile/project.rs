//! Manages compiling of a `Project`
//!
//! The compilation of a project is performed in several steps.
//!
//! First the project's dependency graph [`crate::Graph`] is constructed and all imported
//! dependencies are resolved. The graph holds all the relationships between the files and their
//! versions. From there the appropriate version set is derived
//! [`crate::Graph`] which need to be compiled with different
//! [`crate::compilers::solc::Solc`] versions.
//!
//! At this point we check if we need to compile a source file or whether we can reuse an _existing_
//! `Artifact`. We don't to compile if:
//!     - caching is enabled
//!     - the file is **not** dirty
//!     - the artifact for that file exists
//!
//! This concludes the preprocessing, and we now have either
//!    - only `Source` files that need to be compiled
//!    - only cached `Artifacts`, compilation can be skipped. This is considered an unchanged,
//!      cached project
//!    - Mix of both `Source` and `Artifacts`, only the `Source` files need to be compiled, the
//!      `Artifacts` can be reused.
//!
//! The final step is invoking `Solc` via the standard JSON format.
//!
//! ### Notes on [Import Path Resolution](https://docs.soliditylang.org/en/develop/path-resolution.html#path-resolution)
//!
//! In order to be able to support reproducible builds on all platforms, the Solidity compiler has
//! to abstract away the details of the filesystem where source files are stored. Paths used in
//! imports must work the same way everywhere while the command-line interface must be able to work
//! with platform-specific paths to provide good user experience. This section aims to explain in
//! detail how Solidity reconciles these requirements.
//!
//! The compiler maintains an internal database (virtual filesystem or VFS for short) where each
//! source unit is assigned a unique source unit name which is an opaque and unstructured
//! identifier. When you use the import statement, you specify an import path that references a
//! source unit name. If the compiler does not find any source unit name matching the import path in
//! the VFS, it invokes the callback, which is responsible for obtaining the source code to be
//! placed under that name.
//!
//! This becomes relevant when dealing with resolved imports
//!
//! #### Relative Imports
//!
//! ```solidity
//! import "./math/math.sol";
//! import "contracts/tokens/token.sol";
//! ```
//! In the above `./math/math.sol` and `contracts/tokens/token.sol` are import paths while the
//! source unit names they translate to are `contracts/math/math.sol` and
//! `contracts/tokens/token.sol` respectively.
//!
//! #### Direct Imports
//!
//! An import that does not start with `./` or `../` is a direct import.
//!
//! ```solidity
//! import "/project/lib/util.sol";         // source unit name: /project/lib/util.sol
//! import "lib/util.sol";                  // source unit name: lib/util.sol
//! import "@openzeppelin/address.sol";     // source unit name: @openzeppelin/address.sol
//! import "https://example.com/token.sol"; // source unit name: <https://example.com/token.sol>
//! ```
//!
//! After applying any import remappings the import path simply becomes the source unit name.
//!
//! ##### Import Remapping
//!
//! ```solidity
//! import "github.com/ethereum/dapp-bin/library/math.sol"; // source unit name: dapp-bin/library/math.sol
//! ```
//!
//! If compiled with `solc github.com/ethereum/dapp-bin/=dapp-bin/` the compiler will look for the
//! file in the VFS under `dapp-bin/library/math.sol`. If the file is not available there, the
//! source unit name will be passed to the Host Filesystem Loader, which will then look in
//! `/project/dapp-bin/library/iterable_mapping.sol`
//!
//!
//! ### Caching and Change detection
//!
//! If caching is enabled in the [Project] a cache file will be created upon a successful solc
//! build. The [cache file](crate::cache::CompilerCache) stores metadata for all the files that were
//! provided to solc.
//! For every file the cache file contains a dedicated [cache entry](crate::cache::CacheEntry),
//! which represents the state of the file. A solidity file can contain several contracts, for every
//! contract a separate [artifact](crate::Artifact) is emitted. Therefore the entry also tracks all
//! artifacts emitted by a file. A solidity file can also be compiled with several solc versions.
//!
//! For example in `A(<=0.8.10) imports C(>0.4.0)` and
//! `B(0.8.11) imports C(>0.4.0)`, both `A` and `B` import `C` but there's no solc version that's
//! compatible with `A` and `B`, in which case two sets are compiled: [`A`, `C`] and [`B`, `C`].
//! This is reflected in the cache entry which tracks the file's artifacts by version.
//!
//! The cache makes it possible to detect changes during recompilation, so that only the changed,
//! dirty, files need to be passed to solc. A file will be considered as dirty if:
//!   - the file is new, not included in the existing cache
//!   - the file was modified since the last compiler run, detected by comparing content hashes
//!   - any of the imported files is dirty
//!   - the file's artifacts don't exist, were deleted.
//!
//! Recompiling a project with cache enabled detects all files that meet these criteria and provides
//! solc with only these dirty files instead of the entire source set.

use crate::{
    ArtifactOutput, CompilerSettings, ConfigurableArtifacts, Graph, Project, ProjectCompileOutput,
    ProjectPathsConfig, Sources,
    artifact_output::Artifacts,
    buildinfo::RawBuildInfo,
    cache::ArtifactsCache,
    compilers::{Compiler, CompilerInput, CompilerOutput, Language},
    filter::SparseOutputFilter,
    output::{AggregatedCompilerOutput, Builds},
    report,
    resolver::{GraphEdges, ResolvedSources},
};
use foundry_compilers_artifacts::{Contract, sources::SourceCompilationKind};
use foundry_compilers_core::error::{Result, SolcError};
use rayon::prelude::*;
use semver::Version;
#[cfg(windows)]
use std::path::Path;
use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    path::PathBuf,
    time::Instant,
};

/// A set of different Solc installations with their version and the sources to be compiled
pub(crate) type VersionedSources<'a, L, S> = HashMap<L, Vec<(Version, Sources, (&'a str, &'a S))>>;

/// Invoked before the actual compiler invocation and can override the input.
///
/// Updates the list of identified cached mocks (if any) to be stored in cache and updates the
/// compiler input.
pub trait Preprocessor<C: Compiler>: Debug {
    fn preprocess(
        &self,
        compiler: &C,
        input: &mut C::Input,
        paths: &ProjectPathsConfig<C::Language>,
        mocks: &mut HashSet<PathBuf>,
    ) -> Result<()>;
}

#[derive(Debug)]
pub struct ProjectCompiler<
    'a,
    T: ArtifactOutput<CompilerContract = C::CompilerContract>,
    C: Compiler,
> {
    /// Contains the relationship of the source files and their imports
    edges: GraphEdges<C::Parser>,
    project: &'a Project<C, T>,
    /// A mapping from a source file path to the primary profile name selected for it.
    primary_profiles: HashMap<PathBuf, &'a str>,
    /// how to compile all the sources
    sources: CompilerSources<'a, C::Language, C::Settings>,
    /// Optional preprocessor
    preprocessor: Option<Box<dyn Preprocessor<C>>>,
}

impl<'a, T: ArtifactOutput<CompilerContract = C::CompilerContract>, C: Compiler>
    ProjectCompiler<'a, T, C>
{
    /// Create a new `ProjectCompiler` to bootstrap the compilation process of the project's
    /// sources.
    pub fn new(project: &'a Project<C, T>) -> Result<Self> {
        Self::with_sources(project, project.paths.read_input_files()?)
    }

    /// Bootstraps the compilation process by resolving the dependency graph of all sources and the
    /// appropriate `Solc` -> `Sources` set as well as the compile mode to use (parallel,
    /// sequential)
    ///
    /// Multiple (`Solc` -> `Sources`) pairs can be compiled in parallel if the `Project` allows
    /// multiple `jobs`, see [`crate::Project::set_solc_jobs()`].
    #[instrument(name = "ProjectCompiler::new", skip_all)]
    pub fn with_sources(project: &'a Project<C, T>, mut sources: Sources) -> Result<Self> {
        if let Some(filter) = &project.sparse_output {
            sources.retain(|f, _| filter.is_match(f))
        }
        let graph = Graph::resolve_sources(&project.paths, sources)?;
        let ResolvedSources { sources, primary_profiles, edges } =
            graph.into_sources_by_version(project)?;

        // If there are multiple different versions, and we can use multiple jobs we can compile
        // them in parallel.
        let jobs_cnt = || sources.values().map(|v| v.len()).sum::<usize>();
        let sources = CompilerSources {
            jobs: (project.solc_jobs > 1 && jobs_cnt() > 1).then_some(project.solc_jobs),
            sources,
        };

        Ok(Self { edges, primary_profiles, project, sources, preprocessor: None })
    }

    pub fn with_preprocessor(self, preprocessor: impl Preprocessor<C> + 'static) -> Self {
        Self { preprocessor: Some(Box::new(preprocessor)), ..self }
    }

    /// Compiles all the sources of the `Project` in the appropriate mode
    ///
    /// If caching is enabled, the sources are filtered and only _dirty_ sources are recompiled.
    ///
    /// The output of the compile process can be a mix of reused artifacts and freshly compiled
    /// `Contract`s
    ///
    /// # Examples
    /// ```no_run
    /// use foundry_compilers::Project;
    ///
    /// let project = Project::builder().build(Default::default())?;
    /// let output = project.compile()?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[instrument(name = "compile_project", skip_all)]
    pub fn compile(self) -> Result<ProjectCompileOutput<C, T>> {
        let slash_paths = self.project.slash_paths;

        // drive the compiler statemachine to completion
        let mut output = self.preprocess()?.compile()?.write_artifacts()?.write_cache()?;

        if slash_paths {
            // ensures we always use `/` paths
            output.slash_paths();
        }

        Ok(output)
    }

    /// Does basic preprocessing
    ///   - sets proper source unit names
    ///   - check cache
    #[instrument(skip_all)]
    fn preprocess(self) -> Result<PreprocessedState<'a, T, C>> {
        trace!("preprocessing");
        let Self { edges, project, mut sources, primary_profiles, preprocessor } = self;

        // convert paths on windows to ensure consistency with the `CompilerOutput` `solc` emits,
        // which is unix style `/`
        sources.slash_paths();

        let mut cache = ArtifactsCache::new(project, edges, preprocessor.is_some())?;
        // retain and compile only dirty sources and all their imports
        sources.filter(&mut cache);

        Ok(PreprocessedState { sources, cache, primary_profiles, preprocessor })
    }
}

impl<'a, C: Compiler<CompilerContract = Contract>> ProjectCompiler<'a, ConfigurableArtifacts, C> {
    /// Acquires ABI artifacts, reusing normal artifacts before consulting a separate cache.
    ///
    /// The project must request ABI output. Normal artifacts and their manifest are never
    /// modified by this operation. Additional output files and full build info retain ordinary
    /// compilation behavior. Secondary persistence requires caching and artifact writes enabled.
    pub fn compile_abi_cached(self) -> Result<ProjectCompileOutput<C>> {
        let project = self.project;
        if project.build_info || project.artifacts.additional_files != Default::default() {
            return self.compile();
        }
        let slash_paths = project.slash_paths;
        let preprocessed = self.preprocessor.is_some();
        let state = self.preprocess()?;
        let mut output = if !project.cached
            || state.sources.sources.values().flatten().all(|(_, sources, _)| sources.is_empty())
        {
            state.compile()?.write_artifacts_if(false)?.write_cache_if(false)?
        } else {
            let PreprocessedState { mut sources, cache, primary_profiles, preprocessor } = state;
            let normal_mocks = cache.mocks();
            let (normal_artifacts, normal_builds, edges) =
                cache.consume(&Artifacts::default(), &Vec::new(), false)?;
            let mut storage = Box::new(project.paths.clone());
            let mut directory = project.abi_cache_path();
            if preprocessed {
                // Preprocessors can depend on the complete compiler job, including its source
                // units. Separate storage keeps alternating filtered requests independent.
                let mut jobs = sources
                    .sources
                    .iter()
                    .flat_map(|(language, jobs)| {
                        jobs.iter().map(|(version, sources, (profile, _))| {
                            let files = sources
                                .iter()
                                .map(|(path, source)| {
                                    (
                                        path.strip_prefix(project.root()).unwrap_or(path),
                                        source.kind == SourceCompilationKind::Complete,
                                    )
                                })
                                .collect::<Vec<_>>();
                            (language.to_string(), version, profile, files)
                        })
                    })
                    .collect::<Vec<_>>();
                jobs.sort_unstable();
                let mut mocks = normal_mocks.iter().collect::<Vec<_>>();
                mocks.sort_unstable();
                let identity = serde_json::to_vec(&(jobs, mocks))?;
                directory.push(foundry_compilers_core::utils::unique_hash(identity));
            }
            storage.cache = directory.join("cache.json");
            storage.artifacts = directory.join("artifacts");
            storage.build_infos = directory.join("build-info");
            let mut cache =
                ArtifactsCache::with_storage(project, edges, preprocessed, Some(storage))?;
            let mut mocks = cache.mocks();
            mocks.extend(normal_mocks.iter().cloned());
            cache.update_mocks(mocks);
            // A preprocessor can depend on other source units in its compiler job. Reuse a
            // fully cached request, but preserve the original input on any secondary miss.
            let original_sources = preprocessed.then(|| sources.clone());
            let mut preserved_mocks = HashSet::new();
            sources.filter(&mut cache);
            if let Some(original_sources) = original_sources
                && sources.sources.values().flatten().any(|(_, sources, _)| !sources.is_empty())
            {
                preserved_mocks = cache.mocks();
                for (version, sources, (profile, _)) in original_sources.sources.values().flatten()
                {
                    for (file, source) in sources {
                        preserved_mocks.remove(file);
                        if source.kind == SourceCompilationKind::Complete {
                            cache.invalidate_artifacts(file, version, profile);
                        }
                    }
                }
                sources = original_sources;
                cache.update_mocks(normal_mocks);
            }
            let write = !project.no_artifacts;
            let mut state =
                PreprocessedState { sources, cache, primary_profiles, preprocessor }.compile()?;
            // Keep classifications for secondary artifacts outside the preprocessed jobs.
            if !preserved_mocks.is_empty() {
                let mut mocks = state.cache.mocks();
                mocks.extend(preserved_mocks);
                state.cache.update_mocks(mocks);
            }
            let mut output = state.write_artifacts_if(write)?.write_cache_if(write)?;
            for (file, contracts) in normal_artifacts {
                let cached = output.cached_artifacts.0.entry(file).or_default();
                for (name, artifacts) in contracts {
                    let existing = cached.entry(name).or_default();
                    existing.retain(|artifact| {
                        !artifacts.iter().any(|normal| {
                            normal.version == artifact.version && normal.profile == artifact.profile
                        })
                    });
                    existing.extend(artifacts);
                }
            }
            output.builds.0.extend(normal_builds.into_iter().map(|(id, context)| {
                (id, context.with_joined_paths(project.paths.root.as_path()))
            }));
            output
        };
        if slash_paths {
            output.slash_paths();
        }
        Ok(output)
    }
}

/// A series of states that comprise the [`ProjectCompiler::compile()`] state machine
///
/// The main reason is to debug all states individually
#[derive(Debug)]
struct PreprocessedState<'a, T: ArtifactOutput<CompilerContract = C::CompilerContract>, C: Compiler>
{
    /// Contains all the sources to compile.
    sources: CompilerSources<'a, C::Language, C::Settings>,

    /// Cache that holds `CacheEntry` objects if caching is enabled and the project is recompiled
    cache: ArtifactsCache<'a, T, C>,

    /// A mapping from a source file path to the primary profile name selected for it.
    primary_profiles: HashMap<PathBuf, &'a str>,

    /// Optional preprocessor
    preprocessor: Option<Box<dyn Preprocessor<C>>>,
}

impl<'a, T: ArtifactOutput<CompilerContract = C::CompilerContract>, C: Compiler>
    PreprocessedState<'a, T, C>
{
    /// advance to the next state by compiling all sources
    #[instrument(skip_all)]
    fn compile(self) -> Result<CompiledState<'a, T, C>> {
        trace!("compiling");
        let PreprocessedState { sources, mut cache, primary_profiles, preprocessor } = self;

        let mut output = sources.compile(&mut cache, preprocessor)?;

        // source paths get stripped before handing them over to solc, so solc never uses absolute
        // paths, instead `--base-path <root dir>` is set. this way any metadata that's derived from
        // data (paths) is relative to the project dir and should be independent of the current OS
        // disk. However internally we still want to keep absolute paths, so we join the
        // contracts again
        output.join_all(cache.project().root());

        Ok(CompiledState { output, cache, primary_profiles })
    }
}

/// Represents the state after `solc` was successfully invoked
#[derive(Debug)]
struct CompiledState<'a, T: ArtifactOutput<CompilerContract = C::CompilerContract>, C: Compiler> {
    output: AggregatedCompilerOutput<C>,
    cache: ArtifactsCache<'a, T, C>,
    primary_profiles: HashMap<PathBuf, &'a str>,
}

impl<'a, T: ArtifactOutput<CompilerContract = C::CompilerContract>, C: Compiler>
    CompiledState<'a, T, C>
{
    /// advance to the next state by handling all artifacts
    ///
    /// Writes all output contracts to disk if enabled in the `Project` and if the build was
    /// successful
    #[instrument(skip_all)]
    fn write_artifacts(self) -> Result<ArtifactsState<'a, T, C>> {
        let write = !self.cache.project().no_artifacts;
        self.write_artifacts_if(write)
    }

    fn write_artifacts_if(self, write: bool) -> Result<ArtifactsState<'a, T, C>> {
        let CompiledState { output, cache, primary_profiles } = self;
        let optional_storage =
            matches!(&cache, ArtifactsCache::Cached(inner) if inner.storage_paths.is_some());

        let project = cache.project();
        let ctx = cache.output_ctx();
        // write all artifacts via the handler but only if the build succeeded and project wasn't
        // configured with `no_artifacts == true`
        let compiled_artifacts = if !write {
            project.artifacts_handler().output_to_artifacts(
                &output.contracts,
                &output.sources,
                ctx,
                cache.storage_paths(),
                &primary_profiles,
            )
        } else if output.has_error(
            &project.ignored_error_codes,
            &project.ignored_error_codes_from,
            &project.ignored_file_paths,
            &project.compiler_severity_filter,
        ) {
            trace!("skip writing cache file due to solc errors: {:?}", output.errors);
            project.artifacts_handler().output_to_artifacts(
                &output.contracts,
                &output.sources,
                ctx,
                cache.storage_paths(),
                &primary_profiles,
            )
        } else {
            trace!(
                "handling artifact output for {} contracts and {} sources",
                output.contracts.len(),
                output.sources.len()
            );
            // this emits the artifacts via the project's artifacts handler
            let artifacts = match project.artifacts_handler().on_output(
                &output.contracts,
                &output.sources,
                cache.storage_paths(),
                ctx,
                &primary_profiles,
            ) {
                Ok(artifacts) => artifacts,
                Err(err) if optional_storage && matches!(err, SolcError::Io(_)) => {
                    debug!(%err, "ABI cache unavailable; returning in-memory artifacts");
                    let compiled_artifacts = project.artifacts_handler().output_to_artifacts(
                        &output.contracts,
                        &output.sources,
                        cache.output_ctx(),
                        cache.storage_paths(),
                        &primary_profiles,
                    );
                    return Ok(ArtifactsState {
                        output,
                        cache,
                        compiled_artifacts,
                        persist: false,
                    });
                }
                Err(err) => return Err(err),
            };

            // emits all the build infos, if they exist
            if let Err(err) = output.write_build_infos(&cache.storage_paths().build_infos) {
                if optional_storage && matches!(err, SolcError::Io(_)) {
                    debug!(%err, "ABI build context cache unavailable");
                    return Ok(ArtifactsState {
                        output,
                        cache,
                        compiled_artifacts: artifacts,
                        persist: false,
                    });
                }
                return Err(err);
            }

            artifacts
        };

        Ok(ArtifactsState { output, cache, compiled_artifacts, persist: true })
    }
}

/// Represents the state after all artifacts were written to disk
#[derive(Debug)]
struct ArtifactsState<'a, T: ArtifactOutput<CompilerContract = C::CompilerContract>, C: Compiler> {
    output: AggregatedCompilerOutput<C>,
    cache: ArtifactsCache<'a, T, C>,
    compiled_artifacts: Artifacts<T::Artifact>,
    persist: bool,
}

impl<T: ArtifactOutput<CompilerContract = C::CompilerContract>, C: Compiler>
    ArtifactsState<'_, T, C>
{
    /// Writes the cache file
    ///
    /// this concludes the [`Project::compile()`] statemachine
    #[instrument(skip_all)]
    fn write_cache(self) -> Result<ProjectCompileOutput<C, T>> {
        let write = !self.cache.project().no_artifacts;
        self.write_cache_if(write)
    }

    fn write_cache_if(self, write: bool) -> Result<ProjectCompileOutput<C, T>> {
        let ArtifactsState { output, cache, compiled_artifacts, persist } = self;
        let project = cache.project();
        let ignored_error_codes = project.ignored_error_codes.clone();
        let ignored_error_codes_from = project.ignored_error_codes_from.clone();
        let ignored_file_paths = project.ignored_file_paths.clone();
        let compiler_severity_filter = project.compiler_severity_filter;
        let has_error = output.has_error(
            &ignored_error_codes,
            &ignored_error_codes_from,
            &ignored_file_paths,
            &compiler_severity_filter,
        );
        let skip_write_to_disk = !write || !persist || has_error;
        trace!(has_error, project.no_artifacts, skip_write_to_disk, cache_path=?project.cache_path(),"prepare writing cache file");

        let (cached_artifacts, cached_builds, edges) =
            cache.consume(&compiled_artifacts, &output.build_infos, !skip_write_to_disk)?;

        project.artifacts_handler().handle_cached_artifacts(&cached_artifacts)?;

        let builds = Builds(
            output
                .build_infos
                .iter()
                .map(|build_info| (build_info.id.clone(), build_info.build_context.clone()))
                .chain(cached_builds)
                .map(|(id, context)| (id, context.with_joined_paths(project.paths.root.as_path())))
                .collect(),
        );

        Ok(ProjectCompileOutput {
            compiler_output: output,
            compiled_artifacts,
            cached_artifacts,
            ignored_error_codes,
            ignored_error_codes_from,
            ignored_file_paths,
            compiler_severity_filter,
            builds,
            edges,
        })
    }
}

/// Determines how the `solc <-> sources` pairs are executed.
#[derive(Debug, Clone)]
struct CompilerSources<'a, L, S> {
    /// The sources to compile.
    sources: VersionedSources<'a, L, S>,
    /// The number of jobs to use for parallel compilation.
    jobs: Option<usize>,
}

impl<L: Language, S: CompilerSettings> CompilerSources<'_, L, S> {
    /// Converts all `\\` separators to `/`.
    ///
    /// This effectively ensures that `solc` can find imported files like `/src/Cheats.sol` in the
    /// VFS (the `CompilerInput` as json) under `src/Cheats.sol`.
    #[allow(clippy::missing_const_for_fn)]
    fn slash_paths(&mut self) {
        #[cfg(windows)]
        {
            use path_slash::PathBufExt;

            self.sources.values_mut().for_each(|versioned_sources| {
                versioned_sources.iter_mut().for_each(|(_, sources, _)| {
                    *sources = std::mem::take(sources)
                        .into_iter()
                        .map(|(path, source)| {
                            (PathBuf::from(path.to_slash_lossy().as_ref()), source)
                        })
                        .collect()
                })
            });
        }
    }

    /// Filters out all sources that don't need to be compiled, see [`ArtifactsCache::filter`]
    #[instrument(name = "CompilerSources::filter", skip_all)]
    fn filter<
        T: ArtifactOutput<CompilerContract = C::CompilerContract>,
        C: Compiler<Language = L>,
    >(
        &mut self,
        cache: &mut ArtifactsCache<'_, T, C>,
    ) {
        cache.remove_dirty_sources();
        for versioned_sources in self.sources.values_mut() {
            for (version, sources, (profile, _)) in versioned_sources {
                trace!("Filtering {} sources for {}", sources.len(), version);
                cache.filter(sources, version, profile);
                trace!(
                    "Detected {} sources to compile {:?}",
                    sources.dirty().count(),
                    sources.dirty_files().collect::<Vec<_>>()
                );
            }
        }
    }

    /// Compiles all the files with `Solc`
    fn compile<
        C: Compiler<Language = L, Settings = S>,
        T: ArtifactOutput<CompilerContract = C::CompilerContract>,
    >(
        self,
        cache: &mut ArtifactsCache<'_, T, C>,
        preprocessor: Option<Box<dyn Preprocessor<C>>>,
    ) -> Result<AggregatedCompilerOutput<C>> {
        let project = cache.project();
        let graph = cache.graph();

        let jobs_cnt = self.jobs;

        let sparse_output = SparseOutputFilter::new(project.sparse_output.as_deref());

        // Include additional paths collected during graph resolution.
        let mut include_paths = project.paths.include_paths.clone();
        include_paths.extend(graph.include_paths().clone());

        // Get current list of mocks from cache. This will be passed to preprocessors and updated
        // accordingly, then set back in cache.
        let mut mocks = cache.mocks();

        #[cfg(windows)]
        let contextual_roots = {
            use path_slash::PathBufExt as _;

            project
                .paths
                .remappings
                .iter()
                .filter_map(|remapping| remapping.context.as_deref())
                .map(PathBuf::from_slash)
                .filter_map(|context| {
                    crate::utils::normalize_solidity_import_path(&project.paths.root, &context).ok()
                })
                .collect::<Vec<_>>()
        };

        let mut jobs = Vec::new();
        for (language, versioned_sources) in self.sources {
            for (version, sources, (profile, opt_settings)) in versioned_sources {
                let mut opt_settings = opt_settings.clone();
                if sources.is_empty() {
                    // nothing to compile
                    trace!("skip {} for empty sources set", version);
                    continue;
                }

                // depending on the composition of the filtered sources, the output selection can be
                // optimized
                let actually_dirty =
                    sparse_output.sparse_sources(&sources, &mut opt_settings, graph);

                if actually_dirty.is_empty() {
                    // nothing to compile for this particular language, all dirty files are in the
                    // other language set
                    trace!("skip {} run due to empty source set", version);
                    continue;
                }

                trace!("calling {} with {} sources {:?}", version, sources.len(), sources.keys());

                let settings = opt_settings
                    .with_base_path(&project.paths.root)
                    .with_allow_paths(&project.paths.allowed_paths)
                    .with_include_paths(&include_paths)
                    .with_remappings(&project.paths.remappings);

                // Keep graph, cache, and sparse-output keys absolute. Contextual sources outside
                // the project root only need root-relative names in the compiler input.
                #[cfg(windows)]
                let sources = sources
                    .into_iter()
                    .map(|(path, source)| {
                        (
                            compiler_source_unit_path(
                                &path,
                                &project.paths.root,
                                &contextual_roots,
                            ),
                            source,
                        )
                    })
                    .collect();

                let mut input = C::Input::build(sources, settings, language, version.clone());

                input.strip_prefix(project.paths.root.as_path());

                if let Some(preprocessor) = preprocessor.as_ref() {
                    preprocessor.preprocess(
                        &project.compiler,
                        &mut input,
                        &project.paths,
                        &mut mocks,
                    )?;
                }

                jobs.push((input, profile, actually_dirty));
            }
        }

        // Update cache with mocks updated by preprocessors.
        cache.update_mocks(mocks);

        let results = if let Some(num_jobs) = jobs_cnt {
            compile_parallel(&project.compiler, jobs, num_jobs)
        } else {
            compile_sequential(&project.compiler, jobs)
        }?;

        let mut aggregated = AggregatedCompilerOutput::default();

        for (input, mut output, profile, actually_dirty) in results {
            let version = input.version();

            // Mark all files as seen by the compiler
            for file in &actually_dirty {
                cache.compiler_seen(file);
            }

            let build_info = RawBuildInfo::new(&input, &output, project.build_info)?;

            #[cfg(windows)]
            {
                let internal_paths = actually_dirty
                    .iter()
                    .map(|path| {
                        let source_unit =
                            compiler_source_unit_path(path, &project.paths.root, &contextual_roots);
                        let source_unit = source_unit
                            .strip_prefix(project.paths.root.as_path())
                            .unwrap_or(&source_unit);
                        (source_unit.to_string_lossy().to_lowercase(), path.clone())
                    })
                    .collect::<HashMap<_, _>>();

                output.retain_files(internal_paths.keys());
                output.contracts = std::mem::take(&mut output.contracts)
                    .into_iter()
                    .map(|(path, contracts)| {
                        let internal = internal_paths
                            .get(&path.to_string_lossy().to_lowercase())
                            .cloned()
                            .unwrap_or_else(|| project.paths.root.join(path));
                        (internal, contracts)
                    })
                    .collect();
                output.sources = std::mem::take(&mut output.sources)
                    .into_iter()
                    .map(|(path, source)| {
                        let internal = internal_paths
                            .get(&path.to_string_lossy().to_lowercase())
                            .cloned()
                            .unwrap_or_else(|| project.paths.root.join(path));
                        (internal, source)
                    })
                    .collect();
            }
            #[cfg(not(windows))]
            {
                output.retain_files(
                    actually_dirty
                        .iter()
                        .map(|f| f.strip_prefix(project.paths.root.as_path()).unwrap_or(f)),
                );
                output.join_all(project.paths.root.as_path());
            }

            aggregated.extend(version.clone(), build_info, profile, output);
        }

        Ok(aggregated)
    }
}

#[cfg(windows)]
fn relative_path(path: &Path, base: &Path) -> Option<PathBuf> {
    let mut path = path.components().peekable();
    let mut base = base.components().peekable();

    while path.peek() == base.peek() && path.peek().is_some() {
        path.next();
        base.next();
    }
    if matches!(path.peek(), Some(std::path::Component::Prefix(_)))
        || matches!(base.peek(), Some(std::path::Component::Prefix(_)))
    {
        return None;
    }

    let mut relative = PathBuf::new();
    for component in base {
        if matches!(component, std::path::Component::Normal(_)) {
            relative.push("..");
        }
    }
    relative.extend(path);
    Some(relative)
}

#[cfg(windows)]
fn compiler_source_unit_path(path: &Path, root: &Path, contextual_roots: &[PathBuf]) -> PathBuf {
    use path_slash::PathExt;

    let path = if contextual_roots.iter().any(|context| path.starts_with(context)) {
        relative_path(path, root).unwrap_or_else(|| path.to_path_buf())
    } else {
        path.to_path_buf()
    };
    PathBuf::from(path.to_slash_lossy().as_ref())
}

type CompilationResult<'a, I, E, C> = Result<Vec<(I, CompilerOutput<E, C>, &'a str, Vec<PathBuf>)>>;

/// Compiles the input set sequentially and returns a [Vec] of outputs.
fn compile_sequential<'a, C: Compiler>(
    compiler: &C,
    jobs: Vec<(C::Input, &'a str, Vec<PathBuf>)>,
) -> CompilationResult<'a, C::Input, C::CompilationError, C::CompilerContract> {
    jobs.into_iter()
        .map(|(input, profile, actually_dirty)| {
            let start = Instant::now();
            report::compiler_spawn(
                &input.compiler_name(),
                input.version(),
                profile,
                input.settings_summary().as_deref(),
                actually_dirty.as_slice(),
            );
            let output = compiler.compile(&input)?;
            report::compiler_success(&input.compiler_name(), input.version(), &start.elapsed());

            Ok((input, output, profile, actually_dirty))
        })
        .collect()
}

/// compiles the input set using `num_jobs` threads
fn compile_parallel<'a, C: Compiler>(
    compiler: &C,
    jobs: Vec<(C::Input, &'a str, Vec<PathBuf>)>,
    num_jobs: usize,
) -> CompilationResult<'a, C::Input, C::CompilationError, C::CompilerContract> {
    // need to get the currently installed reporter before installing the pool, otherwise each new
    // thread in the pool will get initialized with the default value of the `thread_local!`'s
    // localkey. This way we keep access to the reporter in the rayon pool
    let scoped_report = report::get_default(|reporter| reporter.clone());

    // start a rayon threadpool that will execute all `Solc::compile()` processes
    let pool = rayon::ThreadPoolBuilder::new().num_threads(num_jobs).build().unwrap();

    pool.install(move || {
        jobs.into_par_iter()
            .map(move |(input, profile, actually_dirty)| {
                // set the reporter on this thread
                let _guard = report::set_scoped(&scoped_report);

                let start = Instant::now();
                report::compiler_spawn(
                    &input.compiler_name(),
                    input.version(),
                    profile,
                    input.settings_summary().as_deref(),
                    actually_dirty.as_slice(),
                );
                compiler.compile(&input).map(move |output| {
                    report::compiler_success(
                        &input.compiler_name(),
                        input.version(),
                        &start.elapsed(),
                    );
                    (input, output, profile, actually_dirty)
                })
            })
            .collect()
    })
}

#[cfg(test)]
#[cfg(all(feature = "project-util", feature = "svm-solc"))]
mod tests {
    use std::path::Path;

    use foundry_compilers_artifacts::output_selection::ContractOutputSelection;

    use crate::{
        ConfigurableArtifacts, MinimalCombinedArtifacts, compilers::multi::MultiCompiler,
        project_util::TempProject,
    };

    use super::*;

    fn init_tracing() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init()
            .ok();
    }

    #[cfg(windows)]
    #[test]
    fn external_source_unit_is_relative_to_project_root() {
        let root = Path::new(r"C:\workspace\utils");
        let source = Path::new(r"C:\workspace\node_modules\dependency\src\Core.sol");
        let contexts = [PathBuf::from(r"C:\workspace\node_modules\dependency")];

        assert_eq!(
            compiler_source_unit_path(source, root, &contexts),
            Path::new("../node_modules/dependency/src/Core.sol")
        );
    }

    #[cfg(windows)]
    #[test]
    fn project_source_unit_remains_absolute() {
        let root = Path::new(r"C:\workspace\utils");
        let source = Path::new(r"C:\workspace\utils\src\Contract.sol");
        let contexts = [PathBuf::from(r"C:\workspace\node_modules\dependency")];

        assert_eq!(
            compiler_source_unit_path(source, root, &contexts),
            Path::new("C:/workspace/utils/src/Contract.sol")
        );
    }

    #[test]
    fn can_preprocess() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/dapp-sample");
        let project = Project::builder()
            .paths(ProjectPathsConfig::dapptools(&root).unwrap())
            .build(Default::default())
            .unwrap();

        let compiler = ProjectCompiler::new(&project).unwrap();
        let prep = compiler.preprocess().unwrap();
        let cache = prep.cache.as_cached().unwrap();
        // ensure that we have exactly 3 empty entries which will be filled on compilation.
        assert_eq!(cache.cache.files.len(), 3);
        assert!(cache.cache.files.values().all(|v| v.artifacts.is_empty()));

        let compiled = prep.compile().unwrap();
        assert_eq!(compiled.output.contracts.files().count(), 3);
    }

    #[test]
    fn can_detect_cached_files() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/dapp-sample");
        let paths = ProjectPathsConfig::builder().sources(root.join("src")).lib(root.join("lib"));
        let project = TempProject::<MultiCompiler, MinimalCombinedArtifacts>::new(paths).unwrap();

        let compiled = project.compile().unwrap();
        compiled.assert_success();

        let inner = project.project();
        let compiler = ProjectCompiler::new(inner).unwrap();
        let prep = compiler.preprocess().unwrap();
        assert!(prep.cache.as_cached().unwrap().dirty_sources.is_empty())
    }

    #[test]
    fn can_recompile_with_optimized_output() {
        let tmp = TempProject::<MultiCompiler, ConfigurableArtifacts>::dapptools().unwrap();

        tmp.add_source(
            "A",
            r#"
    pragma solidity ^0.8.10;
    import "./B.sol";
    contract A {}
   "#,
        )
        .unwrap();

        tmp.add_source(
            "B",
            r#"
    pragma solidity ^0.8.10;
    contract B {
        function hello() public {}
    }
    import "./C.sol";
   "#,
        )
        .unwrap();

        tmp.add_source(
            "C",
            r"
    pragma solidity ^0.8.10;
    contract C {
            function hello() public {}
    }
   ",
        )
        .unwrap();
        let compiled = tmp.compile().unwrap();
        compiled.assert_success();

        tmp.artifacts_snapshot().unwrap().assert_artifacts_essentials_present();

        // modify A.sol
        tmp.add_source(
            "A",
            r#"
    pragma solidity ^0.8.10;
    import "./B.sol";
    contract A {
        function testExample() public {}
    }
   "#,
        )
        .unwrap();

        let compiler = ProjectCompiler::new(tmp.project()).unwrap();
        let state = compiler.preprocess().unwrap();
        let sources = &state.sources.sources;

        let cache = state.cache.as_cached().unwrap();

        // 2 clean sources
        assert_eq!(cache.cache.artifacts_len(), 2);
        assert!(cache.cache.all_artifacts_exist());
        assert_eq!(cache.dirty_sources.len(), 1);

        let len = sources.values().map(|v| v.len()).sum::<usize>();
        // single solc
        assert_eq!(len, 1);

        let filtered = &sources.values().next().unwrap()[0].1;

        // 3 contracts total
        assert_eq!(filtered.0.len(), 3);
        // A is modified
        assert_eq!(filtered.dirty().count(), 1);
        assert!(filtered.dirty_files().next().unwrap().ends_with("A.sol"));

        let state = state.compile().unwrap();
        assert_eq!(state.output.sources.len(), 1);
        for (f, source) in state.output.sources.sources() {
            if f.ends_with("A.sol") {
                assert!(source.ast.is_some());
            } else {
                assert!(source.ast.is_none());
            }
        }

        assert_eq!(state.output.contracts.len(), 1);
        let (a, c) = state.output.contracts_iter().next().unwrap();
        assert_eq!(a, "A");
        assert!(c.abi.is_some() && c.evm.is_some());

        let state = state.write_artifacts().unwrap();
        assert_eq!(state.compiled_artifacts.as_ref().len(), 1);

        let out = state.write_cache().unwrap();

        let artifacts: Vec<_> = out.into_artifacts().collect();
        assert_eq!(artifacts.len(), 3);
        for (_, artifact) in artifacts {
            let c = artifact.into_contract_bytecode();
            assert!(c.abi.is_some() && c.bytecode.is_some() && c.deployed_bytecode.is_some());
        }

        tmp.artifacts_snapshot().unwrap().assert_artifacts_essentials_present();
    }

    #[test]
    #[ignore]
    fn can_compile_real_project() {
        init_tracing();
        let paths = ProjectPathsConfig::builder()
            .root("../../foundry-integration-tests/testdata/solmate")
            .build()
            .unwrap();
        let project = Project::builder().paths(paths).build(Default::default()).unwrap();
        let compiler = ProjectCompiler::new(&project).unwrap();
        let _out = compiler.compile().unwrap();
    }

    #[test]
    fn extra_output_cached() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/dapp-sample");
        let paths = ProjectPathsConfig::builder().sources(root.join("src")).lib(root.join("lib"));
        let mut project = TempProject::<MultiCompiler>::new(paths).unwrap();

        // Compile once without enabled extra output
        project.compile().unwrap();

        // Enable extra output of abi
        project.project_mut().artifacts =
            ConfigurableArtifacts::new([], [ContractOutputSelection::Abi]);

        // Ensure that abi appears after compilation and that we didn't recompile anything
        let abi_path = project.project().paths.artifacts.join("Dapp.sol/Dapp.abi.json");
        assert!(!abi_path.exists());
        let output = project.compile().unwrap();
        assert!(output.compiler_output.is_empty());
        assert!(abi_path.exists());
    }

    #[test]
    fn can_compile_leftovers_after_sparse() {
        let mut tmp = TempProject::<MultiCompiler, ConfigurableArtifacts>::dapptools().unwrap();

        tmp.add_source(
            "A",
            r#"
pragma solidity ^0.8.10;
import "./B.sol";
contract A {}
"#,
        )
        .unwrap();

        tmp.add_source(
            "B",
            r#"
pragma solidity ^0.8.10;
contract B {}
"#,
        )
        .unwrap();

        tmp.project_mut().sparse_output = Some(Box::new(|f: &Path| f.ends_with("A.sol")));
        let compiled = tmp.compile().unwrap();
        compiled.assert_success();
        assert_eq!(compiled.artifacts().count(), 1);

        tmp.project_mut().sparse_output = None;
        let compiled = tmp.compile().unwrap();
        compiled.assert_success();
        assert_eq!(compiled.artifacts().count(), 2);
    }
}
