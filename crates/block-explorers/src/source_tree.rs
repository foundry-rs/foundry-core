use crate::Result;
use std::{
    borrow::Cow,
    collections::HashSet,
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
    /// Expand the source tree into the provided directory.  This method sanitizes paths to ensure
    /// that no directory traversal happens.
    pub fn write_to(&self, dir: &Path) -> Result<()> {
        self.write_to_checked(dir, |_| Ok(()))
    }

    /// Expands the source tree after validating every final relative destination.
    ///
    /// Paths are sanitized and extensionless files receive a `.sol` extension unless another
    /// entry already uses that path. The callback sees exactly the paths that will be written,
    /// in entry order, before any directories or files are created. A validation error leaves
    /// the destination untouched. Filesystem errors during writing can still leave partial output.
    pub fn write_to_checked<E>(
        &self,
        dir: &Path,
        mut validate: impl FnMut(&Path) -> std::result::Result<(), E>,
    ) -> std::result::Result<(), E>
    where
        E: From<std::io::Error>,
    {
        let paths = self.write_paths();
        for path in &paths {
            validate(path)?;
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

    fn write_paths(&self) -> Vec<Cow<'_, Path>> {
        let mut original_paths = None;
        self.entries
            .iter()
            .map(|entry| {
                let normal_components = entry.path.components().all(|component| {
                    matches!(component, Component::Normal(name) if !cfg!(windows)
                        || matches!(Path::new(name).components().next(), Some(Component::Normal(_))))
                });
                // Components omit trailing separators and `.`; those must be removed before a
                // path can be opened as a file. Windows components must also stay relative when
                // pushed individually by the sanitizer.
                let file_tail = entry.path.file_name().is_none_or(|name| {
                    entry.path.as_os_str().as_encoded_bytes().ends_with(name.as_encoded_bytes())
                });
                let path = if normal_components && file_tail {
                    Cow::Borrowed(entry.path.as_path())
                } else {
                    Cow::Owned(sanitize_path(&entry.path))
                };
                if path.extension().is_none() {
                    let with_extension = path.with_extension("sol");
                    // Match against original paths, preserving the writer's collision semantics.
                    let original_paths = original_paths.get_or_insert_with(|| {
                        self.entries.iter().map(|entry| entry.path.as_path()).collect::<HashSet<_>>()
                    });
                    if !original_paths.contains(with_extension.as_path()) {
                        return Cow::Owned(with_extension);
                    }
                }
                path
            })
            .collect()
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

    #[test]
    fn checked_write_validates_all_destinations_before_creating_output() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("sources");
        let tree = SourceTree {
            entries: vec![
                SourceTreeEntry { path: "C/First.sol".into(), contents: "first".into() },
                SourceTreeEntry { path: "C/Second".into(), contents: "second".into() },
            ],
        };
        let mut checked = Vec::new();
        let error = tree
            .write_to_checked(&output, |path| {
                checked.push(path.to_path_buf());
                if path == Path::new("C/Second.sol") {
                    return Err(std::io::Error::other("rejected destination"));
                }
                Ok(())
            })
            .unwrap_err();
        assert_eq!(error.to_string(), "rejected destination");
        assert_eq!(checked, [Path::new("C/First.sol"), Path::new("C/Second.sol")]);
        assert!(!output.exists());
    }

    #[test]
    fn checked_write_preserves_companions_sanitization_and_duplicate_order() {
        let temp = tempfile::tempdir().unwrap();
        let entries = [
            ("C/Pair", "extensionless"),
            ("C/README", "readme"),
            ("C/Pair.sol", "companion"),
            ("C/Duplicate.sol", "first"),
            ("C/Duplicate.sol", "last"),
            ("../Raw/Pair", "raw extensionless"),
            ("../Raw/Pair.sol", "raw companion"),
            ("C/Trailing.sol/", "trailing separator"),
            ("C/Dot.sol/.", "trailing dot"),
        ]
        .map(|(path, contents)| SourceTreeEntry { path: path.into(), contents: contents.into() });
        let tree = SourceTree { entries: entries.into() };
        let mut checked = Vec::new();
        tree.write_to_checked(temp.path(), |path| {
            checked.push(path.to_path_buf());
            assert!(!temp.path().join("C").exists());
            Ok::<_, std::io::Error>(())
        })
        .unwrap();
        assert_eq!(
            checked,
            [
                "C/Pair",
                "C/README.sol",
                "C/Pair.sol",
                "C/Duplicate.sol",
                "C/Duplicate.sol",
                "Raw/Pair.sol",
                "Raw/Pair.sol",
                "C/Trailing.sol",
                "C/Dot.sol",
            ]
            .map(PathBuf::from)
        );
        for (path, expected) in [
            ("C/Pair", "extensionless"),
            ("C/README.sol", "readme"),
            ("C/Pair.sol", "companion"),
            ("C/Duplicate.sol", "last"),
            ("Raw/Pair.sol", "raw companion"),
            ("C/Trailing.sol", "trailing separator"),
            ("C/Dot.sol", "trailing dot"),
        ] {
            assert_eq!(std::fs::read_to_string(temp.path().join(path)).unwrap(), expected);
        }
        assert!(!temp.path().join("Raw/Pair").exists());
    }

    #[cfg(windows)]
    #[test]
    fn checked_write_resolves_nested_drive_components_before_validation() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("sources");
        for (input, expected) in [
            (r"C\nested\C:relative.sol", "C:relative.sol"),
            (r"C\nested\D:relative.sol", "D:relative.sol"),
        ] {
            let tree = SourceTree {
                entries: vec![SourceTreeEntry { path: input.into(), contents: "source".into() }],
            };
            let error = tree
                .write_to_checked(&output, |path| {
                    assert_eq!(path, Path::new(expected));
                    Err(std::io::Error::other("rejected drive component"))
                })
                .unwrap_err();
            assert_eq!(error.to_string(), "rejected drive component");
            assert!(!output.exists());
        }
    }
}
