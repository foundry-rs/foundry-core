use super::*;
use foundry_block_explorers::{
    contract::{ContractMetadata, Metadata},
    source_tree::{SourceTree, SourceTreeEntry},
};
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::symlink;

#[cfg(windows)]
use std::process::Command;

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
        "git~1",
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
    for name in [".. ", ". ", "safe:stream", r"C:relative", r"a\b"] {
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

#[cfg(windows)]
#[test]
#[ignore = "requires NTFS 8.3 name creation"]
fn source_tree_rejects_hidden_short_name_aliases() {
    let temp = tempfile::tempdir().unwrap();
    // Force a different short-name suffix when 8.3 names are enabled.
    std::fs::create_dir(temp.path().join("git~1")).unwrap();
    let git = temp.path().join(".git");
    std::fs::create_dir(&git).unwrap();
    let config = git.join("config");
    std::fs::write(&config, "original").unwrap();
    let output = Command::new("cmd")
        .args(["/d", "/c", "for %I in (.git) do @echo %~snxI"])
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let short_path = String::from_utf8(output.stdout).unwrap();
    let name = Path::new(short_path.trim()).file_name().unwrap().to_str().unwrap();
    assert!(!name.starts_with('.'), "enable NTFS 8.3 name creation to run this regression");
    assert!(!name.eq_ignore_ascii_case("git~1"), "expected a short-name alias beyond git~1");
    for name in [name.to_owned(), name.to_ascii_lowercase(), format!("{name}."), format!("{name} ")]
    {
        let tree = metadata(&name, &[("config", "replacement"), ("config.sol", "companion")])
            .source_tree()
            .unwrap();
        let error = tree.write_to(temp.path()).unwrap_err();
        assert_eq!(error.to_string(), "invalid contract name from explorer: \".git\"");
    }
    assert_eq!(std::fs::read_to_string(config).unwrap(), "original");
}

#[cfg(windows)]
#[test]
fn source_tree_reexports_existing_windows_directories() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("sources");
    let long_output = temp.path().join("segment".repeat(20)).join("segment".repeat(20));
    for output in [&output, &long_output] {
        std::fs::create_dir_all(output.join("Ünïcode")).unwrap();
        let verbatim = std::fs::canonicalize(output).unwrap();
        for output in [output.as_path(), verbatim.as_path()] {
            for name in ["ÜNÏCODE", "NewContract"] {
                for contents in ["first export", "repeat export"] {
                    metadata(name, &[("Contract.sol", contents)])
                        .source_tree()
                        .unwrap()
                        .write_to(output)
                        .unwrap();
                    assert_eq!(
                        std::fs::read_to_string(output.join(name).join("Contract.sol")).unwrap(),
                        contents
                    );
                }
            }
        }
    }
}

#[cfg(windows)]
#[test]
fn source_tree_rejects_windows_search_patterns_before_any_writes() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("Match")).unwrap();
    for name in ["M*", "Matc?", "Match\0ignored"] {
        let metadata = ContractMetadata {
            items: vec![
                metadata("Safe", &[("Contract.sol", "valid")]),
                metadata(name, &[("Contract.sol", "replacement")]),
            ],
        };
        assert!(metadata.source_tree().unwrap().write_to(temp.path()).is_err());
        assert!(!temp.path().join("Safe").exists());
        assert!(!temp.path().join("Match/Contract.sol").exists());
    }
}

#[cfg(windows)]
#[test]
#[ignore = "requires FOUNDRY_SOURCE_SMB_DIR on an SMB share with a traverse-only ancestor"]
fn source_tree_reexports_below_smb_traverse_only_ancestor() {
    let output = std::env::var_os("FOUNDRY_SOURCE_SMB_DIR").expect("set FOUNDRY_SOURCE_SMB_DIR");
    let output = Path::new(&output);
    let long_output = output.join("segment".repeat(20)).join("segment".repeat(20));
    for output in [output, long_output.as_path()] {
        let contract = output.join("Example");
        // The fixture must allow writing here without allowing queries of every ancestor.
        std::fs::create_dir_all(&contract).unwrap();
        std::fs::write(contract.join("Contract.sol"), "original").unwrap();
        for contents in ["first export", "repeat export"] {
            metadata("Example", &[("Contract.sol", contents)])
                .source_tree()
                .unwrap()
                .write_to(output)
                .unwrap();
            assert_eq!(std::fs::read_to_string(contract.join("Contract.sol")).unwrap(), contents);
        }
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
