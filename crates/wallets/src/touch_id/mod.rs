//! Touch ID-protected keystore unlocking on macOS.
//!
//! Enrolling a keystore wraps its password with a P-256 key that lives inside the
//! Secure Enclave and is guarded by an access-control policy (Touch ID by default).
//! The wrapped password and the enclave key's encrypted `dataRepresentation` are
//! stored in a sidecar file next to the keystore JSON, which remains the canonical,
//! portable copy of the key. Unlocking asks the enclave to unwrap the password,
//! which triggers the hardware-enforced Touch ID prompt.
//!
//! This deliberately uses no macOS Keychain items: biometry-protected keychain
//! items require provisioning-profile entitlements that a plain CLI cannot carry,
//! and file-based keychain ACLs break on every binary upgrade. The Secure Enclave
//! blob is device-bound; a Mac migration or (under [`Policy::CurrentBiometry`])
//! a biometric re-enrollment invalidates it.
//!
//! Failures are structured so callers can tell user intent from environment:
//! [`TouchIdError::Canceled`] means the user declined and must abort rather than
//! degrade into a password prompt, [`TouchIdError::Unavailable`] means
//! authentication cannot happen right now (headless session, no enclave) and is
//! the only case where falling back to the prompt is appropriate, and an
//! invalidated or corrupt sidecar is a hard error that requires an explicit
//! re-enroll or removal.

use std::{
    ffi::CString,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use alloy_primitives::hex;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// Extension of the sidecar file stored next to the keystore JSON.
const SIDECAR_EXT: &str = "touchid";
/// Current sidecar format version; bump when the on-disk schema changes.
const SIDECAR_VERSION: u32 = 1;

/// Access-control policy for the Secure Enclave wrap key.
///
/// Every public policy requires user presence; a wrap key that unwraps without
/// any user interaction would defeat the purpose of enrollment.
// kebab-case so the persisted values match a future clap `ValueEnum` policy flag.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Policy {
    /// Touch ID, with device password fallback.
    #[default]
    UserPresence,
    /// Strictly the currently enrolled biometrics; re-enrollment invalidates the key.
    CurrentBiometry,
    /// No user interaction; the secret is only bound to this device's Secure
    /// Enclave. Test-only: it lets CI exercise the full create/wrap/unwrap FFI
    /// path non-interactively, and release builds reject `device-only` sidecars
    /// at parse time.
    #[cfg(test)]
    DeviceOnly,
}

impl Policy {
    /// Raw value shared with the `policy*` constants in `shim.swift`.
    const fn raw(self) -> i32 {
        match self {
            Self::UserPresence => 1,
            Self::CurrentBiometry => 2,
            #[cfg(test)]
            Self::DeviceOnly => 0,
        }
    }
}

/// Errors produced by Touch ID keystore enrollment and unlocking.
///
/// Only [`TouchIdError::NotEnrolled`] and [`TouchIdError::Unavailable`] are
/// recoverable by falling back to the interactive password prompt; every other
/// variant is either an explicit user decision ([`TouchIdError::Canceled`]) or
/// a sidecar that must be re-enrolled or removed before unlocking proceeds.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TouchIdError {
    #[error("keystore is not enrolled for Touch ID unlock")]
    NotEnrolled,
    #[error(
        "unsupported Touch ID sidecar version {0}; re-enroll this keystore to regenerate it, \
         or delete its `.touchid` sidecar to use the password prompt"
    )]
    UnsupportedVersion(u32),
    /// The user dismissed the authentication prompt.
    #[error("Touch ID authentication was canceled")]
    Canceled,
    /// Authentication cannot happen in this environment (headless session, no
    /// Secure Enclave, no enrolled biometrics or passcode).
    #[error("Touch ID authentication is unavailable: {0}")]
    Unavailable(String),
    /// Current biometric authentication is temporarily locked out.
    #[error("Touch ID authentication is locked out: {0}")]
    LockedOut(String),
    /// Authentication succeeded but the enclave rejected the wrap key: the
    /// enrollment no longer matches this device's state (e.g. a
    /// [`Policy::CurrentBiometry`] key after biometric re-enrollment).
    #[error(
        "the Secure Enclave key for this keystore was invalidated ({0}); re-enroll this \
         keystore, or delete its `.touchid` sidecar to use the password prompt"
    )]
    Invalidated(String),
    /// The sidecar's key blob or sealed password failed to decode or
    /// authenticate: it was tampered with, truncated, or copied from another Mac.
    #[error(
        "corrupt Touch ID sidecar ({0}); re-enroll this keystore, or delete its `.touchid` \
         sidecar to use the password prompt"
    )]
    CorruptSidecar(String),
    #[error(
        "the Touch ID-stored password does not match this keystore; re-enroll this keystore, \
         or delete its `.touchid` sidecar to use the password prompt"
    )]
    PasswordMismatch,
    #[error("Secure Enclave: {0}")]
    SecureEnclave(String),
    #[error("failed to access the Touch ID sidecar at `{path}`: {source}")]
    SidecarIo { path: PathBuf, source: std::io::Error },
    #[error(
        "invalid Touch ID sidecar: {0}; re-enroll this keystore, or delete its `.touchid` \
         sidecar to use the password prompt"
    )]
    InvalidSidecar(#[from] serde_json::Error),
    #[error(
        "invalid hex in Touch ID sidecar: {0}; re-enroll this keystore, or delete its \
         `.touchid` sidecar to use the password prompt"
    )]
    InvalidHex(#[from] hex::FromHexError),
    #[error("unwrapped password is not valid UTF-8")]
    InvalidPassword,
}

impl TouchIdError {
    /// Whether falling back to the interactive password prompt is an
    /// appropriate response to this error.
    ///
    /// Cancellation must abort — the user said no, and a prompt would let a
    /// wrapper script capture the password anyway — and invalidated or corrupt
    /// sidecars require an explicit re-enroll/remove decision rather than a
    /// downgrade an attacker could induce.
    pub const fn is_recoverable(&self) -> bool {
        matches!(self, Self::NotEnrolled | Self::Unavailable(_))
    }
}

/// Sidecar file contents: the enclave key and the password it wraps.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Sidecar {
    version: u32,
    policy: Policy,
    /// Hex-encoded, enclave-encrypted `dataRepresentation` of the P-256 wrap key.
    se_key: String,
    /// Hex-encoded ECIES ciphertext of the keystore password.
    sealed_password: String,
}

unsafe extern "C" {
    fn foundry_se_available() -> i32;
    fn foundry_se_create(policy: i32, out: *mut *mut u8, out_len: *mut usize) -> i32;
    fn foundry_se_wrap(
        blob: *const u8,
        blob_len: usize,
        plain: *const u8,
        plain_len: usize,
        out: *mut *mut u8,
        out_len: *mut usize,
    ) -> i32;
    fn foundry_se_unwrap(
        blob: *const u8,
        blob_len: usize,
        sealed: *const u8,
        sealed_len: usize,
        policy: i32,
        reason: *const std::ffi::c_char,
        out: *mut *mut u8,
        out_len: *mut usize,
    ) -> i32;
    fn foundry_se_free(ptr: *mut u8, len: usize);
}

/// Status codes shared with the `status*` constants in `shim.swift`.
mod status {
    pub(super) const OK: i32 = 0;
    pub(super) const CANCELED: i32 = 2;
    pub(super) const UNAVAILABLE: i32 = 3;
    pub(super) const INVALIDATED: i32 = 4;
    pub(super) const INVALID_DATA: i32 = 5;
    pub(super) const LOCKED_OUT: i32 = 6;
}

/// Copies out a shim result buffer, frees it, and interprets it as data or an
/// error message.
///
/// # Safety
///
/// `ptr` must either be null or, together with `len`, be the out-parameter pair
/// written by a single preceding `foundry_se_*` call and not yet freed.
unsafe fn shim_result(status: i32, ptr: *mut u8, len: usize) -> Result<Vec<u8>, TouchIdError> {
    let bytes = if ptr.is_null() {
        Vec::new()
    } else {
        // SAFETY: per this function's contract, `(ptr, len)` is a live malloc'd
        // shim buffer owned by us until the `foundry_se_free` below.
        unsafe {
            let bytes = std::slice::from_raw_parts(ptr, len).to_vec();
            foundry_se_free(ptr, len);
            bytes
        }
    };
    if status == status::OK {
        return Ok(bytes);
    }
    let message = String::from_utf8_lossy(&bytes).into_owned();
    Err(match status {
        status::CANCELED => TouchIdError::Canceled,
        status::UNAVAILABLE => TouchIdError::Unavailable(message),
        status::INVALIDATED => TouchIdError::Invalidated(message),
        status::INVALID_DATA => TouchIdError::CorruptSidecar(message),
        status::LOCKED_OUT => TouchIdError::LockedOut(message),
        _ => TouchIdError::SecureEnclave(message),
    })
}

/// Whether this machine has a usable Secure Enclave.
pub fn is_available() -> bool {
    // SAFETY: the shim function takes no arguments and only returns a flag.
    unsafe { foundry_se_available() == 1 }
}

/// Returns the sidecar path for a keystore: the keystore path with `.touchid` appended.
pub fn sidecar_path(keystore: &Path) -> PathBuf {
    let mut path = keystore.as_os_str().to_os_string();
    path.push(".");
    path.push(SIDECAR_EXT);
    PathBuf::from(path)
}

/// Whether the keystore has a Touch ID sidecar.
pub fn is_enrolled(keystore: &Path) -> bool {
    sidecar_path(keystore).exists()
}

/// Enrolls a keystore: creates a Secure Enclave wrap key under `policy` and stores
/// the wrapped `password` in the sidecar file, replacing any existing sidecar (the
/// previous wrap key and its policy are discarded). The caller is responsible for
/// having verified that `password` decrypts the keystore: unlocking treats a
/// wrapped password that fails to decrypt as tampering and aborts with
/// [`TouchIdError::PasswordMismatch`] instead of falling back to the prompt.
pub fn enroll(keystore: &Path, password: &str, policy: Policy) -> Result<(), TouchIdError> {
    let (mut ptr, mut len) = (std::ptr::null_mut(), 0);
    // SAFETY: the out parameters are valid for writes.
    let status = unsafe { foundry_se_create(policy.raw(), &raw mut ptr, &raw mut len) };
    // SAFETY: `(ptr, len)` were just written by `foundry_se_create` and not yet freed.
    let se_key = unsafe { shim_result(status, ptr, len) }?;

    let (mut ptr, mut len) = (std::ptr::null_mut(), 0);
    // SAFETY: input pointers are valid for their lengths for the duration of the call.
    let status = unsafe {
        foundry_se_wrap(
            se_key.as_ptr(),
            se_key.len(),
            password.as_ptr(),
            password.len(),
            &raw mut ptr,
            &raw mut len,
        )
    };
    // SAFETY: `(ptr, len)` were just written by `foundry_se_wrap` and not yet freed.
    let sealed_password = unsafe { shim_result(status, ptr, len) }?;

    let sidecar = Sidecar {
        version: SIDECAR_VERSION,
        policy,
        se_key: hex::encode(se_key),
        sealed_password: hex::encode(sealed_password),
    };
    write_sidecar(keystore, &sidecar)
        .map_err(|source| TouchIdError::SidecarIo { path: sidecar_path(keystore), source })
}

/// Serializes `sidecar` and atomically replaces the keystore's sidecar file.
///
/// The contents are written to a fresh `0600` temporary file in the sidecar's
/// directory and renamed into place. The rename replaces whatever occupies the
/// sidecar path — including a symlink, which an open-and-truncate would have
/// followed, letting a malicious `foo.touchid -> foo` link clobber the keystore
/// itself. It also means a crash can never leave a truncated sidecar behind,
/// and that re-enrollment repairs loosened file permissions, since the fresh
/// file's `0600` travels with the rename.
fn write_sidecar(keystore: &Path, sidecar: &Sidecar) -> std::io::Result<()> {
    let path = sidecar_path(keystore);
    let dir = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let mut tmp =
        tempfile::Builder::new().permissions(fs::Permissions::from_mode(0o600)).tempfile_in(dir)?;
    serde_json::to_writer_pretty(tmp.as_file_mut(), sidecar).map_err(std::io::Error::from)?;
    tmp.as_file().sync_all()?;
    tmp.persist(&path).map_err(|e| e.error)?;
    Ok(())
}

/// Unwraps the keystore password from the sidecar, triggering the enclave's
/// access-control prompt (Touch ID under the default policy).
///
/// Blocks the calling thread until the prompt resolves; the authentication UI
/// runs out of process, so calling from any CLI thread is fine. The returned
/// password and the intermediate buffer are zeroed on drop.
pub fn unwrap_password(keystore: &Path) -> Result<Zeroizing<String>, TouchIdError> {
    let path = sidecar_path(keystore);
    if !path.exists() {
        return Err(TouchIdError::NotEnrolled);
    }
    let raw =
        fs::read_to_string(&path).map_err(|source| TouchIdError::SidecarIo { path, source })?;
    // Gate on the version before parsing the full schema, so a future format's
    // sidecar reports its version instead of a schema mismatch.
    #[derive(Deserialize)]
    struct VersionOnly {
        version: u32,
    }
    let VersionOnly { version } = serde_json::from_str(&raw)?;
    if version != SIDECAR_VERSION {
        return Err(TouchIdError::UnsupportedVersion(version));
    }
    let sidecar: Sidecar = serde_json::from_str(&raw)?;
    let se_key = hex::decode(&sidecar.se_key)?;
    let sealed = hex::decode(&sidecar.sealed_password)?;

    let name = keystore.file_name().unwrap_or_default().to_string_lossy();
    let reason = CString::new(format!("unlock the `{name}` keystore")).unwrap_or_default();
    let (mut ptr, mut len) = (std::ptr::null_mut(), 0);
    // SAFETY: input pointers are valid for their lengths for the duration of the call.
    let status = unsafe {
        foundry_se_unwrap(
            se_key.as_ptr(),
            se_key.len(),
            sealed.as_ptr(),
            sealed.len(),
            sidecar.policy.raw(),
            reason.as_ptr(),
            &raw mut ptr,
            &raw mut len,
        )
    };
    // SAFETY: `(ptr, len)` were just written by `foundry_se_unwrap` and not yet freed.
    let bytes = Zeroizing::new(unsafe { shim_result(status, ptr, len) }?);
    let password = std::str::from_utf8(&bytes).map_err(|_| TouchIdError::InvalidPassword)?;
    Ok(Zeroizing::new(password.to_string()))
}

/// Removes the keystore's sidecar, if any. Returns whether one existed.
pub fn remove(keystore: &Path) -> Result<bool, TouchIdError> {
    let path = sidecar_path(keystore);
    if path.exists() {
        fs::remove_file(&path).map_err(|source| TouchIdError::SidecarIo { path, source })?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lockout_and_password_mismatch_do_not_fall_back() {
        assert!(!TouchIdError::LockedOut("locked".into()).is_recoverable());
        assert!(!TouchIdError::PasswordMismatch.is_recoverable());
    }

    #[test]
    fn policy_wire_format_is_stable() {
        assert_eq!(serde_json::to_string(&Policy::UserPresence).unwrap(), "\"user-presence\"");
        assert_eq!(
            serde_json::to_string(&Policy::CurrentBiometry).unwrap(),
            "\"current-biometry\""
        );
        assert_eq!(
            serde_json::from_str::<Policy>("\"user-presence\"").unwrap(),
            Policy::UserPresence
        );
    }

    /// The sidecar is a persistent on-disk format; lock its field names and
    /// order against accidental rename refactors.
    #[test]
    fn sidecar_wire_format_is_stable() {
        let sidecar = Sidecar {
            version: SIDECAR_VERSION,
            policy: Policy::UserPresence,
            se_key: "aa".into(),
            sealed_password: "bb".into(),
        };
        let json = serde_json::to_string(&sidecar).unwrap();
        assert_eq!(
            json,
            r#"{"version":1,"policy":"user-presence","se_key":"aa","sealed_password":"bb"}"#
        );
        assert_eq!(serde_json::from_str::<Sidecar>(&json).unwrap(), sidecar);
    }

    /// Corrupt sidecars must surface structured errors without touching the
    /// enclave, so these paths are testable on any machine.
    #[test]
    fn corrupt_sidecars_report_structured_errors() {
        let dir = tempfile::tempdir().unwrap();
        let keystore = dir.path().join("k");
        fs::write(&keystore, "{}").unwrap();

        fs::write(sidecar_path(&keystore), "not json").unwrap();
        assert!(matches!(unwrap_password(&keystore), Err(TouchIdError::InvalidSidecar(_))));

        fs::write(
            sidecar_path(&keystore),
            r#"{"version":1,"policy":"user-presence","se_key":"zz","sealed_password":"bb"}"#,
        )
        .unwrap();
        assert!(matches!(unwrap_password(&keystore), Err(TouchIdError::InvalidHex(_))));
    }

    #[test]
    fn sidecar_path_appends_extension() {
        assert_eq!(sidecar_path(Path::new("/k/deployer")), Path::new("/k/deployer.touchid"));
        // Dots in keystore names must not be treated as extensions.
        assert_eq!(
            sidecar_path(Path::new("/k/UTC--2026-07-18T00-00-00.0Z--dead")),
            Path::new("/k/UTC--2026-07-18T00-00-00.0Z--dead.touchid")
        );
    }

    /// Regression test: a sidecar symlink pointing at the keystore must not let
    /// the sidecar write follow it and destroy the keystore.
    #[test]
    fn sidecar_write_replaces_symlink_instead_of_following_it() {
        let dir = tempfile::tempdir().unwrap();
        let keystore = dir.path().join("deployer");
        fs::write(&keystore, "{}").unwrap();
        std::os::unix::fs::symlink(&keystore, sidecar_path(&keystore)).unwrap();

        let sidecar = Sidecar {
            version: SIDECAR_VERSION,
            policy: Policy::UserPresence,
            se_key: String::new(),
            sealed_password: String::new(),
        };
        write_sidecar(&keystore, &sidecar).unwrap();

        // The keystore is untouched, and the sidecar path now holds a regular
        // 0600 file rather than the symlink.
        assert_eq!(fs::read_to_string(&keystore).unwrap(), "{}");
        let meta = fs::symlink_metadata(sidecar_path(&keystore)).unwrap();
        assert!(meta.is_file());
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    }

    /// Re-enrollment must tighten a sidecar whose mode was loosened.
    #[test]
    fn sidecar_write_repairs_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let keystore = dir.path().join("deployer");
        fs::write(&keystore, "{}").unwrap();
        let sidecar = Sidecar {
            version: SIDECAR_VERSION,
            policy: Policy::UserPresence,
            se_key: String::new(),
            sealed_password: String::new(),
        };
        write_sidecar(&keystore, &sidecar).unwrap();
        fs::set_permissions(sidecar_path(&keystore), fs::Permissions::from_mode(0o644)).unwrap();

        write_sidecar(&keystore, &sidecar).unwrap();
        let meta = fs::metadata(sidecar_path(&keystore)).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn enroll_and_unwrap_device_only_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let keystore = dir.path().join("deployer");
        fs::write(&keystore, "{}").unwrap();

        assert!(!is_enrolled(&keystore));
        assert!(!remove(&keystore).unwrap());
        assert!(matches!(unwrap_password(&keystore), Err(TouchIdError::NotEnrolled)));

        // DeviceOnly avoids a user-interaction prompt, exercising the full
        // create/wrap/unwrap FFI path non-interactively.
        match enroll(&keystore, "hunter2", Policy::DeviceOnly) {
            Ok(()) => {}
            // VMs and CI runners have no usable Secure Enclave; require the
            // hardware path only when explicitly opted in.
            Err(TouchIdError::Unavailable(e) | TouchIdError::SecureEnclave(e))
                if std::env::var_os("FOUNDRY_TOUCH_ID_TESTS").is_none() =>
            {
                eprintln!("skipping Secure Enclave roundtrip: {e}");
                return;
            }
            Err(e) => panic!("enroll failed: {e}"),
        }
        assert!(is_enrolled(&keystore));
        assert_eq!(*unwrap_password(&keystore).unwrap(), "hunter2");

        assert!(remove(&keystore).unwrap());
        assert!(!is_enrolled(&keystore));
    }

    #[test]
    fn unsupported_sidecar_version_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let keystore = dir.path().join("deployer");
        fs::write(&keystore, "{}").unwrap();
        // A future format: bumped version with a policy this build doesn't know.
        fs::write(
            sidecar_path(&keystore),
            r#"{"version":2,"policy":"watch","se_key":"","sealed_password":""}"#,
        )
        .unwrap();
        assert!(matches!(unwrap_password(&keystore), Err(TouchIdError::UnsupportedVersion(2))));
    }

    /// Requires a Touch ID prompt; run manually:
    /// `cargo test -p foundry-wallets --features touch-id -- --ignored touch_id_interactive`
    #[test]
    #[ignore = "requires Touch ID user interaction"]
    fn touch_id_interactive_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let keystore = dir.path().join("deployer");
        fs::write(&keystore, "{}").unwrap();
        enroll(&keystore, "hunter2", Policy::UserPresence).unwrap();
        assert_eq!(*unwrap_password(&keystore).unwrap(), "hunter2");
    }
}
