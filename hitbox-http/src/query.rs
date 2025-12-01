use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum Value {
    Scalar(String),
    Array(Vec<String>),
}

impl Value {
    pub fn inner(&self) -> Vec<String> {
        match self {
            Value::Scalar(value) => vec![value.to_owned()],
            Value::Array(values) => values.to_owned(),
        }
    }
    pub fn contains(&self, value: &String) -> bool {
        self.inner().contains(value)
    }
}

/// Parse a query string into a map of key-value pairs.
///
/// Returns `None` if the query string is malformed.
pub fn parse(value: &str) -> Option<HashMap<String, Value>> {
    serde_qs::Config::new(5, false).deserialize_str(value).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_one() {
        let hash_map = parse("key=value").unwrap();
        let value = hash_map.get("key").unwrap();
        assert_eq!(value.inner(), vec!["value"]);
    }

    #[test]
    fn test_parse_valid_multiple() {
        let hash_map = parse("key-one=value-one&key-two=value-two&key-three=value-three").unwrap();
        let value = hash_map.get("key-one").unwrap();
        assert_eq!(value.inner(), vec!["value-one"]);
        let value = hash_map.get("key-two").unwrap();
        assert_eq!(value.inner(), vec!["value-two"]);
        let value = hash_map.get("key-three").unwrap();
        assert_eq!(value.inner(), vec!["value-three"]);
    }

    #[test]
    fn test_parse_not_valid() {
        let hash_map = parse("   wrong   ").unwrap();
        assert_eq!(hash_map.len(), 1);
    }

    #[test]
    fn test_parse_exceeds_depth_returns_none() {
        // Nesting depth exceeds configured limit (5), should return None
        assert!(parse("a[b][c][d][e][f][g]=1").is_none());
    }

    #[test]
    fn test_parse_array_bracket_syntax() {
        // Note: serde_qs only supports bracket syntax for arrays (color[]=a&color[]=b)
        // Repeated keys without brackets (color=a&color=b) are not supported
        let hash_map = parse("color[]=red&color[]=blue&color[]=green").unwrap();
        let value = hash_map.get("color").unwrap();
        assert_eq!(value.inner(), vec!["red", "blue", "green"]);
    }
}
