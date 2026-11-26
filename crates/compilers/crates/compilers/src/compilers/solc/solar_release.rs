//! Installs Solar releases into `~/.foundry/solar`.

use super::{Solc, solar_releases::SOLAR_RELEASES};
use alloy_primitives::hex;
use foundry_compilers_core::{
    error::{Result, SolcError},
    utils::RuntimeOrHandle,
};
use semver::Version;
use sha2::Digest;
use std::{
    fs,
    io::{ErrorKind, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

#[cfg(unix)]
use std::{fs::Permissions, os::unix::fs::PermissionsExt};

/// The timeout for Solar release downloads.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(600);

/// The target of the Solar release archive for this platform.
///
/// Linux x86_64 uses the static musl build, which has no glibc version requirement.
const RELEASE_TARGET: Option<&str> = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
    Some("aarch64-apple-darwin")
} else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
    Some("x86_64-apple-darwin")
} else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
    Some("x86_64-unknown-linux-musl")
} else if cfg!(all(target_os = "linux", target_arch = "aarch64", target_env = "gnu")) {
    Some("aarch64-unknown-linux-gnu")
} else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
    Some("x86_64-pc-windows-msvc")
} else {
    None
};

/// The archive extension of Solar releases on this platform.
const ARCHIVE_EXTENSION: &str = if cfg!(windows) { "zip" } else { "tar.gz" };

/// A Solar release and the SHA-256 checksums of its archives.
pub(super) struct SolarRelease {
    /// The release version.
    pub(super) version: Version,
    /// The archive file names and their SHA-256 checksums.
    pub(super) archives: &'static [(&'static str, [u8; 32])],
}

impl Solc {
    /// Returns the Solar `version` installed in `~/.foundry/solar`, if any.
    ///
    /// Runs the installed binary to check its version.
    pub fn find_installed_solar(version: &Version) -> Option<Self> {
        let solar = solar_binary(version).ok()?;
        if !solar.is_file() {
            return None;
        }
        Self::new(solar).ok().filter(|solar| solar.solar_version().as_ref() == Some(version))
    }

    /// Downloads the Solar `version` release from GitHub into `~/.foundry/solar`.
    ///
    /// If the release is in [`Self::released_solar_versions`], the archive must match the checksum
    /// pinned in this crate. Otherwise, the archive is checked against the release's SHA-256
    /// checksum file, which only detects corrupted downloads. The binary must report the requested
    /// version.
    #[instrument(name = "Solc::blocking_install_solar", skip_all)]
    pub fn blocking_install_solar(version: &Version) -> Result<Self> {
        crate::report::solar_installation_start(version);
        let result = install_solar(version);
        match &result {
            Ok(_) => crate::report::solar_installation_success(version),
            Err(err) => crate::report::solar_installation_error(version, &err.to_string()),
        }
        result
    }

    /// Returns the Solar releases known to this crate, oldest first.
    ///
    /// These releases have pinned checksums. Other releases can still be installed.
    pub fn released_solar_versions() -> Vec<Version> {
        SOLAR_RELEASES.iter().map(|release| release.version.clone()).collect()
    }

    /// Returns the Solar versions installed in `~/.foundry/solar`, oldest first.
    pub fn installed_solar_versions() -> Vec<Version> {
        let Ok(dir) = solar_home() else { return Vec::new() };
        let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
        let mut versions = entries
            .filter_map(|entry| Version::parse(entry.ok()?.file_name().to_str()?).ok())
            .filter(|version| solar_binary(version).is_ok_and(|binary| binary.is_file()))
            .collect::<Vec<_>>();
        versions.sort();
        versions
    }

    /// Removes the Solar `version` binary from `~/.foundry/solar`.
    ///
    /// Returns `false` if the version is not installed. The lock file stays, as in
    /// [`Self::blocking_install_solar`].
    pub fn remove_solar(version: &Version) -> Result<bool> {
        let dir = solar_dir(version)?;
        if !dir.is_dir() {
            return Ok(false);
        }
        let _lock = lock_solar_dir(&dir)?;
        let binary = solar_binary(version)?;
        match fs::remove_file(&binary) {
            Ok(()) => Ok(true),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(false),
            Err(err) => Err(SolcError::io(err, &binary)),
        }
    }
}

fn install_solar(version: &Version) -> Result<Solc> {
    let target = RELEASE_TARGET.ok_or_else(|| {
        SolcError::msg(format!(
            "Solar releases are not available for this platform; set `solc` to the path of a solar \
             {version} binary"
        ))
    })?;
    let dir = solar_dir(version)?;
    fs::create_dir_all(&dir).map_err(|err| SolcError::io(err, &dir))?;

    let _lock = lock_solar_dir(&dir)?;
    if let Some(solar) = Solc::find_installed_solar(version) {
        return Ok(solar);
    }

    let file_name = format!("solar-{target}.{ARCHIVE_EXTENSION}");
    let url =
        format!("https://github.com/paradigmxyz/solar/releases/download/v{version}/{file_name}");
    let pinned = pinned_checksum(version, &file_name);
    let (archive, expected) = RuntimeOrHandle::new().block_on(async {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|err| SolcError::msg(format!("failed to create HTTP client: {err}")))?;
        let archive = download(&client, &url).await?;
        let expected = match pinned {
            Some(checksum) => checksum.to_vec(),
            None => parse_checksum(&download(&client, &format!("{url}.sha256")).await?, &url)?,
        };
        Ok::<_, SolcError>((archive, expected))
    })?;
    verify_checksum(&archive, &expected, &url)?;
    let binary = if cfg!(windows) { extract_zip(&archive)? } else { extract_tar_gz(&archive)? };

    // Checks the binary under a fresh path before publishing it, since versions are cached by path.
    let temp = tempfile::Builder::new()
        .suffix(std::env::consts::EXE_SUFFIX)
        .tempfile_in(&dir)
        .map_err(|err| SolcError::io(err, &dir))?;
    let (mut file, temp) = temp.into_parts();
    #[cfg(unix)]
    file.set_permissions(Permissions::from_mode(0o755)).map_err(|err| SolcError::io(err, &temp))?;
    file.write_all(&binary)
        .and_then(|()| file.sync_data())
        .map_err(|err| SolcError::io(err, &temp))?;
    // An open write handle would make executing the binary fail with "text file busy".
    drop(file);

    let found = Solc::new(&temp).map_err(|err| {
        SolcError::msg(format!("failed to get the version of Solar from {url}: {err}"))
    })?;
    if found.solar_version().as_ref() != Some(version) {
        return Err(SolcError::msg(format!(
            "downloaded Solar from {url} reports version {}, expected {version}",
            found.version
        )));
    }
    let path = solar_binary(version)?;
    temp.persist(&path).map_err(|err| SolcError::io(err.error, &path))?;
    Ok(Solc::new_with_version(path, found.version))
}

/// Returns the directory of Solar installations: `~/.foundry/solar`.
///
/// This stays outside svm's data directory, since svm-rs 0.5.26 and older fail to list installed
/// solc versions when it contains an entry that is not a version.
fn solar_home() -> Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| SolcError::msg("could not detect the user home directory"))?;
    Ok(home.join(".foundry").join("solar"))
}

/// Returns the directory of the Solar `version` installation: `~/.foundry/solar/<version>`.
fn solar_dir(version: &Version) -> Result<PathBuf> {
    Ok(solar_home()?.join(version.to_string()))
}

/// Returns the path of the installed Solar `version` binary.
fn solar_binary(version: &Version) -> Result<PathBuf> {
    Ok(solar_dir(version)?.join(format!("solar{}", std::env::consts::EXE_SUFFIX)))
}

/// Locks the Solar installation `dir`, which serializes installs and removals of one version.
///
/// The lock file is never removed, as in svm.
fn lock_solar_dir(dir: &Path) -> Result<fs::File> {
    let lock_path = dir.join(".lock");
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock_path)
        .map_err(|err| SolcError::io(err, &lock_path))?;
    lock.lock().map_err(|err| SolcError::io(err, &lock_path))?;
    Ok(lock)
}

/// Returns the checksum pinned in this crate for the archive `file_name` of the Solar `version`.
fn pinned_checksum(version: &Version, file_name: &str) -> Option<&'static [u8; 32]> {
    let release = SOLAR_RELEASES.iter().find(|release| release.version == *version)?;
    release.archives.iter().find(|(name, _)| *name == file_name).map(|(_, checksum)| checksum)
}

async fn download(client: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let response = client
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|err| SolcError::msg(format!("failed to download {url}: {err}")))?;
    let bytes = response
        .bytes()
        .await
        .map_err(|err| SolcError::msg(format!("failed to download {url}: {err}")))?;
    Ok(bytes.into())
}

/// Returns the checksum of a `<sha256>  <file>` checksum file.
fn parse_checksum(checksum: &[u8], url: &str) -> Result<Vec<u8>> {
    std::str::from_utf8(checksum)
        .ok()
        .and_then(|checksum| checksum.split_whitespace().next())
        .and_then(|checksum| hex::decode(checksum).ok())
        .ok_or_else(|| SolcError::msg(format!("invalid checksum file for {url}")))
}

/// Checks `archive` against the `expected` SHA-256 checksum.
fn verify_checksum(archive: &[u8], expected: &[u8], url: &str) -> Result<()> {
    let detected = sha2::Sha256::digest(archive);
    if detected[..] != expected[..] {
        return Err(SolcError::msg(format!(
            "checksum mismatch for {url}: expected {}, found {}",
            hex::encode(expected),
            hex::encode(detected)
        )));
    }
    Ok(())
}

/// Returns the `solar` binary from a `.tar.gz` release archive.
fn extract_tar_gz(archive: &[u8]) -> Result<Vec<u8>> {
    let err = |err: std::io::Error| SolcError::msg(format!("invalid Solar release archive: {err}"));
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(archive));
    for entry in archive.entries().map_err(err)? {
        let mut entry = entry.map_err(err)?;
        if entry.header().entry_type().is_file()
            && entry.path().map_err(err)?.file_name() == Some("solar".as_ref())
        {
            let mut binary = Vec::new();
            entry.read_to_end(&mut binary).map_err(err)?;
            return Ok(binary);
        }
    }
    Err(SolcError::msg("Solar release archive does not contain `solar`"))
}

/// Returns the `solar.exe` binary from a `.zip` release archive.
fn extract_zip(archive: &[u8]) -> Result<Vec<u8>> {
    let err = |err: zip::result::ZipError| {
        SolcError::msg(format!("invalid Solar release archive: {err}"))
    };
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(archive)).map_err(err)?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(err)?;
        if file.is_file()
            && file
                .enclosed_name()
                .is_some_and(|path| path.file_name() == Some("solar.exe".as_ref()))
        {
            let mut binary = Vec::new();
            file.read_to_end(&mut binary).map_err(|e| err(e.into()))?;
            return Ok(binary);
        }
    }
    Err(SolcError::msg("Solar release archive does not contain `solar.exe`"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_checksum_mismatch() {
        let archive = b"archive";
        let checksum = format!("{}  solar.tar.gz\n", hex::encode(sha2::Sha256::digest(archive)));
        let expected = parse_checksum(checksum.as_bytes(), "url").unwrap();
        verify_checksum(archive, &expected, "url").unwrap();
        verify_checksum(b"tampered", &expected, "url").unwrap_err();
        parse_checksum(b"not hex", "url").unwrap_err();
    }

    #[test]
    fn pins_supported_release_checksums() {
        assert!(SOLAR_RELEASES.is_sorted_by(|a, b| a.version < b.version));
        assert!(SOLAR_RELEASES.first().unwrap().version >= Version::new(0, 2, 0));
        // Every release must pin the archive that this platform downloads.
        if let Some(target) = RELEASE_TARGET {
            let file_name = format!("solar-{target}.{ARCHIVE_EXTENSION}");
            for release in SOLAR_RELEASES {
                assert!(pinned_checksum(&release.version, &file_name).is_some());
            }
        }
        assert!(
            pinned_checksum(&Version::new(0, 1, 8), "solar-x86_64-apple-darwin.tar.gz").is_none()
        );
    }

    #[test]
    fn extracts_nested_binary() {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::default(),
        ));
        for (path, data) in
            [("solar-target/README.md", &b"readme"[..]), ("solar-target/solar", b"binary")]
        {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, path, data).unwrap();
        }
        let archive = builder.into_inner().unwrap().finish().unwrap();
        assert_eq!(extract_tar_gz(&archive).unwrap(), b"binary");
        extract_tar_gz(b"not an archive").unwrap_err();
    }

    #[test]
    fn extracts_zip_binary() {
        let zip = |files: &[(&str, &[u8])]| {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            for (path, data) in files {
                writer.start_file(*path, zip::write::SimpleFileOptions::default()).unwrap();
                writer.write_all(data).unwrap();
            }
            writer.finish().unwrap().into_inner()
        };
        let archive = zip(&[("README.md", b"readme"), ("solar-target/solar.exe", b"binary")]);
        assert_eq!(extract_zip(&archive).unwrap(), b"binary");
        // Releases put `solar.exe` at the archive root.
        assert_eq!(extract_zip(&zip(&[("solar.exe", b"root")])).unwrap(), b"root");
        extract_zip(&zip(&[("solar", b"binary")])).unwrap_err();
        extract_zip(b"not an archive").unwrap_err();
    }

    #[test]
    fn can_install_solar() {
        if RELEASE_TARGET.is_none() {
            return;
        }
        let version = Version::new(0, 3, 0);
        let solar = Solc::blocking_install_solar(&version).unwrap();
        assert_eq!(solar.solar_version(), Some(version.clone()));
        assert_eq!(Solc::find_installed_solar(&version).unwrap().solc, solar.solc);
        assert!(Solc::installed_solar_versions().contains(&version));
        assert!(!Solc::remove_solar(&Version::new(0, 0, 1)).unwrap());
    }
}
