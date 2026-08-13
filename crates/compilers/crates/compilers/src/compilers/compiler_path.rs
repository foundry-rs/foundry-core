use foundry_compilers_core::{
    error::{Result, SolcError},
    utils::canonicalize,
};
use std::{
    env,
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

type CompilerApprovalHandler = dyn Fn(&Path) -> Result<()> + Send + Sync;

static COMPILER_APPROVAL_HANDLER: RwLock<Option<Arc<CompilerApprovalHandler>>> = RwLock::new(None);

/// Installs the process-wide handler used to approve resolved compiler executable paths.
pub fn set_compiler_approval_handler(
    handler: impl Fn(&Path) -> Result<()> + Send + Sync + 'static,
) {
    *COMPILER_APPROVAL_HANDLER.write().unwrap_or_else(|err| err.into_inner()) =
        Some(Arc::new(handler));
}

pub(super) fn resolve_and_approve(path: PathBuf) -> Result<PathBuf> {
    resolve_and_approve_with(path, |path| {
        let handler = COMPILER_APPROVAL_HANDLER
            .read()
            .unwrap_or_else(|err| err.into_inner())
            .clone()
            .ok_or_else(|| {
                SolcError::msg(format!(
                    "compiler executable {path:?} requires approval, but no approval handler is installed"
                ))
            })?;
        handler(path)
    })
}

fn resolve_and_approve_with(
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
    env::split_paths(search_path).flat_map(|dir| executable_candidates(dir.join(path))).find_map(
        |candidate| is_executable(&candidate).then(|| canonicalize(candidate).ok()).flatten(),
    )
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(not(windows))]
fn executable_candidates(path: PathBuf) -> impl Iterator<Item = PathBuf> {
    std::iter::once(path)
}

#[cfg(windows)]
fn executable_candidates(path: PathBuf) -> impl Iterator<Item = PathBuf> {
    let candidates = if path.extension().is_none() {
        let path_ext = env::var_os("PATHEXT").unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".into());
        path_ext
            .to_string_lossy()
            .split(';')
            .filter(|ext| !ext.is_empty())
            .map(|ext| path.with_extension(ext.trim_start_matches('.')))
            .collect()
    } else {
        vec![path]
    };
    candidates.into_iter()
}

fn not_found(path: &Path) -> SolcError {
    SolcError::msg(format!("compiler executable {path:?} was not found"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = path.metadata().unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions).unwrap();
    }

    #[test]
    fn resolves_bare_name_from_search_path() {
        let dir = tempfile::tempdir().unwrap();
        #[cfg(windows)]
        let (name, file_name) = ("compiler", "compiler.exe");
        #[cfg(not(windows))]
        let (name, file_name) = ("compiler", "compiler");
        let compiler = dir.path().join(file_name);
        std::fs::write(&compiler, []).unwrap();
        #[cfg(unix)]
        make_executable(&compiler);
        let search_path = env::join_paths([dir.path()]).unwrap();

        assert_eq!(
            resolve_in_path(Path::new(name), &search_path),
            Some(canonicalize(compiler).unwrap())
        );
    }

    #[test]
    fn skips_non_executable_path_entry() {
        let first_dir = tempfile::tempdir().unwrap();
        let second_dir = tempfile::tempdir().unwrap();
        #[cfg(windows)]
        let (name, file_name) = ("compiler", "compiler.exe");
        #[cfg(not(windows))]
        let (name, file_name) = ("compiler", "compiler");
        let non_executable = first_dir.path().join(name);
        let executable = second_dir.path().join(file_name);
        std::fs::write(non_executable, []).unwrap();
        std::fs::write(&executable, []).unwrap();
        #[cfg(unix)]
        make_executable(&executable);
        let search_path = env::join_paths([first_dir.path(), second_dir.path()]).unwrap();

        assert_eq!(
            resolve_in_path(Path::new(name), &search_path),
            Some(canonicalize(executable).unwrap())
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

        let resolved = resolve_and_approve_with(alias, |path| {
            assert_eq!(path, expected);
            Ok(())
        })
        .unwrap();

        assert_eq!(resolved, expected);
    }
}
