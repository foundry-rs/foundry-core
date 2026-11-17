use crate::{EtherscanError, Result};
use std::{
    fs::create_dir_all,
    path::{Component, Path, PathBuf},
};

#[cfg(windows)]
use std::{
    ffi::OsString,
    io,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::Prefix,
};
#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::INVALID_HANDLE_VALUE,
    Storage::FileSystem::{FindClose, FindFirstFileW, WIN32_FIND_DATAW},
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
        // NTFS may expose hidden directory names through non-hidden 8.3 aliases.
        // Query only this entry: normalizing a full SMB path can require access to every ancestor.
        #[cfg(windows)]
        match windows_file_name(&dir.join(root)) {
            Ok(name) => {
                let name = name
                    .to_str()
                    .ok_or_else(|| EtherscanError::InvalidSourcePath(path.to_path_buf()))?;
                validate_contract_name(name)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
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

/// Resolves an entry's long name without normalizing its ancestors.
#[cfg(windows)]
fn windows_file_name(path: &Path) -> io::Result<OsString> {
    // FindFirstFileW accepts patterns; sources and the output directory must be literal paths.
    if path.components().any(|component| {
        matches!(component, Component::Normal(name) if name.encode_wide().any(|c| c == b'*' as u16 || c == b'?' as u16))
    }) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "wildcards in source destination"));
    }

    // Like std::fs, retain ordinary Win32 name normalization for short paths and extend long paths.
    // absolute is lexical and also rejects embedded NULs; it does not query filesystem ancestors.
    let absolute = std::path::absolute(path)?;
    let wide = absolute.as_os_str().encode_wide().collect::<Vec<_>>();
    let (prefix, skip) = if wide.len() + 1 >= 248 {
        match absolute.components().next() {
            Some(Component::Prefix(prefix)) => match prefix.kind() {
                Prefix::Disk(_) => (r"\\?\", 0),
                Prefix::UNC(..) => (r"\\?\UNC\", 2),
                Prefix::DeviceNS(_) => (r"\\?\", 4),
                _ => ("", 0),
            },
            _ => ("", 0),
        }
    } else {
        ("", 0)
    };
    let wide =
        prefix.encode_utf16().chain(wide.into_iter().skip(skip)).chain(Some(0)).collect::<Vec<_>>();
    let mut data = WIN32_FIND_DATAW::default();
    // SAFETY: wide is NUL-terminated and data is a valid output buffer.
    let handle = unsafe { FindFirstFileW(wide.as_ptr(), &mut data) };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: this is a valid search handle, and all returned data has already been copied.
    unsafe { FindClose(handle) };
    let end = data.cFileName.iter().position(|&c| c == 0).unwrap_or(data.cFileName.len());
    Ok(OsString::from_wide(&data.cFileName[..end]))
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
