use crate::{
    Graph, ProjectPathsConfig, SourceParser,
    compilers::ParsedSource,
    filter::MaybeSolData,
    resolver::parse::SolData,
};
use foundry_compilers_core::error::{Result, SolcError};
use itertools::Itertools;
use std::{
    collections::{BTreeSet, HashSet},
    path::{Path, PathBuf},
};

/// Performs DFS to collect all dependencies of a target
fn collect_deps<P: SourceParser<ParsedSource: MaybeSolData>>(
    path: &Path,
    paths: &ProjectPathsConfig<<P::ParsedSource as ParsedSource>::Language>,
    graph: &Graph<P>,
    deps: &mut HashSet<PathBuf>,
) -> Result<()> {
    if deps.insert(path.to_path_buf()) {
        let target_dir = path.parent().ok_or_else(|| {
            SolcError::msg(format!("failed to get parent directory for \"{}\"", path.display()))
        })?;

        let node_id = graph
            .files()
            .get(path)
            .ok_or_else(|| SolcError::msg(format!("cannot resolve file at {}", path.display())))?;

        if let Some(data) = graph.node(*node_id).data.sol_data() {
            for import in &data.imports {
                let path = paths.resolve_import(target_dir, import.data().path())?;
                collect_deps(&path, paths, graph, deps)?;
            }
        }
    }
    Ok(())
}

/// We want to make order in which sources are written to resulted flattened file
/// deterministic.
///
/// We can't just sort files alphabetically as it might break compilation, because Solidity
/// does not allow base class definitions to appear after derived contract
/// definitions.
///
/// Instead, we sort files by the number of their dependencies (imports of any depth) in ascending
/// order. If files have the same number of dependencies, we sort them alphabetically.
/// Target file is always placed last.
pub fn collect_ordered_deps<P: SourceParser<ParsedSource: MaybeSolData>>(
    path: &Path,
    paths: &ProjectPathsConfig<<P::ParsedSource as ParsedSource>::Language>,
    graph: &Graph<P>,
) -> Result<Vec<PathBuf>> {
    let mut deps = HashSet::new();
    collect_deps(path, paths, graph, &mut deps)?;

    // Remove path prior counting dependencies
    // It will be added later to the end of resulted Vec
    deps.remove(path);

    let mut paths_with_deps_count = Vec::new();
    for path in deps {
        let mut path_deps = HashSet::new();
        collect_deps(&path, paths, graph, &mut path_deps)?;
        paths_with_deps_count.push((path_deps.len(), path));
    }

    paths_with_deps_count.sort_by(|(count_0, path_0), (count_1, path_1)| {
        // Compare dependency counts
        match count_0.cmp(count_1) {
            o if !o.is_eq() => return o,
            _ => {}
        };

        // Try comparing file names
        if let Some((name_0, name_1)) = path_0.file_name().zip(path_1.file_name()) {
            match name_0.cmp(name_1) {
                o if !o.is_eq() => return o,
                _ => {}
            }
        }

        // If both filenames and dependency counts are equal, fallback to comparing file paths
        path_0.cmp(path_1)
    });

    let mut ordered_deps =
        paths_with_deps_count.into_iter().map(|(_, path)| path).collect::<Vec<_>>();

    ordered_deps.push(path.to_path_buf());

    Ok(ordered_deps)
}

pub fn combine_version_pragmas(pragmas: &[impl AsRef<str>]) -> Option<String> {
    let versions = pragmas
        .iter()
        .map(AsRef::as_ref)
        .filter_map(SolData::parse_version_pragma)
        .filter_map(Result::ok)
        .flat_map(|req| req.comparators)
        .map(|comp| comp.to_string())
        .collect::<BTreeSet<_>>();
    if versions.is_empty() {
        return None;
    }
    Some(format!("pragma solidity {};", versions.iter().format(" ")))
}
