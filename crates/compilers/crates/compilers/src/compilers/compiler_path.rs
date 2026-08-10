use foundry_compilers_core::{
    error::{Result, SolcError},
    utils::canonicalize,
};
use std::{
    env,
    ffi::OsStr,
    path::{Path, PathBuf},
};

pub(super) fn resolve_and_approve(
    path: PathBuf,
    approve: impl FnOnce(&Path) -> Result<()>,
) -> Result<PathBuf> {
    let path = resolve(&path)?;
    approve(&path)?;
    Ok(path)
}

fn resolve(path: &Path) -> Result<PathBuf> {
    if path.as_os_str().is_empty() {
        return Err(not_found(path));
    }
    if path.components().count() != 1 {
        return canonicalize(path).map_err(|err| {
            SolcError::msg(format!(
                "failed to resolve compiler executable {path:?}: {}",
                err.source()
            ))
        });
    }

    let search_path = env::var_os("PATH").ok_or_else(|| not_found(path))?;
    resolve_in_path(path, &search_path).ok_or_else(|| not_found(path))
}

fn resolve_in_path(path: &Path, search_path: &OsStr) -> Option<PathBuf> {
    env::split_paths(search_path)
        .flat_map(|dir| executable_candidates(dir.join(path)))
        .find_map(|candidate| candidate.is_file().then(|| canonicalize(candidate).ok()).flatten())
}

#[cfg(not(windows))]
fn executable_candidates(path: PathBuf) -> impl Iterator<Item = PathBuf> {
    std::iter::once(path)
}

#[cfg(windows)]
fn executable_candidates(path: PathBuf) -> impl Iterator<Item = PathBuf> {
    let mut candidates = vec![path.clone()];
    if path.extension().is_none() {
        let path_ext = env::var_os("PATHEXT").unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".into());
        candidates.extend(
            path_ext
                .to_string_lossy()
                .split(';')
                .filter(|ext| !ext.is_empty())
                .map(|ext| path.with_extension(ext.trim_start_matches('.'))),
        );
    }
    candidates.into_iter()
}

fn not_found(path: &Path) -> SolcError {
    SolcError::msg(format!("compiler executable {path:?} was not found"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_bare_name_from_search_path() {
        let dir = tempfile::tempdir().unwrap();
        #[cfg(windows)]
        let (name, file_name) = ("compiler", "compiler.exe");
        #[cfg(not(windows))]
        let (name, file_name) = ("compiler", "compiler");
        let compiler = dir.path().join(file_name);
        std::fs::write(&compiler, []).unwrap();
        let search_path = env::join_paths([dir.path()]).unwrap();

        assert_eq!(
            resolve_in_path(Path::new(name), &search_path),
            Some(canonicalize(compiler).unwrap())
        );
    }

    #[cfg(unix)]
    #[test]
    fn approval_receives_and_returns_symlink_target() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let compiler = dir.path().join("compiler");
        let alias = dir.path().join("alias");
        std::fs::write(&compiler, []).unwrap();
        symlink(&compiler, &alias).unwrap();
        let expected = canonicalize(&compiler).unwrap();

        let resolved = resolve_and_approve(alias, |path| {
            assert_eq!(path, expected);
            Ok(())
        })
        .unwrap();

        assert_eq!(resolved, expected);
    }
}
