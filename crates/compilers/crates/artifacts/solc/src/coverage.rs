//! Coverage maps of compiler-instrumented sources.
//!
//! The map follows LLVM's coverage-mapping model. An instrumented build emits `COVERAGE; PUSHn
//! <counter>` probes, where `COVERAGE` is the otherwise undefined opcode `0xC2`. Counter IDs start
//! at 1 and are unique within one compilation. Each source's [`SourceCoverage`] has one
//! [`FunctionCoverage`] record per function, modifier, or creation-time scope, laid out like
//! LLVM's function records: regions that refer to counters through [`Counter`] values, with byte
//! offsets as positions.
//!
//! A bytecode output lists the counters the compiler dropped in favor of another counter in
//! [`CoverageAlias`]es.

use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{Error as _, Unexpected},
    ser::SerializeSeq,
};

/// The version of the coverage map this crate reads and writes.
pub const COVERAGE_MAP_VERSION: u32 = 2;

/// A counter the compiler dropped, resolved to a counter it kept: `[dropped, kept]`.
///
/// Before it evaluates a map, a consumer sets the hits of the dropped counter to the hits of the
/// kept counter for the code object whose bytecode lists the alias.
pub type CoverageAlias = [u32; 2];

/// The coverage records of one source file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceCoverage {
    /// The version of the map format, [`COVERAGE_MAP_VERSION`].
    #[serde(deserialize_with = "deserialize_version")]
    pub version: u32,
    /// The records of the functions, modifiers, and creation-time scopes defined in the source.
    pub functions: Vec<FunctionCoverage>,
}

impl Default for SourceCoverage {
    fn default() -> Self {
        Self { version: COVERAGE_MAP_VERSION, functions: Vec::new() }
    }
}

/// The coverage record of one function, laid out like an LLVM function record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionCoverage {
    /// The PGO name: unique within the compilation and stable across builds, such as
    /// `src/Foo.sol:Foo.bar(uint256)`. Creation-time decisions outside functions have a synthetic
    /// record such as `src/Foo.sol:Foo.<init>`.
    pub name: String,
    /// A structural hash of the counter layout of the record.
    #[serde(with = "hex_u64")]
    pub hash: u64,
    /// The record's own contiguous counter range, as `[first, len]`. The record of a modifier has
    /// `len = 0`, because its counters live in its expansions.
    pub counters: [u32; 2],
    /// The virtual file table, as source IDs. Index 0 is the record's own source, and each
    /// modifier expansion adds an entry.
    pub files: Vec<u32>,
    /// The counter expressions, as `[lhs, rhs]` pairs of [`Counter`] values local to the record.
    #[serde(default)]
    pub expressions: Vec<[u32; 2]>,
    /// The regions, starting with a code region over the whole body.
    pub regions: Vec<Region>,
    /// The MC/DC decisions of the record.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<Decision>,
}

impl FunctionCoverage {
    /// The first counter ID of the record's own counter range.
    pub const fn first_counter(&self) -> u32 {
        self.counters[0]
    }

    /// The number of counters in the record's own counter range.
    pub const fn counter_len(&self) -> u32 {
        self.counters[1]
    }
}

/// An MC/DC decision of a function record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    /// The runtime ID of the decision, as in its `COVERAGE` probes. Unique within a compilation.
    pub id: u32,
    /// The number of conditions.
    pub conditions: u32,
    /// The test vectors in test vector index order. A vector has one character per condition ID
    /// (`T`, `F`, or `-` for a condition that was not evaluated), followed by the outcome
    /// character.
    pub vectors: Vec<String>,
}

/// A tagged counter value, as in LLVM's coverage-mapping encoding.
///
/// The low two bits are the tag: `0` is zero, `1` is the counter `c >> 2`, `2` is the
/// subtraction `expressions[c >> 2]`, and `3` is the addition `expressions[c >> 2]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Counter {
    /// The constant zero.
    Zero,
    /// The hits of a counter.
    Counter(u32),
    /// The difference of the two operands of an expression.
    Subtract(u32),
    /// The sum of the two operands of an expression.
    Add(u32),
}

impl Counter {
    /// Decodes a tagged counter value.
    pub const fn decode(value: u32) -> Self {
        let id = value >> 2;
        match value & 3 {
            0 => Self::Zero,
            1 => Self::Counter(id),
            2 => Self::Subtract(id),
            _ => Self::Add(id),
        }
    }

    /// Encodes the counter as a tagged value.
    pub const fn encode(self) -> u32 {
        match self {
            Self::Zero => 0,
            Self::Counter(id) => id << 2 | 1,
            Self::Subtract(id) => id << 2 | 2,
            Self::Add(id) => id << 2 | 3,
        }
    }
}

/// A region of a function record.
///
/// A region serializes as an array that starts with its LLVM kind, followed by its file index,
/// byte offset, and byte length. `count` fields are tagged [`Counter`] values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    /// `[0, file, start, len, count]`: code with an execution count.
    Code {
        /// The index in the virtual file table.
        file: u32,
        /// The start byte offset.
        start: u32,
        /// The length in bytes.
        len: u32,
        /// The execution count.
        count: u32,
    },
    /// `[1, file, start, len, count, expandedFile]`: the expansion of a virtual file, such as a
    /// modifier invocation.
    Expansion {
        /// The index in the virtual file table.
        file: u32,
        /// The start byte offset.
        start: u32,
        /// The length in bytes.
        len: u32,
        /// The count of the expansion, which is the count of the first region of the expanded
        /// file.
        count: u32,
        /// The index of the expanded virtual file.
        expanded_file: u32,
    },
    /// `[2, file, start, len]`: code that constant folding removed.
    Skipped {
        /// The index in the virtual file table.
        file: u32,
        /// The start byte offset.
        start: u32,
        /// The length in bytes.
        len: u32,
    },
    /// `[4, file, start, len, trueCount, falseCount]`: a condition.
    Branch {
        /// The index in the virtual file table.
        file: u32,
        /// The start byte offset.
        start: u32,
        /// The length in bytes.
        len: u32,
        /// How many times the condition held.
        true_count: u32,
        /// How many times the condition did not hold.
        false_count: u32,
    },
    /// `[5, file, start, len, decision]`: an MC/DC decision.
    Decision {
        /// The index in the virtual file table.
        file: u32,
        /// The start byte offset.
        start: u32,
        /// The length in bytes.
        len: u32,
        /// The index in the record's `decisions`.
        decision: u32,
    },
    /// `[6, file, start, len, trueCount, falseCount, condId, falseNext, trueNext]`: a condition of
    /// an MC/DC decision.
    McdcBranch {
        /// The index in the virtual file table.
        file: u32,
        /// The start byte offset.
        start: u32,
        /// The length in bytes.
        len: u32,
        /// How many times the condition held.
        true_count: u32,
        /// How many times the condition did not hold.
        false_count: u32,
        /// The condition ID within its decision.
        cond_id: u32,
        /// The condition evaluated next when this one does not hold, or `-1` when the decision
        /// ends.
        false_next: i32,
        /// The condition evaluated next when this one holds, or `-1` when the decision ends.
        true_next: i32,
    },
}

impl Region {
    /// The LLVM region kind.
    pub const fn kind(&self) -> u32 {
        match self {
            Self::Code { .. } => 0,
            Self::Expansion { .. } => 1,
            Self::Skipped { .. } => 2,
            Self::Branch { .. } => 4,
            Self::Decision { .. } => 5,
            Self::McdcBranch { .. } => 6,
        }
    }

    /// The index in the virtual file table.
    pub const fn file(&self) -> u32 {
        match *self {
            Self::Code { file, .. }
            | Self::Expansion { file, .. }
            | Self::Skipped { file, .. }
            | Self::Branch { file, .. }
            | Self::Decision { file, .. }
            | Self::McdcBranch { file, .. } => file,
        }
    }

    /// The start byte offset.
    pub const fn start(&self) -> u32 {
        match *self {
            Self::Code { start, .. }
            | Self::Expansion { start, .. }
            | Self::Skipped { start, .. }
            | Self::Branch { start, .. }
            | Self::Decision { start, .. }
            | Self::McdcBranch { start, .. } => start,
        }
    }

    /// The length in bytes.
    pub const fn len(&self) -> u32 {
        match *self {
            Self::Code { len, .. }
            | Self::Expansion { len, .. }
            | Self::Skipped { len, .. }
            | Self::Branch { len, .. }
            | Self::Decision { len, .. }
            | Self::McdcBranch { len, .. } => len,
        }
    }

    /// Returns whether the region spans no bytes.
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The region as the array it serializes to.
    fn to_array(self) -> Vec<i64> {
        let mut array = vec![
            i64::from(self.kind()),
            i64::from(self.file()),
            i64::from(self.start()),
            i64::from(self.len()),
        ];
        match self {
            Self::Code { count, .. } => array.push(count.into()),
            Self::Expansion { count, expanded_file, .. } => {
                array.extend([i64::from(count), expanded_file.into()])
            }
            Self::Skipped { .. } => {}
            Self::Branch { true_count, false_count, .. } => {
                array.extend([i64::from(true_count), false_count.into()])
            }
            Self::Decision { decision, .. } => array.push(decision.into()),
            Self::McdcBranch {
                true_count, false_count, cond_id, false_next, true_next, ..
            } => array.extend([
                i64::from(true_count),
                false_count.into(),
                cond_id.into(),
                false_next.into(),
                true_next.into(),
            ]),
        }
        array
    }

    /// Reads a region from its array form.
    fn from_array(array: &[i64]) -> Result<Self, String> {
        let &[kind, file, start, len, ref rest @ ..] = array else {
            return Err("a region needs a kind, file, start, and length".into());
        };
        let u32_of = |value: i64| u32::try_from(value).map_err(|_| format!("{value} is too big"));
        let i32_of = |value: i64| i32::try_from(value).map_err(|_| format!("{value} is too big"));
        let (file, start, len) = (u32_of(file)?, u32_of(start)?, u32_of(len)?);
        let region = match (kind, rest) {
            (0, &[count]) => Self::Code { file, start, len, count: u32_of(count)? },
            (1, &[count, expanded_file]) => Self::Expansion {
                file,
                start,
                len,
                count: u32_of(count)?,
                expanded_file: u32_of(expanded_file)?,
            },
            (2, []) => Self::Skipped { file, start, len },
            (4, &[true_count, false_count]) => Self::Branch {
                file,
                start,
                len,
                true_count: u32_of(true_count)?,
                false_count: u32_of(false_count)?,
            },
            (5, &[decision]) => Self::Decision { file, start, len, decision: u32_of(decision)? },
            (6, &[true_count, false_count, cond_id, false_next, true_next]) => Self::McdcBranch {
                file,
                start,
                len,
                true_count: u32_of(true_count)?,
                false_count: u32_of(false_count)?,
                cond_id: u32_of(cond_id)?,
                false_next: i32_of(false_next)?,
                true_next: i32_of(true_next)?,
            },
            (0 | 1 | 2 | 4 | 5 | 6, _) => {
                return Err(format!("a region of kind {kind} has the wrong number of fields"));
            }
            _ => return Err(format!("unknown region kind {kind}")),
        };
        Ok(region)
    }
}

impl Serialize for Region {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let array = self.to_array();
        let mut seq = serializer.serialize_seq(Some(array.len()))?;
        for value in array {
            seq.serialize_element(&value)?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for Region {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_array(&Vec::<i64>::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

fn deserialize_version<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
    let version = u32::deserialize(deserializer)?;
    if version != COVERAGE_MAP_VERSION {
        return Err(D::Error::invalid_value(
            Unexpected::Unsigned(version.into()),
            &"coverage map version 2",
        ));
    }
    Ok(version)
}

/// A 64-bit hash as a `0x`-prefixed hex string.
mod hex_u64 {
    use super::*;

    pub(super) fn serialize<S: Serializer>(hash: &u64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&format_args!("{hash:#018x}"))
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
        let hash = String::deserialize(deserializer)?;
        let digits = hash.strip_prefix("0x").unwrap_or(&hash);
        u64::from_str_radix(digits, 16)
            .map_err(|_| D::Error::invalid_value(Unexpected::Str(&hash), &"a 64-bit hex string"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAP: &str = r#"{
        "version": 2,
        "functions": [{
            "name": "src/Foo.sol:Foo.bar(uint256)",
            "hash": "0x1f2e3d4c5b6a7980",
            "counters": [17, 3],
            "files": [3, 5],
            "expressions": [[69, 73]],
            "regions": [
                [0, 0, 120, 340, 69],
                [1, 0, 130, 8, 77, 1],
                [2, 0, 200, 20],
                [4, 0, 220, 10, 73, 14],
                [5, 0, 220, 30, 0],
                [6, 0, 220, 10, 73, 14, 0, 1, -1]
            ],
            "decisions": [{"id": 4, "conditions": 2, "vectors": ["TTT", "TFF", "F-F"]}]
        }]
    }"#;

    #[test]
    fn round_trips_a_map() {
        let coverage: SourceCoverage = serde_json::from_str(MAP).unwrap();
        let function = &coverage.functions[0];
        assert_eq!(
            (function.hash, function.first_counter(), function.counter_len()),
            (0x1f2e3d4c5b6a7980, 17, 3)
        );
        assert_eq!(function.regions[0], Region::Code { file: 0, start: 120, len: 340, count: 69 });
        assert_eq!(
            function.regions[5],
            Region::McdcBranch {
                file: 0,
                start: 220,
                len: 10,
                true_count: 73,
                false_count: 14,
                cond_id: 0,
                false_next: 1,
                true_next: -1,
            }
        );
        let expected = serde_json::from_str::<serde_json::Value>(MAP).unwrap();
        assert_eq!(serde_json::to_value(&coverage).unwrap(), expected);
    }

    #[test]
    fn decodes_tagged_counters() {
        let values = [0, 69, 6, 7, 1];
        let counters = values.map(Counter::decode);
        assert_eq!(
            counters,
            [
                Counter::Zero,
                Counter::Counter(17),
                Counter::Subtract(1),
                Counter::Add(1),
                Counter::Counter(0)
            ]
        );
        assert_eq!(counters.map(Counter::encode), values);
    }

    #[test]
    fn rejects_malformed_maps() {
        let version_1 = MAP.replace(r#""version": 2"#, r#""version": 1"#);
        assert!(serde_json::from_str::<SourceCoverage>(&version_1).is_err());
        let bad_kind = MAP.replace("[2, 0, 200, 20]", "[3, 0, 200, 20]");
        assert!(serde_json::from_str::<SourceCoverage>(&bad_kind).is_err());
        let short = MAP.replace("[4, 0, 220, 10, 73, 14]", "[4, 0, 220, 10, 73]");
        assert!(serde_json::from_str::<SourceCoverage>(&short).is_err());
        let bad_hash = MAP.replace("0x1f2e3d4c5b6a7980", "not-hex");
        assert!(serde_json::from_str::<SourceCoverage>(&bad_hash).is_err());
    }
}
