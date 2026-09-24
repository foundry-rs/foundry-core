//! Serde helpers.

use alloy_primitives::Bytes;
use serde::{Deserialize, Deserializer};
use std::borrow::Cow;

#[derive(Deserialize)]
#[serde(transparent)]
pub(crate) struct BorrowedString<'a>(#[serde(borrow)] pub(crate) Cow<'a, str>);

pub fn deserialize_bytes<'de, D>(d: D) -> Result<Bytes, D::Error>
where
    D: Deserializer<'de>,
{
    display_from_str::deserialize(d)
}

pub fn deserialize_opt_bytes<'de, D>(d: D) -> Result<Option<Bytes>, D::Error>
where
    D: Deserializer<'de>,
{
    display_from_str_opt::deserialize(d)
}

pub fn default_for_null<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

pub mod json_string_opt {
    use serde::{
        Deserialize, Deserializer, Serialize, Serializer,
        de::{self, DeserializeOwned},
    };

    pub fn serialize<T, S>(value: &Option<T>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: Serialize,
    {
        if let Some(value) = value {
            value.serialize(serializer)
        } else {
            serializer.serialize_none()
        }
    }

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
    where
        D: Deserializer<'de>,
        T: DeserializeOwned,
    {
        if let Some(s) = Option::<String>::deserialize(deserializer)? {
            if s.is_empty() {
                return Ok(None);
            }
            let value = serde_json::Value::String(s);
            serde_json::from_value(value).map_err(de::Error::custom).map(Some)
        } else {
            Ok(None)
        }
    }
}

/// deserializes empty json object `{}` as `None`
pub mod empty_json_object_opt {
    use serde::{
        Deserialize, Deserializer, Serialize, Serializer,
        de::{self, DeserializeOwned},
    };

    pub fn serialize<T, S>(value: &Option<T>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: Serialize,
    {
        if let Some(value) = value {
            value.serialize(serializer)
        } else {
            let empty = serde_json::Value::Object(Default::default());
            serde_json::Value::serialize(&empty, serializer)
        }
    }

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
    where
        D: Deserializer<'de>,
        T: DeserializeOwned,
    {
        let json = serde_json::Value::deserialize(deserializer)?;
        if json.is_null() {
            return Ok(None);
        }
        if json.as_object().map(|obj| obj.is_empty()).unwrap_or_default() {
            return Ok(None);
        }
        serde_json::from_value(json).map_err(de::Error::custom).map(Some)
    }
}

/// serde support for string
pub mod string_bytes {
    use super::BorrowedString;
    use serde::{Deserialize, Deserializer, Serializer};
    use std::borrow::Cow;

    pub fn serialize<S>(value: &String, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if value.starts_with("0x") {
            serializer.serialize_str(value.as_str())
        } else {
            serializer.collect_str(&format_args!("0x{value}"))
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<String, D::Error>
    where
        D: Deserializer<'de>,
    {
        let BorrowedString(value) = BorrowedString::deserialize(deserializer)?;
        Ok(match value {
            Cow::Borrowed(value) => value.strip_prefix("0x").unwrap_or(value).to_owned(),
            Cow::Owned(mut value) => {
                if value.starts_with("0x") {
                    value.drain(..2);
                }
                value
            }
        })
    }
}

pub mod display_from_str_opt {
    use serde::{Deserializer, Serializer, de::Visitor};
    use std::{fmt, marker::PhantomData, str::FromStr};

    pub fn serialize<T, S>(value: &Option<T>, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: fmt::Display,
        S: Serializer,
    {
        if let Some(value) = value {
            serializer.collect_str(value)
        } else {
            serializer.serialize_none()
        }
    }

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
    where
        D: Deserializer<'de>,
        T: FromStr,
        T::Err: fmt::Display,
    {
        struct Optional<T>(PhantomData<T>);

        impl<'de, T: FromStr<Err: fmt::Display>> Visitor<'de> for Optional<T> {
            type Value = Option<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("option")
            }

            fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(None)
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(None)
            }
            fn visit_some<D: Deserializer<'de>>(
                self,
                deserializer: D,
            ) -> Result<Self::Value, D::Error> {
                super::display_from_str::deserialize(deserializer).map(Some)
            }
        }

        deserializer.deserialize_option(Optional(PhantomData))
    }
}

pub mod display_from_str {
    use serde::{
        Deserializer, Serializer,
        de::{self, Visitor},
    };
    use std::{fmt, marker::PhantomData, str::FromStr};

    pub fn serialize<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: fmt::Display,
        S: Serializer,
    {
        serializer.collect_str(value)
    }

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
        T: FromStr,
        T::Err: fmt::Display,
    {
        struct Parse<T>(PhantomData<T>);

        impl<T: FromStr<Err: fmt::Display>> Visitor<'_> for Parse<T> {
            type Value = T;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a string")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<T, E> {
                value.parse().map_err(E::custom)
            }

            fn visit_bytes<E: de::Error>(self, value: &[u8]) -> Result<T, E> {
                let value = std::str::from_utf8(value)
                    .map_err(|_| E::invalid_value(de::Unexpected::Bytes(value), &self))?;
                self.visit_str(value)
            }
        }

        deserializer.deserialize_string(Parse(PhantomData))
    }
}

/// (De)serialize vec of tuples as map
pub mod tuple_vec_map {
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::DeserializeOwned};

    pub fn serialize<K, V, S>(data: &[(K, V)], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        K: Serialize,
        V: Serialize,
    {
        serializer.collect_map(data.iter().map(|x| (&x.0, &x.1)))
    }

    pub fn deserialize<'de, K, V, D>(deserializer: D) -> Result<Vec<(K, V)>, D::Error>
    where
        D: Deserializer<'de>,
        K: DeserializeOwned,
        V: DeserializeOwned,
    {
        use serde::de::{MapAccess, Visitor};
        use std::{fmt, marker::PhantomData};

        struct TupleVecMapVisitor<K, V> {
            marker: PhantomData<Vec<(K, V)>>,
        }

        impl<K, V> TupleVecMapVisitor<K, V> {
            pub const fn new() -> Self {
                Self { marker: PhantomData }
            }
        }

        impl<'de, K, V> Visitor<'de> for TupleVecMapVisitor<K, V>
        where
            K: Deserialize<'de>,
            V: Deserialize<'de>,
        {
            type Value = Vec<(K, V)>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a map")
            }

            #[inline]
            fn visit_unit<E>(self) -> Result<Vec<(K, V)>, E> {
                Ok(Vec::new())
            }

            #[inline]
            fn visit_map<T>(self, mut access: T) -> Result<Vec<(K, V)>, T::Error>
            where
                T: MapAccess<'de>,
            {
                let mut values =
                    Vec::with_capacity(std::cmp::min(access.size_hint().unwrap_or(0), 4096));

                while let Some((key, value)) = access.next_entry()? {
                    values.push((key, value));
                }

                Ok(values)
            }
        }

        deserializer.deserialize_map(TupleVecMapVisitor::new())
    }
}

/// Deserialize short sequences without reserving four elements for a single value.
pub fn deserialize_small_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct SmallVecVisitor<T>(std::marker::PhantomData<T>);

    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for SmallVecVisitor<T> {
        type Value = Vec<T>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a sequence")
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            let Some(first) = seq.next_element()? else { return Ok(Vec::new()) };
            let mut prefix = [Some(first), None, None, None];
            let mut len = 1;
            while len < prefix.len() {
                let Some(value) = seq.next_element()? else { break };
                prefix[len] = Some(value);
                len += 1;
            }
            let next = if len == prefix.len() { seq.next_element()? } else { None };
            let capacity = if next.is_some() {
                seq.size_hint().map_or(8, |n| n.saturating_add(5).clamp(5, 4096))
            } else {
                len
            };
            let mut values = Vec::with_capacity(capacity);
            values.extend(prefix.into_iter().flatten());
            if let Some(next) = next {
                values.push(next);
                while let Some(value) = seq.next_element()? {
                    values.push(value);
                }
            }
            Ok(values)
        }
    }

    deserializer.deserialize_seq(SmallVecVisitor(std::marker::PhantomData))
}

/// Build a map in bulk to avoid sparsely occupied nodes for sorted input.
pub fn deserialize_btree_map<'de, D, K, V>(
    deserializer: D,
) -> Result<std::collections::BTreeMap<K, V>, D::Error>
where
    D: Deserializer<'de>,
    K: Deserialize<'de> + Ord,
    V: Deserialize<'de>,
{
    struct MapVisitor<K, V>(std::marker::PhantomData<(K, V)>);

    impl<'de, K: Deserialize<'de> + Ord, V: Deserialize<'de>> serde::de::Visitor<'de>
        for MapVisitor<K, V>
    {
        type Value = std::collections::BTreeMap<K, V>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a map")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> Result<Self::Value, A::Error> {
            let mut entries = Vec::with_capacity(map.size_hint().unwrap_or(0).min(4096));
            while let Some(entry) = map.next_entry()? {
                entries.push(entry);
            }
            Ok(entries.into_iter().collect())
        }
    }

    deserializer.deserialize_map(MapVisitor(std::marker::PhantomData))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Small(#[serde(deserialize_with = "deserialize_small_vec")] Vec<String>);

    #[test]
    fn small_vectors_preserve_values_and_errors() {
        for len in 0..40 {
            let values = (0..len).map(|i| format!("value-{i}")).collect::<Vec<_>>();
            let json = serde_json::to_string(&values).unwrap();
            let parsed = serde_json::from_str::<Small>(&json).unwrap();
            assert_eq!(parsed.0, values);
            assert_eq!(
                serde_json::from_value::<Small>(serde_json::to_value(&values).unwrap()).unwrap().0,
                values
            );
            if len <= 4 {
                assert_eq!(parsed.0.capacity(), len);
            }
        }
        for json in [
            "null",
            "{}",
            "123",
            r#"["ok",false]"#,
            r#"["a","b","c","d",null]"#,
            r#"["a","b","c","d","e",{}]"#,
        ] {
            assert!(serde_json::from_str::<Small>(json).is_err());
        }
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Number(#[serde(deserialize_with = "display_from_str::deserialize")] u32);
    #[derive(Debug, Deserialize, PartialEq)]
    struct Optional(#[serde(deserialize_with = "display_from_str_opt::deserialize")] Option<u32>);
    #[derive(Debug, Deserialize, PartialEq)]
    struct HexString(#[serde(deserialize_with = "string_bytes::deserialize")] String);

    #[test]
    fn borrowed_string_helpers_preserve_formats() {
        assert_eq!(serde_json::from_str::<Number>(r#""12""#).unwrap(), Number(12));
        assert_eq!(serde_json::from_str::<Number>(r#""\u0031\u0032""#).unwrap(), Number(12));
        assert_eq!(serde_json::from_value::<Number>(serde_json::json!("12")).unwrap(), Number(12));
        assert_eq!(serde_json::from_str::<Optional>("null").unwrap(), Optional(None));
        assert_eq!(serde_json::from_str::<Optional>(r#""12""#).unwrap(), Optional(Some(12)));
        for json in ["12", "false", "{}", "[]", r#""invalid""#] {
            assert!(serde_json::from_str::<Number>(json).is_err());
            assert!(serde_json::from_str::<Optional>(json).is_err());
        }
        for text in ["", "0x", "0xabc", "abc", "0Xabc", "0x日本語"] {
            let expected = text.strip_prefix("0x").unwrap_or(text);
            assert_eq!(
                serde_json::from_str::<HexString>(&serde_json::to_string(text).unwrap()).unwrap().0,
                expected
            );
            assert_eq!(
                serde_json::from_value::<HexString>(serde_json::json!(text)).unwrap().0,
                expected
            );
        }
        let value = String::from("0xabcdef");
        let ptr = value.as_ptr();
        let parsed = string_bytes::deserialize(serde::de::value::StringDeserializer::<
            serde::de::value::Error,
        >::new(value))
        .unwrap();
        assert_eq!(parsed, "abcdef");
        assert_eq!(parsed.as_ptr(), ptr);
    }
}
