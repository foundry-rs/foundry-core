use crate::{
    Ast, CompactBytecode, CompactContract, CompactContractBytecode, CompactContractBytecodeCow,
    CompactDeployedBytecode, DevDoc, Ewasm, FunctionDebugData, GasEstimates, GeneratedSource,
    Metadata, Offsets, SourceFile, StorageLayout, UserDoc,
};
use alloy_json_abi::JsonAbi;
use serde::{Deserialize, Deserializer, Serialize};
use std::{borrow::Cow, collections::BTreeMap};

/// Represents the `Artifact` that `ConfigurableArtifacts` emits.
///
/// This is essentially a superset of [`CompactContractBytecode`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigurableContractArtifact {
    /// The Ethereum Contract ABI. If empty, it is represented as an empty
    /// array. See <https://docs.soliditylang.org/en/develop/abi-spec.html>
    pub abi: Option<JsonAbi>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytecode: Option<CompactBytecode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployed_bytecode: Option<CompactDeployedBytecode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assembly: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_assembly: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opcodes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method_identifiers: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub generated_sources: Vec<GeneratedSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function_debug_data: Option<BTreeMap<String, FunctionDebugData>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gas_estimates: Option<GasEstimates>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_metadata: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_layout: Option<StorageLayout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transient_storage_layout: Option<StorageLayout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub userdoc: Option<UserDoc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub devdoc: Option<DevDoc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ir_optimized: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ir_optimized_ast: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ewasm: Option<Ewasm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ast: Option<Ast>,
    /// The identifier of the source file
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
}

impl ConfigurableContractArtifact {
    /// Reads artifact JSON without buffering valid deployed bytecode through Serde flatten.
    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        // Flattened Option fields suppress some malformed inputs; retain that behavior on fallback.
        serde_json::from_str::<DirectArtifactValue>(json)
            .map(|artifact| artifact.0)
            .or_else(|_| serde_json::from_str(json))
    }

    /// Returns the inner element that contains the core bytecode related information
    pub fn into_contract_bytecode(self) -> CompactContractBytecode {
        self.into()
    }

    /// Looks for all link references in deployment and runtime bytecodes
    pub fn all_link_references(&self) -> BTreeMap<String, BTreeMap<String, Vec<Offsets>>> {
        let mut links = BTreeMap::new();
        if let Some(bcode) = &self.bytecode {
            links.extend(bcode.link_references.clone());
        }

        if let Some(d_bcode) = &self.deployed_bytecode
            && let Some(bcode) = &d_bcode.bytecode
        {
            links.extend(bcode.link_references.clone());
        }
        links
    }

    /// Returns the source file of this artifact's contract
    pub fn source_file(&self) -> Option<SourceFile> {
        self.id.map(|id| SourceFile { id, ast: self.ast.clone() })
    }
}

impl From<ConfigurableContractArtifact> for CompactContractBytecode {
    fn from(artifact: ConfigurableContractArtifact) -> Self {
        Self {
            abi: artifact.abi,
            bytecode: artifact.bytecode,
            deployed_bytecode: artifact.deployed_bytecode,
        }
    }
}

impl From<ConfigurableContractArtifact> for CompactContract {
    fn from(artifact: ConfigurableContractArtifact) -> Self {
        CompactContractBytecode::from(artifact).into()
    }
}

impl<'a> From<&'a ConfigurableContractArtifact> for CompactContractBytecodeCow<'a> {
    fn from(artifact: &'a ConfigurableContractArtifact) -> Self {
        CompactContractBytecodeCow {
            abi: artifact.abi.as_ref().map(Cow::Borrowed),
            bytecode: artifact.bytecode.as_ref().map(Cow::Borrowed),
            deployed_bytecode: artifact.deployed_bytecode.as_ref().map(Cow::Borrowed),
        }
    }
}

// The remote derive checks that the direct reader covers every artifact field.
#[derive(Deserialize)]
#[serde(remote = "ConfigurableContractArtifact", rename_all = "camelCase")]
#[allow(dead_code)]
struct DirectArtifact {
    #[serde(deserialize_with = "deserialize_direct_abi")]
    abi: Option<JsonAbi>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bytecode: Option<CompactBytecode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(deserialize_with = "deserialize_direct_deployed")]
    deployed_bytecode: Option<CompactDeployedBytecode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    assembly: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    legacy_assembly: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    opcodes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    method_identifiers: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    generated_sources: Vec<GeneratedSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    function_debug_data: Option<BTreeMap<String, FunctionDebugData>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    gas_estimates: Option<GasEstimates>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    raw_metadata: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    metadata: Option<Metadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    storage_layout: Option<StorageLayout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    transient_storage_layout: Option<StorageLayout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    userdoc: Option<UserDoc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    devdoc: Option<DevDoc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ir_optimized: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ir_optimized_ast: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ewasm: Option<Ewasm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ast: Option<Ast>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id: Option<u32>,
}

#[derive(Deserialize)]
#[serde(transparent)]
struct DirectArtifactValue(#[serde(with = "DirectArtifact")] ConfigurableContractArtifact);

fn deserialize_direct_deployed<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<CompactDeployedBytecode>, D::Error> {
    Option::<DirectDeployed>::deserialize(deserializer).map(|value| value.map(|value| value.0))
}

struct DirectDeployed(CompactDeployedBytecode);

impl<'de> Deserialize<'de> for DirectDeployed {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Fields {
            object: crate::BytecodeObject,
            #[serde(default)]
            source_map: Option<String>,
            #[serde(default)]
            link_references: BTreeMap<String, BTreeMap<String, Vec<Offsets>>>,
            #[serde(default)]
            immutable_references: BTreeMap<String, Vec<Offsets>>,
        }

        struct MapVisitor;
        impl<'de> serde::de::Visitor<'de> for MapVisitor {
            type Value = DirectDeployed;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a map")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                let fields =
                    Fields::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                Ok(DirectDeployed(CompactDeployedBytecode {
                    bytecode: Some(CompactBytecode {
                        object: fields.object,
                        source_map: fields.source_map,
                        link_references: fields.link_references,
                    }),
                    immutable_references: fields.immutable_references,
                }))
            }
        }

        deserializer.deserialize_map(MapVisitor)
    }
}

fn deserialize_direct_abi<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<JsonAbi>, D::Error> {
    Option::<DirectAbi>::deserialize(deserializer).map(|value| value.map(|value| value.0))
}

struct DirectAbi(JsonAbi);

impl<'de> Deserialize<'de> for DirectAbi {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Tag<'a> {
            #[serde(rename = "type", borrow)]
            kind: Cow<'a, str>,
        }

        struct AbiVisitor;
        impl<'de> serde::de::Visitor<'de> for AbiVisitor {
            type Value = DirectAbi;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a valid JSON ABI sequence")
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut abi = JsonAbi::new();
                while let Some(raw) = seq.next_element::<&serde_json::value::RawValue>()? {
                    let tag = serde_json::from_str::<Tag<'_>>(raw.get())
                        .map_err(serde::de::Error::custom)?;
                    // Read the concrete item directly instead of buffering its entire parameter tree.
                    let item = match tag.kind.as_ref() {
                        "constructor" => DirectConstructor::deserialize(
                            &mut serde_json::Deserializer::from_str(raw.get()),
                        )
                        .map(Into::into),
                        "function" => DirectFunction::deserialize(
                            &mut serde_json::Deserializer::from_str(raw.get()),
                        )
                        .map(Into::into),
                        "event" => DirectEvent::deserialize(
                            &mut serde_json::Deserializer::from_str(raw.get()),
                        )
                        .map(Into::into),
                        "error" => DirectError::deserialize(
                            &mut serde_json::Deserializer::from_str(raw.get()),
                        )
                        .map(Into::into),
                        "fallback" => serde_json::from_str::<alloy_json_abi::Fallback>(raw.get())
                            .map(Into::into),
                        "receive" => serde_json::from_str::<alloy_json_abi::Receive>(raw.get())
                            .map(Into::into),
                        _ => return Err(serde::de::Error::custom("unknown ABI item type")),
                    }
                    .map_err(serde::de::Error::custom)?;
                    match item {
                        alloy_json_abi::AbiItem::Constructor(value) => {
                            if abi.constructor.replace(value.into_owned()).is_some() {
                                return Err(serde::de::Error::duplicate_field("constructor"));
                            }
                        }
                        alloy_json_abi::AbiItem::Fallback(value) => {
                            if abi.fallback.replace(value.into_owned()).is_some() {
                                return Err(serde::de::Error::duplicate_field("fallback"));
                            }
                        }
                        alloy_json_abi::AbiItem::Receive(value) => {
                            if abi.receive.replace(value.into_owned()).is_some() {
                                return Err(serde::de::Error::duplicate_field("receive"));
                            }
                        }
                        alloy_json_abi::AbiItem::Function(value) => abi
                            .functions
                            .entry(value.name.clone())
                            .or_insert_with(|| Vec::with_capacity(1))
                            .push(value.into_owned()),
                        alloy_json_abi::AbiItem::Event(value) => abi
                            .events
                            .entry(value.name.clone())
                            .or_insert_with(|| Vec::with_capacity(1))
                            .push(value.into_owned()),
                        alloy_json_abi::AbiItem::Error(value) => abi
                            .errors
                            .entry(value.name.clone())
                            .or_insert_with(|| Vec::with_capacity(1))
                            .push(value.into_owned()),
                    }
                }
                Ok(DirectAbi(abi))
            }
        }

        deserializer.deserialize_seq(AbiVisitor)
    }
}
#[derive(Deserialize)]
#[serde(
    remote = "alloy_json_abi::Constructor",
    rename = "constructor",
    rename_all = "camelCase",
    tag = "type"
)]
#[allow(dead_code)]
struct DirectConstructor {
    #[serde(deserialize_with = "crate::serde_helpers::deserialize_small_vec")]
    inputs: Vec<alloy_json_abi::Param>,
    #[serde(default, flatten, with = "alloy_json_abi::serde_state_mutability_compat")]
    state_mutability: alloy_json_abi::StateMutability,
}

#[derive(Deserialize)]
#[serde(
    remote = "alloy_json_abi::Function",
    rename = "function",
    rename_all = "camelCase",
    tag = "type"
)]
#[allow(dead_code)]
struct DirectFunction {
    #[serde(deserialize_with = "deserialize_abi_name")]
    name: String,
    #[serde(deserialize_with = "crate::serde_helpers::deserialize_small_vec")]
    inputs: Vec<alloy_json_abi::Param>,
    #[serde(deserialize_with = "crate::serde_helpers::deserialize_small_vec")]
    outputs: Vec<alloy_json_abi::Param>,
    #[serde(default, flatten, with = "alloy_json_abi::serde_state_mutability_compat")]
    state_mutability: alloy_json_abi::StateMutability,
}

#[derive(Deserialize)]
#[serde(remote = "alloy_json_abi::Event", rename = "event", rename_all = "camelCase", tag = "type")]
#[allow(dead_code)]
struct DirectEvent {
    #[serde(deserialize_with = "deserialize_abi_name")]
    name: String,
    #[serde(deserialize_with = "crate::serde_helpers::deserialize_small_vec")]
    inputs: Vec<alloy_json_abi::EventParam>,
    anonymous: bool,
}

#[derive(Deserialize)]
#[serde(remote = "alloy_json_abi::Error", rename = "error", rename_all = "camelCase", tag = "type")]
#[allow(dead_code)]
struct DirectError {
    #[serde(deserialize_with = "deserialize_abi_name")]
    name: String,
    #[serde(deserialize_with = "crate::serde_helpers::deserialize_small_vec")]
    inputs: Vec<alloy_json_abi::Param>,
}

fn deserialize_abi_name<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let name = String::deserialize(deserializer)?;
    if !name.is_empty() && !alloy_json_abi::parser::is_valid_identifier(&name) {
        return Err(serde::de::Error::invalid_value(
            serde::de::Unexpected::Str(&name),
            &"a valid Solidity identifier",
        ));
    }
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_abi_reader_matches_deserialize() {
        for abi in [
            "null",
            "[]",
            "{}",
            "true",
            "123",
            "[null]",
            r#"[{"type":"event","name":"E","inputs":[],"anonymous":false,"unknown":1e999}]"#,
            r#"[{"type":"event","name":"E","inputs":[{"name":"x","type":"uint256","unknown":1e999}],"anonymous":false}]"#,
            r#"[{"type":"function","name":"f","inputs":[],"outputs":[],"stateMutability":"view"}]"#,
            r#"[{"type":"function","name":"f","inputs":[{"name":"x","type":"tuple","components":[{"name":"y","type":"uint256"}]}],"outputs":[],"constant":true,"payable":false}]"#,
            r#"[{"type":"function","name":"f","inputs":[],"outputs":[]},{"type":"function","name":"f","inputs":[],"outputs":[]}]"#,
            r#"[{"type":"constructor","inputs":[],"stateMutability":"nonpayable"},{"type":"fallback","stateMutability":"payable"},{"type":"receive","stateMutability":"payable"}]"#,
            r#"[{"type":"constructor","inputs":[]},{"type":"constructor","inputs":[]}]"#,
            r#"[{"type":"fallback"},{"type":"fallback"}]"#,
            r#"[{"type":"receive"},{"type":"receive"}]"#,
            r#"[{"type":"event","name":"E","inputs":[{"name":"x","type":"address","indexed":true}],"anonymous":false},{"type":"error","name":"Oops","inputs":[]}]"#,
            r#"[{"type":"function","name":"f","inputs":[{"name":"x","type":"uint256","indexed":false}],"outputs":[]}]"#,
            r#"[{"type":"function","type":"event","name":"f","inputs":[],"outputs":[]}]"#,
            r#"[{"type":"function","name":"f","name":"g","inputs":[],"outputs":[]}]"#,
            r#"[{"name":"f","inputs":[],"outputs":[]}]"#,
            r#"[{"type":"unknown"}]"#,
            r#"[{"type":"funct\u0069on","name":"f","inputs":[],"outputs":[]}]"#,
            r#"[{"type":"function","name":"f","inputs":[],"outputs":[],"unknown":{"n":123456789012345678901234567890}}]"#,
        ] {
            let json = format!(r#"{{"abi":{abi}}}"#);
            let expected = serde_json::from_str::<ConfigurableContractArtifact>(&json);
            let actual = ConfigurableContractArtifact::from_json(&json);
            match expected {
                Ok(expected) => assert_eq!(actual.unwrap(), expected, "{json}"),
                Err(expected) => {
                    assert_eq!(actual.unwrap_err().to_string(), expected.to_string(), "{json}")
                }
            }
        }
    }

    #[test]
    fn direct_artifact_reader_matches_deserialize() {
        for deployed in [
            "null",
            "{}",
            r#"{"object":"0x1234"}"#,
            r#"{"object":"0x12","unknown":1e999}"#,
            r#"{"object":"0x12","linkReferences":{"a":{"L":[{"start":1,"length":20,"unknown":1e999}]}}}"#,
            r#"{"object":"0x__placeholder__","sourceMap":"1:2:3","linkReferences":{"a":{"L":[{"start":1,"length":20}]}},"immutableReferences":{"1":[{"start":4,"length":32}]}}"#,
            r#"{"object":"0x12","unknown":[{"nested":true}]}"#,
            r#"{"object":123}"#,
            r#"{"object":null}"#,
            r#"{"object":[]}"#,
            r#"{"object":"0x12","sourceMap":[]}"#,
            r#"{"object":"0x12","linkReferences":{"a":{"L":[{"start":{},"length":20}]}}}"#,
            r#"{"object":"0x12","linkReferences":{"a":{"L":[{"start":4294967296,"length":20}]}}}"#,
            r#"{"object":"0x12","linkReferences":null}"#,
            r#"{"object":"0x12","object":"0x34"}"#,
            r#"{"object":"0x12","sourceMap":null,"sourceMap":null}"#,
            r#"{"object":"0x12","immutableReferences":null}"#,
            r#"{"immutableReferences":{"1":[{"start":0,"length":32}]}}"#,
            r#"["0x12",null,{},{}]"#,
            "true",
            "123",
            "[]",
        ] {
            let json = format!(r#"{{"abi":[],"deployedBytecode":{deployed},"id":7}}"#);
            let expected = serde_json::from_str::<ConfigurableContractArtifact>(&json);
            let actual = ConfigurableContractArtifact::from_json(&json);
            match expected {
                Ok(expected) => assert_eq!(actual.unwrap(), expected, "{json}"),
                Err(expected) => {
                    assert_eq!(actual.unwrap_err().to_string(), expected.to_string(), "{json}")
                }
            }
        }
        for json in [
            "{}",
            "null",
            "[]",
            "{} trailing",
            r#"{"abi":[],"abi":[]}"#,
            r#"{"deployedBytecode":null,"deployedBytecode":null}"#,
        ] {
            let expected = serde_json::from_str::<ConfigurableContractArtifact>(json);
            let actual = ConfigurableContractArtifact::from_json(json);
            match expected {
                Ok(expected) => assert_eq!(actual.unwrap(), expected),
                Err(expected) => assert_eq!(actual.unwrap_err().to_string(), expected.to_string()),
            }
        }
    }
}
