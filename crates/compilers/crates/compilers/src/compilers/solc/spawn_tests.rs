use super::*;
use std::{fs, os::unix::fs::PermissionsExt, sync::Barrier, thread, time::Duration};

struct BusySolc {
    _dir: tempfile::TempDir,
    solc: Solc,
    writer: fs::File,
}

impl BusySolc {
    fn new() -> Self {
        Self::with_contents(b"#!/bin/sh\ncase \"$*\" in\n*--version*) echo 'Version: 0.8.18+commit.87f61d96.Linux.g++';;\n*) input=$(cat); printf '%s' \"$input\";;\nesac\n")
    }

    fn with_contents(contents: &[u8]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("solc");
        let (mut file, temp_path) =
            tempfile::NamedTempFile::new_in(dir.path()).unwrap().into_parts();
        file.write_all(contents).unwrap();
        file.set_permissions(fs::Permissions::from_mode(0o755)).unwrap();
        // Model a descriptor inherited by a fork: closing the publisher's copy before the
        // rename is insufficient while another process still holds a writable copy.
        let writer = file.try_clone().unwrap();
        drop(file);
        temp_path.persist(&path).unwrap();
        let error = Command::new(&path).spawn().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::ExecutableFileBusy);
        Self { _dir: dir, solc: Solc::new_with_version(path, Version::new(0, 8, 18)), writer }
    }
}

#[test]
fn version_recovers_from_busy_executable() {
    let BusySolc { _dir, solc, writer } = BusySolc::new();
    let release = thread::spawn(|| {
        thread::sleep(Duration::from_millis(50));
        drop(writer);
    });
    let result = Solc::version_with_args(&solc.solc, &["--extra-argument".into()]);
    release.join().unwrap();
    assert_eq!(result.unwrap(), "0.8.18+commit.87f61d96.Linux.gcc".parse().unwrap());
}

#[test]
fn compile_recovers_from_busy_executable() {
    let BusySolc { _dir, solc, writer } = BusySolc::new();
    let release = thread::spawn(|| {
        thread::sleep(Duration::from_millis(50));
        drop(writer);
    });
    let input = serde_json::json!({"large_input": "x".repeat(256 * 1024)});
    let result = solc.compile_output(&input);
    release.join().unwrap();
    assert_eq!(result.unwrap(), serde_json::to_vec(&input).unwrap());
}

#[cfg(feature = "async")]
#[tokio::test]
async fn async_version_recovers_from_busy_executable() {
    let BusySolc { _dir, solc, writer } = BusySolc::new();
    let release = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        drop(writer);
    });
    let result = Solc::async_version(&solc.solc).await;
    release.await.unwrap();
    assert_eq!(result.unwrap(), "0.8.18+commit.87f61d96.Linux.gcc".parse().unwrap());
}

#[cfg(feature = "async")]
#[tokio::test]
async fn async_compile_recovers_from_busy_executable() {
    let BusySolc { _dir, solc, writer } = BusySolc::new();
    let release = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        drop(writer);
    });
    let input = serde_json::json!({"large_input": "x".repeat(256 * 1024)});
    let result = solc.async_compile_output(&input).await;
    release.await.unwrap();
    assert_eq!(result.unwrap(), serde_json::to_vec(&input).unwrap());
}

#[cfg(feature = "async")]
#[test]
fn async_version_recovers_without_timer_driver() {
    let BusySolc { _dir, solc, writer } = BusySolc::new();
    let runtime = tokio::runtime::Builder::new_current_thread().enable_io().build().unwrap();
    runtime.block_on(async {
        // On a single-threaded runtime this task can release the writer only if the
        // compiler yields while retrying. No timer driver is enabled or required.
        let release = tokio::spawn(async move {
            tokio::task::yield_now().await;
            drop(writer);
        });
        let result = Solc::async_version(&solc.solc).await;
        release.await.unwrap();
        assert_eq!(result.unwrap(), "0.8.18+commit.87f61d96.Linux.gcc".parse().unwrap());
    });
}

#[cfg(feature = "async")]
#[test]
fn async_compile_recovers_without_timer_driver() {
    let BusySolc { _dir, solc, writer } = BusySolc::new();
    let runtime = tokio::runtime::Builder::new_current_thread().enable_io().build().unwrap();
    runtime.block_on(async {
        let release = tokio::spawn(async move {
            tokio::task::yield_now().await;
            drop(writer);
        });
        let input = serde_json::json!({"large_input": "x".repeat(256 * 1024)});
        let result = solc.async_compile_output(&input).await;
        release.await.unwrap();
        assert_eq!(result.unwrap(), serde_json::to_vec(&input).unwrap());
    });
}

#[cfg(feature = "async")]
#[test]
fn async_version_recovers_with_saturated_blocking_pool() {
    let BusySolc { _dir, solc, writer } = BusySolc::new();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .max_blocking_threads(1)
        .enable_io()
        .build()
        .unwrap();
    let handle = runtime.handle().clone();
    let (completed, result) = std::sync::mpsc::channel();
    runtime.spawn_blocking(move || {
        let result = handle.block_on(async {
            let version = Solc::async_version(&solc.solc);
            tokio::pin!(version);
            // Keep the file busy until the first spawn has entered its backoff.
            assert!(futures_util::poll!(&mut version).is_pending());
            drop(writer);
            version.await
        });
        let _ = completed.send(result);
    });
    let result = result.recv_timeout(Duration::from_secs(3));
    // A regression must fail instead of hanging while dropping the runtime.
    runtime.shutdown_timeout(Duration::from_millis(100));
    assert_eq!(
        result.expect("compiler retry deadlocked on the blocking pool").unwrap(),
        "0.8.18+commit.87f61d96.Linux.gcc".parse().unwrap()
    );
}

#[test]
fn compiler_failure_is_not_retried() {
    let BusySolc { _dir, solc, writer } = BusySolc::with_contents(
        b"#!/bin/sh\ncat >/dev/null\nprintf x >> \"$0.runs\"\necho 'compiler failed' >&2\nexit 1\n",
    );
    drop(writer);
    let result = solc.compile_output(&serde_json::json!({}));
    assert!(matches!(result, Err(SolcError::SolcError(..))));
    assert_eq!(fs::read(solc.solc.with_extension("runs")).unwrap(), b"x");
}

#[cfg(feature = "async")]
#[tokio::test]
async fn async_compiler_failure_is_not_retried() {
    let BusySolc { _dir, solc, writer } = BusySolc::with_contents(
        b"#!/bin/sh\ncat >/dev/null\nprintf x >> \"$0.runs\"\necho 'compiler failed' >&2\nexit 1\n",
    );
    drop(writer);
    let result = solc.async_compile_output(&serde_json::json!({})).await;
    assert!(matches!(result, Err(SolcError::SolcError(..))));
    assert_eq!(fs::read(solc.solc.with_extension("runs")).unwrap(), b"x");
}

#[test]
fn concurrent_publish_and_spawn() {
    let BusySolc { _dir, solc, writer } = BusySolc::new();
    drop(writer);
    let bytes = fs::read(&solc.solc).unwrap();
    let barrier = Barrier::new(5);
    let input = serde_json::json!({"input": 42});
    let expected = serde_json::to_vec(&input).unwrap();
    thread::scope(|scope| {
        scope.spawn(|| {
            barrier.wait();
            for _ in 0..1000 {
                let (mut file, path) =
                    tempfile::NamedTempFile::new_in(_dir.path()).unwrap().into_parts();
                file.write_all(&bytes).unwrap();
                file.set_permissions(fs::Permissions::from_mode(0o755)).unwrap();
                path.persist(&solc.solc).unwrap();
                // Positive control: the unguarded spawn must fail in the publication window.
                assert_eq!(
                    Command::new(&solc.solc).spawn().unwrap_err().kind(),
                    io::ErrorKind::ExecutableFileBusy
                );
                drop(file);
            }
        });
        for _ in 0..4 {
            scope.spawn(|| {
                barrier.wait();
                for _ in 0..250 {
                    assert_eq!(
                        Solc::version_impl(&solc.solc, &[]).unwrap(),
                        "0.8.18+commit.87f61d96.Linux.gcc".parse().unwrap()
                    );
                    assert_eq!(solc.compile_output(&input).unwrap(), expected);
                }
            });
        }
    });
}

#[cfg(feature = "async")]
#[tokio::test]
#[ignore = "requires SVM_STRESS_SOLC pointing to solc 0.8.18"]
async fn real_solc_recovers_from_busy_executable() {
    let bytes =
        fs::read(std::env::var_os("SVM_STRESS_SOLC").expect("set SVM_STRESS_SOLC")).unwrap();
    let input = serde_json::json!({
        "language": "Solidity",
        "sources": {"Test.sol": {"content": "pragma solidity =0.8.18; contract Test { function value() external pure returns (uint256) { return 42; } }"}},
        "settings": {"outputSelection": {"*": {"*": ["abi"]}}}
    });
    for asynchronous in [false, true] {
        for version_only in [false, true] {
            let BusySolc { _dir, solc, writer } = BusySolc::with_contents(&bytes);
            let release = thread::spawn(|| {
                thread::sleep(Duration::from_millis(50));
                drop(writer);
            });
            if version_only {
                let version = if asynchronous {
                    Solc::async_version(&solc.solc).await.unwrap()
                } else {
                    Solc::version(&solc.solc).unwrap()
                };
                assert_eq!((version.major, version.minor, version.patch), (0, 8, 18));
            } else {
                let output = if asynchronous {
                    solc.async_compile_output(&input).await.unwrap()
                } else {
                    solc.compile_output(&input).unwrap()
                };
                let output: serde_json::Value = serde_json::from_slice(&output).unwrap();
                assert_eq!(output["contracts"]["Test.sol"]["Test"]["abi"][0]["name"], "value");
            }
            release.join().unwrap();
        }
    }
}
