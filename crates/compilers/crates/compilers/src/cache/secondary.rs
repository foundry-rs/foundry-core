//! Transactional storage for optional compiler artifacts.

use super::{ArtifactsCache, CompilerCache};
use crate::{
    ArtifactOutput, Project, ProjectPathsConfig,
    compilers::{Compiler, CompilerSettings},
};
use foundry_compilers_core::{
    error::{Result, SolcError},
    utils,
};
use std::{
    fs,
    io::{self, Write},
    path::{Component, Path, PathBuf},
};
use tempfile::{Builder, TempDir};

/// Holds the store lock across snapshot loading, compilation, publication, and collection.
/// The lock file is outside the store so collection cannot replace its identity.
pub(crate) struct SecondaryCache {
    _lock: fs::File,
    root: PathBuf,
    directory: PathBuf,
    kind: SecondaryCacheKind,
}

impl SecondaryCache {
    pub(crate) fn open(
        root: PathBuf,
        directory: PathBuf,
        write: bool,
        kind: SecondaryCacheKind,
    ) -> io::Result<Self> {
        let lock = Self::lock(&root, write, kind)?;
        Ok(Self { _lock: lock, root, directory, kind })
    }

    fn lock(root: &Path, write: bool, kind: SecondaryCacheKind) -> io::Result<fs::File> {
        let lock_path = root.with_extension(match kind {
            SecondaryCacheKind::Abi => "abi.lock",
            SecondaryCacheKind::Outputs => "outputs.lock",
        });
        let lock = if write {
            if let Some(parent) = lock_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(lock_path)?
        } else {
            fs::File::open(lock_path)?
        };
        // Contention is a cache miss, not a reason to stall a compiler invocation.
        lock.try_lock()?;
        Ok(lock)
    }

    pub(crate) fn paths<L>(&self, logical: &ProjectPathsConfig<L>) -> Box<ProjectPathsConfig<L>>
    where
        L: Clone,
    {
        let generation =
            Self::current(&self.directory).unwrap_or_else(|| self.directory.join("missing"));
        let mut paths = Box::new(logical.clone());
        paths.cache = generation.join("cache.json");
        paths.artifacts = generation.join("artifacts");
        paths.build_infos = generation.join("build-info");
        paths
    }

    fn current(directory: &Path) -> Option<PathBuf> {
        let name = fs::read_to_string(directory.join("current")).ok()?;
        let mut components = Path::new(&name).components();
        (name.starts_with("generation-")
            && matches!(components.next(), Some(Component::Normal(_)))
            && components.next().is_none())
        .then(|| directory.join(name))
    }

    /// Copies every retained manifest dependency before rebasing paths used for new writes.
    pub(crate) fn stage<T: ArtifactOutput<CompilerContract = C::CompilerContract>, C: Compiler>(
        &self,
        cache: &mut ArtifactsCache<'_, T, C>,
    ) -> Result<TempDir> {
        fs::create_dir_all(&self.directory).map_err(|err| SolcError::io(err, &self.directory))?;
        let generation = Builder::new()
            .prefix("generation-")
            .tempdir_in(&self.directory)
            .map_err(|err| SolcError::io(err, &self.directory))?;
        let ArtifactsCache::Cached(inner) = cache else { unreachable!() };
        let storage = inner.storage_paths.as_mut().expect("secondary cache");
        let artifacts = generation.path().join("artifacts");
        let builds = generation.path().join("build-info");
        for artifact in inner.cache.files.values().flat_map(|entry| entry.artifacts()) {
            let relative = artifact.path.strip_prefix(&storage.artifacts).map_err(|err| {
                SolcError::io(io::Error::new(io::ErrorKind::InvalidData, err), &artifact.path)
            })?;
            Self::copy(&artifact.path, &artifacts.join(relative))?;
        }
        for id in &inner.cache.builds {
            let filename = format!("{id}.json");
            Self::copy(&storage.build_infos.join(&filename), &builds.join(filename))?;
        }
        inner
            .cache
            .strip_artifact_files_prefixes(&storage.artifacts)
            .join_artifacts_files(&artifacts);
        for artifact in inner.cached_artifacts.artifact_files_mut() {
            artifact.strip_prefix(&storage.artifacts);
            artifact.join(&artifacts);
        }
        storage.cache = generation.path().join("cache.json");
        storage.artifacts = artifacts;
        storage.build_infos = builds;
        Ok(generation)
    }

    fn copy(source: &Path, destination: &Path) -> Result<()> {
        utils::create_parent_dir_all(destination)?;
        match fs::copy(source, destination) {
            Ok(_) => Ok(()),
            // Missing artifacts already force cache misses. Do not prevent their recovery.
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(SolcError::io(err, source)),
        }
    }

    pub(crate) fn publish<
        T: ArtifactOutput<CompilerContract = C::CompilerContract>,
        C: Compiler,
    >(
        &self,
        generation: TempDir,
        project: &Project<C, T>,
    ) -> io::Result<()> {
        // A failed artifact/build/manifest write never creates a complete generation manifest.
        if !generation.path().join("cache.json").is_file() {
            return Ok(());
        }
        let mut pointer = Builder::new().prefix(".current-").tempfile_in(&self.directory)?;
        pointer.write_all(generation.path().file_name().unwrap().as_encoded_bytes())?;
        pointer.persist(self.directory.join("current")).map_err(|err| err.error)?;
        let published = generation.keep();
        Self::collect_generations(&self.directory, &published);
        let _ = fs::remove_file(self.root.join("cache.json"));
        self.prune(project);
        Ok(())
    }

    fn collect_generations(directory: &Path, published: &Path) {
        // Previous ABI caches stored these payloads directly in the context directory.
        // They are superseded only once a complete generation has been published.
        let _ = fs::remove_file(directory.join("cache.json"));
        let _ = fs::remove_dir_all(directory.join("artifacts"));
        let _ = fs::remove_dir_all(directory.join("build-info"));
        // The lock protects snapshot acquisition, not the lifetime of returned artifact paths.
        // Other acquisitions cannot load retired files while this lock is held.
        if let Ok(entries) = fs::read_dir(directory) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if path != published && name.starts_with("generation-") {
                    let _ = fs::remove_dir_all(path);
                } else if name.starts_with(".current-") {
                    let _ = fs::remove_file(path);
                }
            }
        }
    }

    /// Collects obsolete contexts using their own dependencies, never the current filtered graph.
    fn prune<T: ArtifactOutput<CompilerContract = C::CompilerContract>, C: Compiler>(
        &self,
        project: &Project<C, T>,
    ) {
        let Ok(entries) = fs::read_dir(&self.root) else { return };
        for entry in entries.flatten() {
            let directory = entry.path();
            if directory == self.directory
                || !directory.is_dir()
                || entry.file_name().to_string_lossy().starts_with("generation-")
            {
                continue;
            }
            let Some(current) = Self::current(&directory) else {
                let _ = fs::remove_dir_all(directory);
                continue;
            };
            let Ok(cache) = CompilerCache::<C::Settings>::read(&current.join("cache.json")) else {
                let _ = fs::remove_dir_all(directory);
                continue;
            };
            let obsolete = cache.paths != project.paths.paths_relative()
                || (self.kind == SecondaryCacheKind::Abi
                    && cache.profiles.iter().any(|(profile, settings)| {
                        !project.settings_profiles().any(|(name, current)| {
                            name == profile && current.can_use_cached(settings)
                        })
                    }))
                || cache.files.iter().any(|(file, entry)| {
                    match crate::artifacts::Source::read(&project.root().join(file)) {
                        Ok(source) => source.content_hash() != entry.content_hash,
                        Err(err) => err.source().kind() == io::ErrorKind::NotFound,
                    }
                });
            if obsolete {
                let _ = fs::remove_dir_all(directory);
            } else {
                Self::collect_generations(&directory, &current);
            }
        }
    }

    /// Removes output artifacts without racing a reader or publisher. Busy stores are retained.
    pub(crate) fn cleanup_outputs(root: &Path) -> io::Result<()> {
        if !root.exists() {
            return Ok(());
        }
        let _lock = match Self::lock(root, true, SecondaryCacheKind::Outputs) {
            Ok(lock) => lock,
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                debug!(?root, "output artifact cache is busy; skipping cleanup");
                return Ok(());
            }
            Err(err) => return Err(err),
        };
        let result = if root.is_dir() { fs::remove_dir_all(root) } else { fs::remove_file(root) };
        match result {
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    }

    /// Ordinary cleanup must not recursively remove a locked output store or its sibling lock.
    pub(crate) fn outputs_path_is_safe<L>(root: &Path, paths: &ProjectPathsConfig<L>) -> bool {
        let resolve = |path: &Path| {
            let existing = path.ancestors().find(|path| path.exists())?;
            let suffix = path.strip_prefix(existing).ok()?;
            if suffix.components().any(|part| !matches!(part, Component::Normal(_))) {
                return None;
            }
            Some(utils::canonicalize(existing).ok()?.join(suffix))
        };
        // A final symlink can redirect storage while its sibling lock stays beside the entry.
        let (Some(target), Some(parent), Some(name)) =
            (resolve(root), root.parent().and_then(resolve), root.file_name())
        else {
            return false;
        };
        let entry = parent.join(name);
        [&paths.artifacts, &paths.build_infos].into_iter().all(|path| {
            path == &paths.root
                || resolve(path)
                    .is_some_and(|path| !target.starts_with(&path) && !entry.starts_with(path))
        })
    }
}

/// Selects the compiler-owned namespace and its context retention policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SecondaryCacheKind {
    Abi,
    Outputs,
}
