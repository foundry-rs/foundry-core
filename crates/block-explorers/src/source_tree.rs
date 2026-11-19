use crate::{EtherscanError, Result};
use std::{
    fs::create_dir_all,
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct SourceTreeEntry {
    pub path: PathBuf,
    pub contents: String,
}

#[derive(Clone, Debug)]
pub struct SourceTree {
    pub entries: Vec<SourceTreeEntry>,
}

impl SourceTree {
    /// Expands the source tree into the provided directory, sanitizing paths and adding `.sol`
    /// to extensionless files unless a companion entry already has that name.
    ///
    /// Single-file entries are supported; directory roots must be non-hidden. All final
    /// destinations are checked before creating directories or writing files. Existing symlink
    /// components below `dir` are rejected. Validation errors leave the destination untouched,
    /// while filesystem errors during writing can leave partial output.
    pub fn write_to(&self, dir: &Path) -> Result<()> {
        let paths = self
            .entries
            .iter()
            .map(|entry| {
                let mut path = sanitize_path(&entry.path);
                if path.extension().is_none() {
                    let with_extension = path.with_extension("sol");
                    if !self.entries.iter().any(|entry| entry.path == with_extension) {
                        path = with_extension;
                    }
                }
                path
            })
            .collect::<Vec<_>>();
        for path in &paths {
            validate_destination(dir, path)?;
        }

        create_dir_all(dir)?;
        for (entry, path) in self.entries.iter().zip(paths) {
            let joined = dir.join(path);
            if let Some(parent) = joined.parent() {
                create_dir_all(parent)?;
                std::fs::write(joined, &entry.contents)?;
            }
        }
        Ok(())
    }
}

/// Remove any components in a smart contract source path that could cause a directory traversal.
pub(crate) fn sanitize_path(path: impl AsRef<Path>) -> PathBuf {
    path.as_ref()
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part),
            _ => None,
        })
        .collect()
}

/// Ensures explorer sources are rooted in a single, non-hidden contract directory.
pub(crate) fn validate_contract_name(name: &str) -> Result<()> {
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(Component::Normal(name)) if is_safe_component(name))
        || components.next().is_some()
        || name.starts_with('.')
        // NTFS 8.3 aliases such as GIT~1 can resolve to hidden directories.
        || (cfg!(windows) && name.contains('~'))
    {
        return Err(EtherscanError::InvalidContractName(name.to_owned()));
    }
    Ok(())
}

fn validate_destination(dir: &Path, path: &Path) -> Result<()> {
    if !is_safe_source_path(path) {
        return Err(EtherscanError::InvalidSourcePath(path.to_path_buf()));
    }
    let mut components = path.components();
    if let Some(Component::Normal(root)) = components.next()
        && components.next().is_some()
    {
        validate_contract_name(&root.to_string_lossy())?;
    }
    // A cloned repository can contain a committed symlink such as Token -> .git.
    let mut current = dir.to_path_buf();
    for component in path.components() {
        let Component::Normal(name) = component else {
            return Err(EtherscanError::InvalidSourcePath(path.to_path_buf()));
        };
        current.push(name);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(EtherscanError::SourcePathSymlink(current));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

/// Checks that sanitization left a nonempty relative path with ordinary filesystem components.
pub(crate) fn is_safe_source_path(path: &Path) -> bool {
    let mut components = path.components();
    matches!(components.next(), Some(Component::Normal(name)) if is_safe_component(name))
        && components.all(
            |component| matches!(component, Component::Normal(name) if is_safe_component(name)),
        )
}

fn is_safe_component(name: &std::ffi::OsStr) -> bool {
    // Prevent alternate data streams; other filename restrictions belong to the filesystem.
    !cfg!(windows) || name.to_str().is_some_and(|name| !name.contains(':'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::read_dir;

    /// Ensure that the source tree is written correctly and .sol extension is added to a path with
    /// no extension.
    #[test]
    fn test_source_tree_write() {
        let tempdir = tempfile::tempdir().unwrap();
        let st = SourceTree {
            entries: vec![
                SourceTreeEntry { path: PathBuf::from("a/a.sol"), contents: String::from("Test") },
                SourceTreeEntry { path: PathBuf::from("b/b"), contents: String::from("Test 2") },
            ],
        };
        st.write_to(tempdir.path()).unwrap();
        let a_sol_path = PathBuf::new().join(&tempdir).join("a").join("a.sol");
        let b_sol_path = PathBuf::new().join(&tempdir).join("b").join("b.sol");
        assert!(a_sol_path.exists());
        assert!(b_sol_path.exists());
    }

    /// Ensure that the .. are ignored when writing the source tree to disk because of
    /// sanitization.
    #[test]
    fn test_malformed_source_tree_write() {
        let tempdir = tempfile::tempdir().unwrap();
        let st = SourceTree {
            entries: vec![
                SourceTreeEntry {
                    path: PathBuf::from("../a/a.sol"),
                    contents: String::from("Test"),
                },
                SourceTreeEntry {
                    path: PathBuf::from("../b/../b.sol"),
                    contents: String::from("Test 2"),
                },
                SourceTreeEntry {
                    path: PathBuf::from("/c/c.sol"),
                    contents: String::from("Test 3"),
                },
            ],
        };
        st.write_to(tempdir.path()).unwrap();
        let written_paths = read_dir(tempdir.path()).unwrap();
        let paths: Vec<PathBuf> =
            written_paths.into_iter().filter_map(|x| x.ok()).map(|x| x.path()).collect();
        assert_eq!(paths.len(), 3);
        assert!(paths.contains(&tempdir.path().join("a")));
        assert!(paths.contains(&tempdir.path().join("b")));
        assert!(paths.contains(&tempdir.path().join("c")));
    }

    #[cfg(windows)]
    #[test]
    fn test_windows_source_tree_write() {
        let tempdir = tempfile::tempdir().unwrap();
        let outside = tempdir.path().join("outside.sol");
        std::fs::write(&outside, "Original").unwrap();
        let dir = tempdir.path().join("sources");
        let relative = outside.strip_prefix(outside.ancestors().last().unwrap()).unwrap();
        let path = outside.to_str().unwrap();

        for (path, expected) in [
            (outside.clone(), relative.to_path_buf()),
            (PathBuf::from(path.replace('\\', "/")), relative.to_path_buf()),
            (PathBuf::from(path.replacen(":\\", ":", 1)), relative.to_path_buf()),
            (PathBuf::from(r"\\server\share\unc.sol"), PathBuf::from("unc.sol")),
            (PathBuf::from(r"\\?\C:\verbatim.sol"), PathBuf::from("verbatim.sol")),
            (
                PathBuf::from(r"\\?\UNC\server\share\verbatim-unc.sol"),
                PathBuf::from("verbatim-unc.sol"),
            ),
        ] {
            let st = SourceTree {
                entries: vec![SourceTreeEntry { path, contents: String::from("Source") }],
            };
            st.write_to(&dir).unwrap();
            assert_eq!(std::fs::read_to_string(&outside).unwrap(), "Original");
            assert_eq!(std::fs::read_to_string(dir.join(expected)).unwrap(), "Source");
        }
    }
}
