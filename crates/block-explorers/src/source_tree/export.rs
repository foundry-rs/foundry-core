//! User-directed source exports.

use super::{SourceTree, destination_metadata, destination_type, validate_destination};
use crate::{EtherscanError, Result};
use std::{
    collections::{HashMap, hash_map::Entry},
    fs,
    hash::Hash,
    io::{self, Write},
    path::{Path, PathBuf},
};
use tempfile::{Builder, NamedTempFile, TempDir};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};

/// Controls how [`SourceTree::export_to`] handles existing files.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExportMode {
    /// Fail if any destination already exists.
    #[default]
    CreateNew,
    /// Allow existing regular files to be replaced.
    Overwrite,
}

/// An error preparing or publishing an exported source file.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    /// An explorer source failed the source tree's path checks.
    #[error(transparent)]
    Source(#[from] EtherscanError),
    /// A destination already exists where the export attempted to create a new file.
    #[error("source file already exists: {0:?}")]
    AlreadyExists(PathBuf),
    /// A destination has conflicting file types or is used by multiple entries.
    #[error("conflicting source destination: {0:?}")]
    Conflict(PathBuf),
    /// A filesystem operation for the indicated destination failed.
    #[error("cannot export {path:?}: {source}")]
    Io {
        /// The requested destination.
        path: PathBuf,
        /// The underlying filesystem error.
        source: io::Error,
    },
}

impl ExportError {
    fn at_destination(error: EtherscanError, path: &Path) -> Self {
        match error {
            EtherscanError::IO(err) if err.kind() == io::ErrorKind::NotADirectory => {
                Self::Conflict(path.to_owned())
            }
            EtherscanError::IO(source) => Self::Io { path: path.to_owned(), source },
            error => Self::Source(error),
        }
    }
}

impl SourceTree {
    /// Exports sources into a new or existing directory using the same final names as
    /// [`Self::write_to`]. [`ExportMode::CreateNew`] is the default policy.
    ///
    /// Checks the whole batch for existing files, conflicting types, duplicate destinations and
    /// symlinks at or below `dir` before publishing the first file. If `dir` contains `..`,
    /// the path through the last `..` must already resolve. Files are published
    /// individually in entry order. A concurrent create-only export cannot replace a file that
    /// wins the race. On Unix, new files use the usual creation permissions (0666 subject to
    /// the process umask), and replacements preserve the existing file's read, write and
    /// execute permission bits.
    ///
    /// An I/O error during publication may leave earlier files in place. As with
    /// [`Self::write_to`], callers must control the output directory and its ancestors
    /// throughout the export.
    pub fn export_to(&self, dir: &Path, mode: ExportMode) -> Result<(), ExportError> {
        // Inspect the directory entry itself even with a trailing separator.
        let dir = dir.components().as_path();
        let paths = self.output_paths();
        mode.preflight(dir, &paths)?;
        fs::create_dir_all(dir).map_err(|source| ExportError::Io { path: dir.into(), source })?;

        let files = &mut Builder::new();
        #[cfg(unix)]
        files.permissions(fs::Permissions::from_mode(0o666));
        #[cfg(unix)]
        let mut staging = None;
        for (path, entry) in paths.iter().zip(&self.entries) {
            let destination = dir.join(path);
            let io_error = |source| ExportError::Io { path: destination.clone(), source };
            if let Some(parent) = destination.parent() {
                validate_destination(dir, path)
                    .map_err(|err| ExportError::at_destination(err, &destination))?;
                fs::create_dir_all(parent).map_err(io_error)?;
                #[cfg(unix)]
                let parent = if mode == ExportMode::Overwrite {
                    // Keep replacements private until persist sets each file's permissions.
                    let relative_parent = path.parent();
                    let temp = match &staging {
                        Some((previous, temp)) if *previous == relative_parent => temp,
                        _ => {
                            let temp = private_tempdir_in(parent).map_err(io_error)?;
                            &staging.insert((relative_parent, temp)).1
                        }
                    };
                    temp.path()
                } else {
                    parent
                };
                let mut file = files.tempfile_in(parent).map_err(io_error)?;
                file.write_all(entry.contents.as_bytes()).map_err(io_error)?;
                mode.persist(file, &destination)?;
            }
        }
        Ok(())
    }
}

impl ExportMode {
    fn preflight(self, dir: &Path, paths: &[PathBuf]) -> Result<(), ExportError> {
        // Creating missing directories before `..` could reveal existing destinations.
        if let Some(parent) = dir.ancestors().find(|p| p.ends_with("..")) {
            fs::metadata(parent).map_err(|err| ExportError::at_destination(err.into(), dir))?;
        }
        if let Some(file_type) =
            destination_type(dir).map_err(|err| ExportError::at_destination(err, dir))?
            && !file_type.is_dir()
        {
            return Err(ExportError::Conflict(dir.to_owned()));
        }

        let mut staging = HashMap::new();
        for path in paths {
            let destination = dir.join(path);
            let target = validate_destination(dir, path)
                .map_err(|err| ExportError::at_destination(err, &destination))?;
            self.check_file(&destination, target.file_type)?;

            // Reserve names in a scratch directory on the target filesystem so case and Unicode
            // aliases follow its rules. Multiple spellings of one directory share a scratch tree.
            let io_error = |source| ExportError::Io { path: destination.clone(), source };
            let (anchor, id) = existing_directory(&target.directory).map_err(io_error)?;
            let temp = match staging.entry(id) {
                Entry::Occupied(entry) => entry.into_mut(),
                Entry::Vacant(entry) => entry.insert(private_tempdir_in(anchor).map_err(io_error)?),
            };
            let staged = temp.path().join(target.suffix);
            let staging_error = |source: io::Error| {
                if matches!(
                    source.kind(),
                    io::ErrorKind::AlreadyExists
                        | io::ErrorKind::NotADirectory
                        | io::ErrorKind::IsADirectory
                ) || staged.is_dir()
                {
                    ExportError::Conflict(destination.clone())
                } else {
                    io_error(source)
                }
            };
            if let Some(parent) = staged.parent() {
                fs::create_dir_all(parent).map_err(staging_error)?;
            }
            fs::File::create_new(&staged).map_err(staging_error)?;
        }
        Ok(())
    }

    fn persist(self, file: NamedTempFile, path: &Path) -> Result<(), ExportError> {
        let result = if self == Self::Overwrite
            && let Some(metadata) =
                destination_metadata(path).map_err(|err| ExportError::at_destination(err, path))?
        {
            self.check_file(path, Some(metadata.file_type()))?;
            #[cfg(unix)]
            file.as_file()
                .set_permissions(fs::Permissions::from_mode(metadata.mode() & 0o777))
                .map_err(|source| ExportError::Io { path: path.into(), source })?;
            file.persist(path)
        } else {
            file.persist_noclobber(path)
        };
        result.map(|_| ()).map_err(|err| {
            if err.error.kind() == io::ErrorKind::AlreadyExists {
                ExportError::AlreadyExists(path.into())
            } else {
                ExportError::Io { path: path.into(), source: err.error }
            }
        })
    }

    fn check_file(self, path: &Path, file_type: Option<fs::FileType>) -> Result<(), ExportError> {
        match file_type {
            Some(ty) if !ty.is_file() => Err(ExportError::Conflict(path.into())),
            Some(_) if self == Self::CreateNew => Err(ExportError::AlreadyExists(path.into())),
            _ => Ok(()),
        }
    }
}

fn private_tempdir_in(parent: &Path) -> io::Result<TempDir> {
    let builder = &mut Builder::new();
    #[cfg(unix)]
    builder.permissions(fs::Permissions::from_mode(0o700));
    builder.tempdir_in(parent)
}

// The requested export root itself may have missing parents.
fn existing_directory(path: &Path) -> io::Result<(&Path, impl Eq + Hash + use<>)> {
    for ancestor in path.ancestors() {
        let dir = if ancestor.as_os_str().is_empty() { Path::new(".") } else { ancestor };
        #[cfg(unix)]
        let id = fs::metadata(dir).map(|metadata| (metadata.dev(), metadata.ino()));
        #[cfg(not(unix))]
        let id = same_file::Handle::from_path(dir);
        match id {
            Ok(id) => return Ok((dir, id)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::ErrorKind::NotFound.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_only_persist_preserves_a_file_created_after_preflight() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("A.sol");
        let mode = ExportMode::CreateNew;
        mode.preflight(dir.path(), &["A.sol".into()]).unwrap();
        let mut pending = NamedTempFile::new_in(dir.path()).unwrap();
        pending.write_all(b"pending").unwrap();
        fs::write(&path, b"winner").unwrap();

        assert!(
            matches!(mode.persist(pending, &path), Err(ExportError::AlreadyExists(existing)) if existing == path)
        );
        assert_eq!(fs::read(path).unwrap(), b"winner");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn overwrite_persist_rechecks_the_destination_type() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("A.sol");
        let mode = ExportMode::Overwrite;
        fs::write(&path, b"original").unwrap();
        mode.preflight(dir.path(), &["A.sol".into()]).unwrap();
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();

        let pending = NamedTempFile::new_in(dir.path()).unwrap();
        assert!(
            matches!(mode.persist(pending, &path), Err(ExportError::Conflict(existing)) if existing == path)
        );
        assert!(path.is_dir());
    }
}
