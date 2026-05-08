use super::VyperLanguage;
use crate::{
    ProjectPathsConfig, SourceParser,
    compilers::{ParsedSource, vyper::VYPER_EXTENSIONS},
};
use foundry_compilers_core::{
    error::{Result, SolcError},
    utils::{RE_VYPER_VERSION, capture_outer_and_inner},
};
use semver::VersionReq;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use winnow::{
    ModalResult, Parser,
    ascii::space1,
    combinator::{alt, opt, preceded},
    token::{take_till, take_while},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VyperImport {
    pub level: usize,
    pub path: Option<String>,
    pub final_part: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct VyperParser {
    _inner: (),
}

impl SourceParser for VyperParser {
    type ParsedSource = VyperParsedSource;

    fn new(_config: &ProjectPathsConfig) -> Self {
        Self { _inner: () }
    }
}

#[derive(Clone, Debug)]
pub struct VyperParsedSource {
    path: PathBuf,
    version_req: Option<VersionReq>,
    imports: Vec<VyperImport>,
}

impl ParsedSource for VyperParsedSource {
    type Language = VyperLanguage;

    #[instrument(name = "VyperParsedSource::parse", skip_all)]
    fn parse(content: &str, file: &Path) -> Result<Self> {
        let version_req = capture_outer_and_inner(content, &RE_VYPER_VERSION, &["version"])
            .first()
            .and_then(|(_, cap)| match parse_vyper_version_req(cap.as_str()) {
                Ok(req) => Some(req),
                Err(err) => {
                    warn!(
                        file = %file.display(),
                        pragma = cap.as_str(),
                        error = %err,
                        "failed to parse Vyper `pragma version` requirement; \
                         continuing without a version constraint",
                    );
                    None
                }
            });

        let imports = parse_imports(content);

        let path = file.to_path_buf();

        Ok(Self { path, version_req, imports })
    }

    fn version_req(&self) -> Option<&VersionReq> {
        self.version_req.as_ref()
    }

    fn contract_names(&self) -> &[String] {
        &[]
    }

    fn language(&self) -> Self::Language {
        VyperLanguage
    }

    fn resolve_imports<C>(
        &self,
        paths: &ProjectPathsConfig<C>,
        include_paths: &mut BTreeSet<PathBuf>,
    ) -> Result<Vec<PathBuf>> {
        let mut imports = Vec::new();
        'outer: for import in &self.imports {
            // skip built-in imports
            if import.level == 0
                && import
                    .path
                    .as_ref()
                    .map(|path| path.starts_with("vyper.") || path.starts_with("ethereum.ercs"))
                    .unwrap_or_default()
            {
                continue;
            }

            // Potential locations of imported source.
            let mut candidate_dirs = Vec::new();

            // For relative imports, vyper always checks only directory containing contract which
            // includes given import.
            if import.level > 0 {
                let mut candidate_dir = Some(self.path.as_path());

                for _ in 0..import.level {
                    candidate_dir = candidate_dir.and_then(|dir| dir.parent());
                }

                let candidate_dir = candidate_dir.ok_or_else(|| {
                    SolcError::msg(format!(
                        "Could not go {} levels up for import at {}",
                        import.level,
                        self.path.display()
                    ))
                })?;

                candidate_dirs.push(candidate_dir);
            } else {
                // For absolute imports, Vyper firstly checks current directory, and then root.
                if let Some(parent) = self.path.parent() {
                    candidate_dirs.push(parent);
                }
                candidate_dirs.push(paths.root.as_path());
            }

            candidate_dirs.extend(paths.libraries.iter().map(PathBuf::as_path));

            let import_path = {
                let mut path = PathBuf::new();

                if let Some(import_path) = &import.path {
                    path = path.join(import_path.replace('.', "/"));
                }

                if let Some(part) = &import.final_part {
                    path = path.join(part);
                }

                path
            };

            for candidate_dir in candidate_dirs {
                let candidate = candidate_dir.join(&import_path);
                for extension in VYPER_EXTENSIONS {
                    let candidate = candidate.clone().with_extension(extension);
                    trace!("trying {}", candidate.display());
                    if candidate.exists() {
                        imports.push(candidate);
                        include_paths.insert(candidate_dir.to_path_buf());
                        continue 'outer;
                    }
                }
            }

            return Err(SolcError::msg(format!(
                "failed to resolve import {}{} at {}",
                ".".repeat(import.level),
                import_path.display(),
                self.path.display()
            )));
        }
        Ok(imports)
    }
}

/// Parses a Vyper `pragma version` requirement into a [`VersionReq`].
///
/// Vyper's pragma follows PEP 440, not Cargo's semver. Two PEP 440 spellings frequently appear
/// in real-world contracts that `semver::VersionReq::parse` can't handle:
///
/// * The "compatible release" operator `~=`, e.g. `~=0.5.0` (≡ `>=0.5.0, <0.6.0`).
/// * Implicit pre-release tags without a hyphen, e.g. `0.5.0a1`, `0.5.0b1`, `0.5.0rc1` (semver
///   requires `0.5.0-a1`).
///
/// This helper first tries to parse the input as plain semver to preserve existing behavior, and
/// only falls back to the PEP 440 → semver translation if that fails.
fn parse_vyper_version_req(input: &str) -> Result<VersionReq, semver::Error> {
    let trimmed = strip_inline_comment(input).trim();
    if let Ok(req) = VersionReq::parse(trimmed) {
        return Ok(req);
    }
    VersionReq::parse(&pep440_to_semver_req(trimmed))
}

/// Strip a trailing `#` comment from a captured pragma line.
fn strip_inline_comment(s: &str) -> &str {
    s.split_once('#').map_or(s, |(head, _)| head)
}

/// Translate the subset of PEP 440 grammar that shows up in Vyper pragmas into semver syntax.
fn pep440_to_semver_req(input: &str) -> String {
    let hyphenated = hyphenate_prerelease(input.trim());

    if let Some(rest) = hyphenated.strip_prefix("~=") {
        return compatible_release(rest.trim());
    }
    if let Some(rest) = hyphenated.strip_prefix("==") {
        return format!("={}", rest.trim());
    }
    hyphenated
}

/// Insert a hyphen before bare PEP 440 pre-release labels (`a`, `b`, `rc`) so the result is valid
/// semver: `0.5.0a1` -> `0.5.0-a1`. Anything after `+` (build metadata) is copied verbatim.
fn hyphenate_prerelease(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len() + 1);
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'+' {
            out.push_str(&input[i..]);
            return out;
        }

        let prev_is_digit = out.as_bytes().last().is_some_and(u8::is_ascii_digit);
        if prev_is_digit {
            // `<digit>rc<digit>` -> `<digit>-rc<digit>`
            if c == b'r'
                && bytes.get(i + 1) == Some(&b'c')
                && bytes.get(i + 2).is_some_and(u8::is_ascii_digit)
            {
                out.push('-');
                out.push_str("rc");
                i += 2;
                continue;
            }
            // `<digit>a<digit>` / `<digit>b<digit>` -> `<digit>-a<digit>` / `<digit>-b<digit>`
            if (c == b'a' || c == b'b') && bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
                out.push('-');
                out.push(c as char);
                i += 1;
                continue;
            }
        }

        out.push(c as char);
        i += 1;
    }
    out
}

/// Expand the PEP 440 `~=` "compatible release" operator into the equivalent semver range.
///
/// Per PEP 440, `~=X.Y.Z` ≡ `>=X.Y.Z, ==X.Y.*` ≡ `>=X.Y.Z, <X.(Y+1).0`, and `~=X.Y` ≡
/// `>=X.Y, <(X+1).0.0`.
fn compatible_release(version: &str) -> String {
    // Strip pre-release / build metadata before splitting on `.`; PEP 440 allows e.g. `~=0.5.0a1`
    // and the upper bound is computed from the release component only.
    let core = version.split(['+', '-']).next().unwrap_or(version);
    let parts: Vec<&str> = core.split('.').collect();

    if parts.len() < 2 {
        // Not enough components to bump; encode at least the lower bound.
        return format!(">={version}");
    }

    let bump_idx = parts.len() - 2;
    let mut upper: Vec<String> = parts.iter().take(bump_idx + 1).map(|s| s.to_string()).collect();
    let Ok(n) = upper[bump_idx].parse::<u64>() else {
        return format!(">={version}");
    };
    upper[bump_idx] = (n + 1).to_string();
    while upper.len() < 3 {
        upper.push("0".to_string());
    }

    format!(">={version}, <{}", upper.join("."))
}

/// Parses given source trying to find all import directives.
fn parse_imports(content: &str) -> Vec<VyperImport> {
    let mut imports = Vec::new();

    for mut line in content.split('\n') {
        if let Ok(parts) = parse_import(&mut line) {
            imports.push(parts);
        }
    }

    imports
}

/// Parses given input, trying to find (import|from) part1.part2.part3 (import part4)?
fn parse_import(input: &mut &str) -> ModalResult<VyperImport> {
    (
        preceded(
            (alt(["from", "import"]), space1),
            (take_while(0.., |c| c == '.'), take_till(0.., [' '])),
        ),
        opt(preceded((space1, "import", space1), take_till(0.., [' ']))),
    )
        .parse_next(input)
        .map(|((dots, path), last)| VyperImport {
            level: dots.len(),
            path: (!path.is_empty()).then(|| path.to_string()),
            final_part: last.map(|p| p.to_string()),
        })
}

#[cfg(test)]
mod tests {
    use super::{
        VyperImport, VyperParsedSource, parse_import, parse_vyper_version_req, pep440_to_semver_req,
    };
    use crate::compilers::ParsedSource;
    use semver::{Version, VersionReq};
    use std::path::Path;
    use winnow::Parser;

    #[test]
    fn parses_semver_pragmas_unchanged() {
        let req = parse_vyper_version_req("^0.3.7").unwrap();
        assert_eq!(req, VersionReq::parse("^0.3.7").unwrap());
    }

    #[test]
    fn parses_pep440_compatible_release_three_part() {
        let req = parse_vyper_version_req("~=0.5.0").unwrap();
        let expected = VersionReq::parse(">=0.5.0, <0.6.0").unwrap();
        assert_eq!(req, expected);
        assert!(req.matches(&Version::parse("0.5.3").unwrap()));
        assert!(!req.matches(&Version::parse("0.6.0").unwrap()));
    }

    #[test]
    fn parses_pep440_compatible_release_two_part() {
        let req = parse_vyper_version_req("~=2.2").unwrap();
        let expected = VersionReq::parse(">=2.2, <3.0.0").unwrap();
        assert_eq!(req, expected);
    }

    #[test]
    fn parses_pep440_compatible_release_with_prerelease() {
        // `~=0.5.0a1` is the pragma snekmate uses for Vyper 0.5.0a1.
        let req = parse_vyper_version_req("~=0.5.0a1").unwrap();
        let expected = VersionReq::parse(">=0.5.0-a1, <0.6.0").unwrap();
        assert_eq!(req, expected);
        // Should match both the alpha and the eventual stable release within the same minor.
        assert!(req.matches(&Version::parse("0.5.0-a1").unwrap()));
        assert!(req.matches(&Version::parse("0.5.0").unwrap()));
    }

    #[test]
    fn parses_pep440_bare_prerelease_versions() {
        // `==0.5.0a1` -> `=0.5.0-a1`
        let req = parse_vyper_version_req("==0.5.0a1").unwrap();
        let expected = VersionReq::parse("=0.5.0-a1").unwrap();
        assert_eq!(req, expected);
    }

    #[test]
    fn pep440_translation_handles_rc_and_beta_tags() {
        assert_eq!(pep440_to_semver_req(">=0.5.0rc2"), ">=0.5.0-rc2");
        assert_eq!(pep440_to_semver_req(">=0.5.0b3"), ">=0.5.0-b3");
    }

    #[test]
    fn rejects_garbage_pragmas() {
        assert!(parse_vyper_version_req("not a version").is_err());
    }

    #[test]
    fn vyper_pragma_with_space_after_hash_is_recognized() {
        // Vyper accepts `# pragma version <req>` (with a space after `#`); make sure the
        // regex picks it up and the constraint is recorded.
        let parsed =
            VyperParsedSource::parse("# pragma version ~=0.5.0a1\n", Path::new("test.vy")).unwrap();
        let req = parsed.version_req().expect("expected a version requirement");
        assert!(req.matches(&Version::parse("0.5.0-a1").unwrap()));
        assert!(!req.matches(&Version::parse("0.6.0").unwrap()));
    }

    #[test]
    fn legacy_at_version_pragma_still_parses() {
        let parsed = VyperParsedSource::parse("#@version ^0.3.7\n", Path::new("test.vy")).unwrap();
        assert_eq!(parsed.version_req(), Some(&VersionReq::parse("^0.3.7").unwrap()));
    }

    #[test]
    fn can_parse_import() {
        assert_eq!(
            parse_import.parse("import one.two.three").unwrap(),
            VyperImport { level: 0, path: Some("one.two.three".to_string()), final_part: None }
        );
        assert_eq!(
            parse_import.parse("from one.two.three import four").unwrap(),
            VyperImport {
                level: 0,
                path: Some("one.two.three".to_string()),
                final_part: Some("four".to_string()),
            }
        );
        assert_eq!(
            parse_import.parse("from one import two").unwrap(),
            VyperImport {
                level: 0,
                path: Some("one".to_string()),
                final_part: Some("two".to_string()),
            }
        );
        assert_eq!(
            parse_import.parse("import one").unwrap(),
            VyperImport { level: 0, path: Some("one".to_string()), final_part: None }
        );
        assert_eq!(
            parse_import.parse("from . import one").unwrap(),
            VyperImport { level: 1, path: None, final_part: Some("one".to_string()) }
        );
        assert_eq!(
            parse_import.parse("from ... import two").unwrap(),
            VyperImport { level: 3, path: None, final_part: Some("two".to_string()) }
        );
        assert_eq!(
            parse_import.parse("from ...one.two import three").unwrap(),
            VyperImport {
                level: 3,
                path: Some("one.two".to_string()),
                final_part: Some("three".to_string())
            }
        );
    }
}
