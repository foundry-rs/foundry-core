use super::*;
use foundry_block_explorers::{
    contract::{ContractMetadata, Metadata},
    source_tree::{ExportError, ExportMode, SourceTree, SourceTreeEntry},
};
use std::{fs, path::Path, sync::Barrier, thread};

#[cfg(unix)]
use std::os::unix::{
    fs::{PermissionsExt, symlink},
    net::UnixListener,
};

#[test]
fn metadata_rejects_hidden_contract_roots() {
    for name in ["", ".", "..", ".git", "a/b", "/abs"] {
        let metadata = metadata(name, &[("config", "replacement"), ("config.sol", "companion")]);
        let error = metadata.source_tree().unwrap_err();
        assert!(matches!(&error, EtherscanError::InvalidContractName(_)));
        assert_eq!(error.to_string(), format!("invalid contract name from explorer: {name:?}"));
    }
}

#[test]
fn metadata_rejects_empty_sanitized_sources() {
    for path in ["", ".", "..", "/", "../../"] {
        let metadata = ContractMetadata {
            items: vec![
                metadata("Valid", &[("Contract.sol", "valid")]),
                metadata("foundry.toml", &[(path, "replacement")]),
            ],
        };
        let error = metadata.source_tree().unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("invalid source path from explorer: {:?}", Path::new("foundry.toml").join(""))
        );
    }
}

#[test]
fn metadata_preserves_normal_names_sanitization_and_overwrites() {
    for name in [
        "Example",
        "Contract_1",
        "Ünïcode",
        "foundry.toml",
        "CON",
        "Example.",
        "Example ",
        "\u{200c}.git",
    ] {
        let entries = metadata(name, &[("Contract.sol", "source")]).source_entries().unwrap();
        assert_eq!(entries[0].path, Path::new(name).join("Contract.sol"));
    }

    let temp = tempfile::tempdir().unwrap();
    let tree = metadata(
        "Example",
        &[
            ("/contracts/Z.sol", "contract Z {}"),
            ("../../A.sol", "contract A {}"),
            ("README", "readme"),
            ("Pair", "extensionless"),
            ("Pair.sol", "companion"),
        ],
    )
    .source_tree()
    .unwrap();
    tree.write_to(temp.path()).unwrap();
    for (path, contents) in [
        ("contracts/Z.sol", "contract Z {}"),
        ("A.sol", "contract A {}"),
        ("README.sol", "readme"),
        ("Pair", "extensionless"),
        ("Pair.sol", "companion"),
    ] {
        assert_eq!(
            std::fs::read_to_string(temp.path().join("Example").join(path)).unwrap(),
            contents
        );
    }
    metadata("Example", &[("A.sol", "replacement")])
        .source_tree()
        .unwrap()
        .write_to(temp.path())
        .unwrap();
    assert_eq!(std::fs::read_to_string(temp.path().join("Example/A.sol")).unwrap(), "replacement");
}

#[cfg(not(windows))]
#[test]
fn metadata_preserves_tilde_in_contract_names_on_non_windows() {
    assert!(metadata("git~1", &[("Contract.sol", "source")]).source_entries().is_ok());
}

#[cfg(unix)]
#[test]
fn source_tree_checks_final_symlink_destinations_before_any_writes() {
    for (link, source, companion, target) in [
        ("C", "config", true, ".git"),
        ("C/.git", ".git/config", true, "../.git"),
        ("C/config", "config", true, "../.git/config"),
        ("C/config.sol", "config", false, "../.git/config"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("sources");
        let git = output.join(".git");
        std::fs::create_dir_all(&git).unwrap();
        let config = git.join("config");
        std::fs::write(&config, "original").unwrap();
        let link = output.join(link);
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        symlink(target, &link).unwrap();
        let mut sources = vec![(source, "replacement")];
        let companion_path = format!("{source}.sol");
        if companion {
            sources.push((&companion_path, "companion"));
        }
        let metadata = ContractMetadata {
            items: vec![metadata("Safe", &[("Contract.sol", "valid")]), metadata("C", &sources)],
        };
        let error = metadata.source_tree().unwrap().write_to(&output).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("source path from explorer traverses a symlink: {link:?}")
        );
        assert_eq!(std::fs::read_to_string(config).unwrap(), "original");
        assert!(!output.join("Safe").exists());
    }
}

#[test]
fn source_tree_rejects_hidden_roots_before_any_writes() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("sources");
    let tree = SourceTree {
        entries: vec![
            SourceTreeEntry { path: "Safe/Contract.sol".into(), contents: "valid".into() },
            SourceTreeEntry { path: ".git/config".into(), contents: "replacement".into() },
            SourceTreeEntry { path: ".git/config.sol".into(), contents: "companion".into() },
        ],
    };
    let error = tree.write_to(&output).unwrap_err();
    assert_eq!(error.to_string(), "invalid contract name from explorer: \".git\"");
    assert!(!output.exists());
}

#[test]
fn source_tree_preserves_sanitization_companions_and_nested_hidden_paths() {
    let temp = tempfile::tempdir().unwrap();
    let entries = [
        ("Single.sol", "single file"),
        (".env.sol", "hidden file"),
        ("C/.git/config", "nested config"),
        ("C/.git/config.sol", "companion"),
        ("C/README", "readme"),
        ("C/Duplicate.sol", "first"),
        ("C/Duplicate.sol", "last"),
        ("../Raw/Pair", "raw extensionless"),
        ("../Raw/Pair.sol", "raw companion"),
        ("C/Trailing.sol/", "trailing separator"),
        ("C/Dot.sol/.", "trailing dot"),
    ]
    .map(|(path, contents)| SourceTreeEntry { path: path.into(), contents: contents.into() });
    SourceTree { entries: entries.into() }.write_to(temp.path()).unwrap();
    for (path, expected) in [
        ("Single.sol", "single file"),
        (".env.sol", "hidden file"),
        ("C/.git/config", "nested config"),
        ("C/.git/config.sol", "companion"),
        ("C/README.sol", "readme"),
        ("C/Duplicate.sol", "last"),
        ("Raw/Pair.sol", "raw companion"),
        ("C/Trailing.sol", "trailing separator"),
        ("C/Dot.sol", "trailing dot"),
    ] {
        assert_eq!(std::fs::read_to_string(temp.path().join(path)).unwrap(), expected);
    }
    assert!(!temp.path().join("Raw/Pair").exists());
}

#[test]
fn source_tree_rejects_empty_destinations_before_creating_output() {
    let temp = tempfile::tempdir().unwrap();
    for path in ["", ".", "..", "/"] {
        let output = temp.path().join("sources");
        let tree = SourceTree {
            entries: vec![SourceTreeEntry { path: path.into(), contents: "replacement".into() }],
        };
        let error = tree.write_to(&output).unwrap_err();
        assert_eq!(error.to_string(), "invalid source path from explorer: \"\"");
        assert!(!output.exists());
    }
}

#[cfg(windows)]
#[test]
fn metadata_rejects_windows_path_syntax() {
    for name in [".. ", ". ", "safe:stream", r"C:relative", r"a\b", "git~1", "VSCODE~1"] {
        let error = metadata(name, &[("Contract.sol", "replacement")]).source_tree().unwrap_err();
        assert_eq!(error.to_string(), format!("invalid contract name from explorer: {name:?}"));
    }
    for path in ["a/C:/foundry.toml", "lib/x:y.sol"] {
        assert!(metadata("C", &[(path, "replacement")]).source_entries().is_err());
    }
    // Device names are left to the operating system rather than a source-name blacklist.
    assert!(metadata("CON", &[("Contract.sol", "source")]).source_entries().is_ok());
}

#[cfg(windows)]
#[test]
fn source_tree_rejects_windows_path_syntax_before_any_writes() {
    let temp = tempfile::tempdir().unwrap();
    for path in ["C/config:stream.sol", "C/nested/D:relative.sol"] {
        let output = temp.path().join("sources");
        let tree = SourceTree {
            entries: vec![
                SourceTreeEntry { path: "Safe/Contract.sol".into(), contents: "valid".into() },
                SourceTreeEntry { path: path.into(), contents: "replacement".into() },
            ],
        };
        assert!(matches!(tree.write_to(&output), Err(EtherscanError::InvalidSourcePath(_))));
        assert!(!output.exists());
    }
}

fn metadata(name: &str, sources: &[(&str, &str)]) -> Metadata {
    serde_json::from_value(serde_json::json!({
        "SourceCode": sources.iter().map(|(path, content)| {
            (path.to_string(), serde_json::json!({ "content": content }))
        }).collect::<serde_json::Map<_, _>>(),
        "ABI": "[]",
        "ContractName": name,
        "CompilerVersion": "v0.8.26+commit.8a97fa7a",
        "OptimizationUsed": "0",
        "Runs": "0",
        "EVMVersion": "Default",
        "Proxy": "0"
    }))
    .unwrap()
}

#[test]
fn export_preserves_existing_files_before_writing_any_sources() {
    let temp = tempfile::tempdir().unwrap();
    let existing = temp.path().join("Existing.sol");
    let original = [0xff, 0, 1];
    fs::write(&existing, original).unwrap();
    let tree = source_tree(&[("New/First.sol", "new"), ("Existing", "replacement")]);

    let error = tree.export_to(temp.path(), ExportMode::default()).unwrap_err();
    assert_eq!(error.to_string(), format!("source file already exists: {existing:?}"));
    assert_eq!(fs::read(existing).unwrap(), original);
    assert!(!temp.path().join("New").exists());
}

#[test]
fn export_creates_sources_and_explicitly_replaces_regular_files() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let contents = [0, 0xff, 1];
    let tree = metadata("C", &[("A", "first"), ("Pair", "raw"), ("Pair.sol", "companion")])
        .source_tree()
        .unwrap();

    tree.export_to(&project, ExportMode::CreateNew).unwrap();
    assert_eq!(fs::read(project.join("C/A.sol")).unwrap(), b"first");
    assert_eq!(fs::read(project.join("C/Pair")).unwrap(), b"raw");
    assert_eq!(fs::read(project.join("C/Pair.sol")).unwrap(), b"companion");
    fs::write(project.join("notes"), contents).unwrap();
    source_tree(&[("C/New.sol", "new")]).export_to(&project, ExportMode::CreateNew).unwrap();
    source_tree(&[("C/A.sol", "replacement"), ("D/B.sol", "created")])
        .export_to(&project, ExportMode::Overwrite)
        .unwrap();
    assert_eq!(fs::read(project.join("C/New.sol")).unwrap(), b"new");
    assert_eq!(fs::read(project.join("C/A.sol")).unwrap(), b"replacement");
    assert_eq!(fs::read(project.join("D/B.sol")).unwrap(), b"created");
    assert_eq!(fs::read(project.join("notes")).unwrap(), contents);
}

#[test]
fn export_preflights_duplicate_names_and_file_directory_conflicts() {
    for mode in [ExportMode::CreateNew, ExportMode::Overwrite] {
        for (first, last, conflict) in [
            ("../C/Pair", "C/./Pair", "C/Pair.sol"),
            ("C/Pair.sol", "C/Pair.sol/Nested.sol", "C/Pair.sol/Nested.sol"),
            ("C/Pair.sol/Nested.sol", "C/Pair.sol", "C/Pair.sol"),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let dir = temp.path().join("new/project");
            let error = source_tree(&[("First.sol", "first"), (first, "one"), (last, "two")])
                .export_to(&dir, mode)
                .unwrap_err();
            assert!(matches!(error, ExportError::Conflict(path) if path == dir.join(conflict)));
            assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
        }
    }
}

#[test]
fn export_respects_the_destination_filesystems_case_sensitivity() {
    for mode in [ExportMode::CreateNew, ExportMode::Overwrite] {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();
        fs::create_dir(dir.join("Sources")).unwrap();
        let tree = source_tree(&[
            ("First.sol", "first"),
            ("Sources/New/A.sol", "one"),
            ("sources/new/a.sol", "two"),
        ]);
        let result = tree.export_to(dir, mode);
        if dir.join("SOURCES").exists() {
            assert!(
                matches!(result, Err(ExportError::Conflict(path)) if path == dir.join("sources/new/a.sol"))
            );
            assert_eq!(fs::read_dir(dir).unwrap().count(), 1);
            assert_eq!(fs::read_dir(dir.join("Sources")).unwrap().count(), 0);
        } else {
            result.unwrap();
            assert_eq!(fs::read(dir.join("Sources/New/A.sol")).unwrap(), b"one");
            assert_eq!(fs::read(dir.join("sources/new/a.sol")).unwrap(), b"two");
        }
    }
}

#[test]
fn export_checks_existing_file_types_before_publication() {
    for mode in [ExportMode::CreateNew, ExportMode::Overwrite] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let directory = root.join("Directory.sol");
        let file = root.join("File.sol");
        fs::create_dir(&directory).unwrap();
        fs::write(&file, b"original").unwrap();
        for path in ["Directory.sol", "File.sol/Child.sol"] {
            let error = source_tree(&[("First.sol", "first"), (path, "source")])
                .export_to(root, mode)
                .unwrap_err();
            assert!(matches!(error, ExportError::Conflict(target) if target == root.join(path)));
            assert_eq!(fs::read_dir(root).unwrap().count(), 2);
            assert_eq!(fs::read(&file).unwrap(), b"original");
            assert!(directory.is_dir());
        }
        let error = source_tree(&[("First.sol", "first")]).export_to(&file, mode).unwrap_err();
        assert_eq!(error.to_string(), format!("conflicting source destination: {file:?}"));
    }
}

#[cfg(unix)]
#[test]
fn export_rejects_symlinks_and_special_files_before_publication() {
    for mode in [ExportMode::CreateNew, ExportMode::Overwrite] {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();
        fs::write(dir.join("target"), "original").unwrap();
        symlink("target", dir.join("Link.sol")).unwrap();
        symlink("missing", dir.join("Dangling.sol")).unwrap();
        symlink(".", dir.join("Directory")).unwrap();
        let socket = dir.join("Socket.sol");
        let _listener = UnixListener::bind(&socket).unwrap();
        for (path, link) in [
            ("Link.sol", "Link.sol"),
            ("Dangling.sol", "Dangling.sol"),
            ("Directory/Child.sol", "Directory"),
        ] {
            let error = source_tree(&[("First.sol", "first"), (path, "source")])
                .export_to(dir, mode)
                .unwrap_err();
            let link = dir.join(link);
            assert_eq!(
                error.to_string(),
                format!("source path from explorer traverses a symlink: {link:?}")
            );
        }
        let root = dir.join("Directory");
        let error =
            source_tree(&[("First.sol", "first")]).export_to(&root.join(""), mode).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("source path from explorer traverses a symlink: {root:?}")
        );
        let error = source_tree(&[("First.sol", "first"), ("Socket.sol", "source")])
            .export_to(dir, mode)
            .unwrap_err();
        assert_eq!(error.to_string(), format!("conflicting source destination: {socket:?}"));
        assert_eq!(fs::read(dir.join("target")).unwrap(), b"original");
        assert!(!dir.join("First.sol").exists());
    }
}

#[test]
fn concurrent_create_only_exports_publish_one_complete_source() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("project");
    let start = Barrier::new(2);
    let contents = ["alpha".repeat(16384), "bravo".repeat(16384)];
    let results = thread::scope(|scope| {
        let writers = contents.each_ref().map(|contents| {
            scope.spawn(|| {
                let tree = source_tree(&[("C/A.sol", contents)]);
                start.wait();
                tree.export_to(&dir, ExportMode::CreateNew)
            })
        });
        writers.map(|writer| writer.join().unwrap())
    });
    let mut winners = 0;
    for (result, contents) in results.into_iter().zip(contents) {
        match result {
            Ok(()) => {
                winners += 1;
                assert_eq!(fs::read_to_string(dir.join("C/A.sol")).unwrap(), contents);
            }
            Err(ExportError::AlreadyExists(path)) => assert_eq!(path, dir.join("C/A.sol")),
            Err(error) => panic!("unexpected export error: {error}"),
        }
    }
    assert_eq!(winners, 1);
}

fn source_tree(entries: &[(&str, &str)]) -> SourceTree {
    SourceTree {
        entries: entries
            .iter()
            .map(|&(path, contents)| SourceTreeEntry {
                path: path.into(),
                contents: contents.into(),
            })
            .collect(),
    }
}

#[cfg(unix)]
#[test]
fn export_creates_files_with_normal_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let reference = dir.path().join("reference");
    fs::write(&reference, "").unwrap();
    let expected = fs::metadata(reference).unwrap().permissions().mode() & 0o777;
    for policy in [ExportMode::CreateNew, ExportMode::Overwrite] {
        let name = "New.sol";
        source_tree(&[(name, "new")]).export_to(dir.path(), policy).unwrap();
        let path = dir.path().join(name);
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, expected);
        fs::remove_file(path).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn overwrite_preserves_existing_access_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Existing.sol");
    fs::write(&path, "original").unwrap();
    let tree = source_tree(&[("Existing.sol", "replacement")]);
    for mode in [0o600, 0o640, 0o755] {
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        tree.export_to(dir.path(), ExportMode::Overwrite).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, mode);
        assert_eq!(fs::read_to_string(&path).unwrap(), "replacement");
    }
}

#[test]
fn export_preflights_roots_with_parent_components() {
    let tree =
        source_tree(&[("New.sol", "new"), ("First.sol", "replacement"), ("Later.sol", "last")]);
    for mode in [ExportMode::CreateNew, ExportMode::Overwrite] {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir_all(project.join("Later.sol")).unwrap();
        fs::write(project.join("First.sol"), "original").unwrap();
        let build = temp.path().join("build");
        let root = build.join("../project");

        tree.export_to(&root, mode).unwrap_err();
        assert_eq!(fs::read(project.join("First.sol")).unwrap(), b"original");
        assert!(!project.join("New.sol").exists());
        assert!(!build.exists());

        fs::create_dir(&build).unwrap();
        tree.export_to(&root.join("fresh"), mode).unwrap();
        assert_eq!(fs::read(project.join("fresh/First.sol")).unwrap(), b"replacement");
    }
}

#[test]
fn export_metadata_errors_report_the_destination() {
    let dir = tempfile::tempdir().unwrap();
    let name = "invalid\0name.sol";
    let tree = source_tree(&[("First.sol", "first"), (name, "invalid")]);
    let error = tree.export_to(dir.path(), ExportMode::CreateNew).unwrap_err();
    assert!(matches!(error, ExportError::Io { path, source }
        if path == dir.path().join(name) && source.kind() == std::io::ErrorKind::InvalidInput));
    assert!(!dir.path().join("First.sol").exists());
}
