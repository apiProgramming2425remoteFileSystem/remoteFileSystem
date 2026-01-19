use serde::{Deserialize, Deserializer, de};
use std::fmt;
use std::marker::PhantomData;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

/// This is used to ensure consistent path representations.
pub fn normalize_path<P: AsRef<Path>>(path: P) -> PathBuf {
    let mut ret = PathBuf::new();

    for component in path.as_ref().components() {
        match component {
            Component::Prefix(prefix) => ret.push(prefix.as_os_str()),
            Component::RootDir => ret.push(Component::RootDir.as_os_str()),
            Component::CurDir => {} // Ignore "."
            Component::ParentDir => {
                ret.pop(); // Handle ".." by removing the previous component
            }
            Component::Normal(c) => ret.push(c),
        }
    }
    ret
}

/// Normalize an optional path
pub fn normalize_optional_path<P: AsRef<Path>>(path: &Option<P>) -> Option<PathBuf> {
    path.as_ref().map(normalize_path)
}

/// Helper to deserialize a Vec<T> from:
/// 1. A comma-separated string (from ENV) -> "a,b"
/// 2. A standard sequence (from TOML/JSON) -> ["a", "b"]
/// 3. A numbered map (from legacy ENV) -> {"0": "a"}
pub fn deserialize_flexible_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + FromStr,
    <T as FromStr>::Err: fmt::Display,
{
    struct FlexibleVecVisitor<T>(PhantomData<T>);

    impl<'de, T> de::Visitor<'de> for FlexibleVecVisitor<T>
    where
        T: Deserialize<'de> + FromStr,
        <T as FromStr>::Err: fmt::Display,
    {
        type Value = Vec<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a sequence, a comma-separated string, or a map")
        }

        // Handle comma-separated strings (e.g. from ENV variables)
        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if v.is_empty() {
                return Ok(Vec::new());
            }
            v.split(',')
                .map(|s| s.trim())
                .map(|s| T::from_str(s).map_err(E::custom))
                .collect()
        }

        // Handle standard sequences (e.g. from JSON arrays)
        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            // Optimization: pre-allocate vector if size is known
            let mut vec = Vec::with_capacity(seq.size_hint().unwrap_or(0));

            while let Some(elem) = seq.next_element()? {
                vec.push(elem);
            }
            Ok(vec)
        }

        // Handle legacy maps (e.g. config-rs hierarchical env vars like "ARR__0=a")
        // Note: This ignores keys and effectively flattens the map into a list.
        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: de::MapAccess<'de>,
        {
            let mut vec = Vec::with_capacity(map.size_hint().unwrap_or(0));

            // We discard the key (_key) and only collect values
            while let Some((_key, value)) = map.next_entry::<String, T>()? {
                vec.push(value);
            }
            Ok(vec)
        }
    }

    deserializer.deserialize_any(FlexibleVecVisitor(PhantomData))
}
