//! Coverage maps of compiler-instrumented sources.
//!
//! An instrumented build emits `PUSHn <counter>; COVERAGE` probes, where `COVERAGE` is the
//! otherwise undefined opcode `0xC2`. Counter IDs start at 1 and are unique within one
//! compilation. Each source's [`SourceCoverage`] lists its coverage regions, including those of
//! functions that no bytecode contains, with the counters whose hits give their execution counts.

use serde::{Deserialize, Serialize};

/// The coverage regions of one source file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceCoverage {
    /// The functions, modifiers, and Yul functions defined in the source.
    pub functions: Vec<FunctionCoverage>,
}

/// The coverage regions of one function.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionCoverage {
    /// The function name, or `constructor`, `receive`, or `fallback`.
    pub name: String,
    /// The contract the function is defined in, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract: Option<String>,
    /// The source range of the function, as `start:length:sourceIndex`.
    pub src: String,
    /// How many times the function was entered.
    pub count: CoverageCount,
    /// The statements of the function body.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub statements: Vec<StatementCoverage>,
    /// The decisions of the function body.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub branches: Vec<BranchCoverage>,
}

/// The coverage region of one statement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatementCoverage {
    /// The source range of the statement, as `start:length:sourceIndex`.
    pub src: String,
    /// How many times the statement started executing.
    pub count: CoverageCount,
}

/// The coverage regions of one decision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchCoverage {
    /// The source range of the decision, as `start:length:sourceIndex`.
    pub src: String,
    /// How many times each path was taken.
    ///
    /// A two-way decision lists the path taken when its condition holds first. A `try`
    /// statement lists its clauses and a `switch` its cases in source order, followed by the
    /// implicit default case if there is none.
    pub paths: Vec<CoverageCount>,
}

/// An execution count, given by counter hits.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CoverageCount {
    /// The hits of one counter.
    Counter(u32),
    /// The hits of the first counter minus the hits of the others.
    Difference {
        /// The counters, minuend first.
        sub: Vec<u32>,
    },
}

impl CoverageCount {
    /// Evaluates the count with the given counter hits.
    pub fn eval(&self, mut hits: impl FnMut(u32) -> u64) -> u64 {
        match self {
            Self::Counter(counter) => hits(*counter),
            Self::Difference { sub } => {
                let mut counters = sub.iter();
                let minuend = counters.next().map_or(0, |&counter| hits(counter));
                counters.fold(minuend, |count, &counter| count.saturating_sub(hits(counter)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_counts() {
        let coverage: SourceCoverage = serde_json::from_str(
            r#"{"functions":[{"name":"f","contract":"C","src":"0:30:0","count":1,
                "statements":[{"src":"10:5:0","count":2}],
                "branches":[{"src":"10:15:0","paths":[3,{"sub":[2,3]}]}]}]}"#,
        )
        .unwrap();
        let function = &coverage.functions[0];
        assert_eq!(function.contract.as_deref(), Some("C"));
        let hits = |counter| [0, 7, 5, 2][counter as usize];
        let paths = &function.branches[0].paths;
        assert_eq!(paths.iter().map(|path| path.eval(hits)).collect::<Vec<_>>(), [2, 3]);
        assert_eq!(function.statements[0].count.eval(hits), 5);
    }
}
