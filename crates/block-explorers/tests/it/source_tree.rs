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
        let tree = metadata.source_tree().unwrap();
        let expected = format!("source path from explorer traverses a symlink: {link:?}");
        assert_eq!(tree.write_to(&output).unwrap_err().to_string(), expected);
        for mode in [ExportMode::CreateNew, ExportMode::Overwrite] {
            assert_eq!(tree.export_to(&output, mode).unwrap_err().to_string(), expected);
        }
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
fn export_creates_sources_and_only_overwrites_on_request() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let original = [0xff, 0, 1];
    fs::write(dir.join("notes"), original).unwrap();
    let tree = metadata("C", &[("A", "first"), ("Pair", "raw"), ("Pair.sol", "companion")])
        .source_tree()
        .unwrap();
    tree.export_to(dir, ExportMode::CreateNew).unwrap();
    assert_eq!(fs::read(dir.join("C/A.sol")).unwrap(), b"first");
    assert_eq!(fs::read(dir.join("C/Pair")).unwrap(), b"raw");
    assert_eq!(fs::read(dir.join("C/Pair.sol")).unwrap(), b"companion");

    let update = source_tree([("New/B.sol", "new"), ("C/A", "replacement")]);
    let existing = dir.join("C/A.sol");
    fs::write(&existing, original).unwrap();
    let error = update.export_to(dir, ExportMode::default()).unwrap_err();
    assert_eq!(error.to_string(), format!("source file already exists: {existing:?}"));
    assert_eq!(fs::read(&existing).unwrap(), original);
    assert!(!dir.join("New").exists());

    update.export_to(dir, ExportMode::Overwrite).unwrap();
    assert_eq!(fs::read(existing).unwrap(), b"replacement");
    assert_eq!(fs::read(dir.join("New/B.sol")).unwrap(), b"new");
    let notes = dir.join("notes");
    assert_eq!(fs::read(&notes).unwrap(), original);
    #[cfg(unix)]
    for new_file in ["C/Pair.sol", "New/B.sol"] {
        assert_eq!(access_mode(&dir.join(new_file)), access_mode(&notes));
    }
}

#[test]
fn export_preflights_duplicate_names_and_file_directory_conflicts() {
    for (mode, first, last, conflict) in [
        (ExportMode::Overwrite, "../C/Pair", "C/./Pair", "C/Pair.sol"),
        (ExportMode::CreateNew, "C/Pair.sol", "C/Pair.sol/Nested.sol", "C/Pair.sol/Nested.sol"),
        (ExportMode::Overwrite, "C/Pair.sol/Nested.sol", "C/Pair.sol", "C/Pair.sol"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("new/project");
        let error = source_tree([("First.sol", "first"), (first, "one"), (last, "two")])
            .export_to(&dir, mode)
            .unwrap_err();
        assert!(matches!(error, ExportError::Conflict(path) if path == dir.join(conflict)));
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    }
}

#[test]
fn export_respects_the_destination_filesystems_case_sensitivity() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    fs::create_dir(dir.join("Sources")).unwrap();
    let tree = source_tree([
        ("First.sol", "first"),
        ("Sources/New/A.sol", "one"),
        ("sources/new/a.sol", "two"),
    ]);
    let result = tree.export_to(dir, ExportMode::CreateNew);
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

#[test]
fn export_checks_existing_file_types_before_publication() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let directory = root.join("Directory.sol");
    let file = root.join("File.sol");
    fs::create_dir(&directory).unwrap();
    fs::write(&file, b"original").unwrap();
    for (mode, path) in [
        (ExportMode::CreateNew, "Directory.sol"),
        (ExportMode::Overwrite, "Directory.sol"),
        (ExportMode::Overwrite, "File.sol/Child.sol"),
    ] {
        let error = preflight_error(root, path, mode);
        assert!(matches!(error, ExportError::Conflict(target) if target == root.join(path)));
    }
    assert_eq!(fs::read(&file).unwrap(), b"original");
    assert!(directory.is_dir());
    assert_eq!(fs::read_dir(root).unwrap().count(), 2);
    let error = preflight_error(&file, "Child.sol", ExportMode::CreateNew);
    assert_eq!(error.to_string(), format!("conflicting source destination: {file:?}"));
}

#[cfg(unix)]
#[test]
fn export_follows_a_directory_symlink_at_the_root() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("project");
    let link = temp.path().join("link");
    fs::create_dir(&target).unwrap();
    symlink(&target, &link).unwrap();

    for (root, mode, contents) in [
        (link.clone(), ExportMode::CreateNew, "first"),
        (link.join(""), ExportMode::Overwrite, "replacement"),
    ] {
        source_tree([("C/A.sol", contents)]).export_to(&root, mode).unwrap();
        assert_eq!(fs::read_to_string(target.join("C/A.sol")).unwrap(), contents);
    }
}

#[cfg(unix)]
#[test]
fn export_rejects_symlinks_and_special_files_before_publication() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let link = dir.join("Dangling.sol");
    symlink("missing", &link).unwrap();
    let error = preflight_error(dir, "Dangling.sol", ExportMode::CreateNew);
    assert!(
        matches!(error, ExportError::Source(EtherscanError::SourcePathSymlink(p)) if p == link)
    );

    let error = preflight_error(&link.join(""), "Child.sol", ExportMode::CreateNew);
    assert!(matches!(error, ExportError::Io { path, source }
        if path == link && source.kind() == std::io::ErrorKind::NotFound));

    fs::write(dir.join("missing"), "original").unwrap();
    let error = preflight_error(&link, "Child.sol", ExportMode::CreateNew);
    assert!(matches!(error, ExportError::Conflict(path) if path == link));

    let socket = dir.join("Socket.sol");
    let _listener = UnixListener::bind(&socket).unwrap();
    let error = preflight_error(dir, "Socket.sol", ExportMode::Overwrite);
    assert!(matches!(error, ExportError::Conflict(path) if path == socket));
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
                let tree = source_tree([("C/A.sol", contents)]);
                start.wait();
                tree.export_to(&dir, ExportMode::CreateNew)
            })
        });
        writers.map(|writer| writer.join().unwrap())
    });
    let path = dir.join("C/A.sol");
    let (winner, error) = match results {
        [Ok(()), Err(error)] => (&contents[0], error),
        [Err(error), Ok(())] => (&contents[1], error),
        results => panic!("expected exactly one successful export: {results:?}"),
    };
    assert!(matches!(error, ExportError::AlreadyExists(existing) if existing == path));
    assert_eq!(&fs::read_to_string(path).unwrap(), winner);
}

fn source_tree<const N: usize>(sources: [(&str, &str); N]) -> SourceTree {
    let entries = sources
        .map(|(path, contents)| SourceTreeEntry { path: path.into(), contents: contents.into() });
    SourceTree { entries: entries.into() }
}

#[cfg(unix)]
#[test]
fn overwrite_preserves_existing_access_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let files = [
        ("Private.sol", 0o600),
        ("Shared.sol", 0o640),
        ("nested/Executable.sol", 0o755),
        ("Last.sol", 0o644),
    ];
    fs::create_dir(dir.path().join("nested")).unwrap();
    for (name, mode) in files {
        let path = dir.path().join(name);
        fs::write(&path, "original").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }

    let tree = source_tree(files.map(|(name, _)| (name, name)));
    tree.export_to(dir.path(), ExportMode::Overwrite).unwrap();
    for (name, mode) in files {
        let path = dir.path().join(name);
        assert_eq!(access_mode(&path), mode);
        assert_eq!(fs::read_to_string(path).unwrap(), name);
    }
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 4);
    assert_eq!(fs::read_dir(dir.path().join("nested")).unwrap().count(), 1);
}

#[test]
fn export_preflights_roots_with_parent_components() {
    let tree =
        source_tree([("New.sol", "new"), ("First.sol", "replacement"), ("Later.sol", "last")]);
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    fs::create_dir_all(project.join("Later.sol")).unwrap();
    fs::write(project.join("First.sol"), "original").unwrap();
    let build = temp.path().join("build");
    let root = build.join("../project");

    tree.export_to(&root, ExportMode::Overwrite).unwrap_err();
    assert_eq!(fs::read(project.join("First.sol")).unwrap(), b"original");
    assert!(!project.join("New.sol").exists());
    assert!(!build.exists());

    fs::create_dir(&build).unwrap();
    tree.export_to(&root.join("fresh"), ExportMode::CreateNew).unwrap();
    assert_eq!(fs::read(project.join("fresh/First.sol")).unwrap(), b"replacement");
}

#[test]
fn export_metadata_errors_report_the_destination() {
    let dir = tempfile::tempdir().unwrap();
    let name = "invalid\0name.sol";
    let error = preflight_error(dir.path(), name, ExportMode::CreateNew);
    assert!(matches!(error, ExportError::Io { path, source }
        if path == dir.path().join(name) && source.kind() == std::io::ErrorKind::InvalidInput));
}

fn preflight_error(dir: &Path, path: &str, mode: ExportMode) -> ExportError {
    let tree = source_tree([("First.sol", "first"), (path, "source")]);
    let error = tree.export_to(dir, mode).unwrap_err();
    assert!(!dir.join("First.sol").exists());
    error
}

#[cfg(unix)]
fn access_mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}
