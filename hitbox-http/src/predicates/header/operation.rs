use http::{HeaderMap, HeaderName, HeaderValue};
use regex::Regex;

#[derive(Debug)]
pub enum Operation {
    Eq(HeaderName, HeaderValue),
    Exist(HeaderName),
    In(HeaderName, Vec<HeaderValue>),
    Contains(HeaderName, String),
    Regex(HeaderName, Regex),
}

impl Operation {
    /// Check if the operation matches the headers.
    pub fn check(&self, headers: &HeaderMap) -> bool {
        match self {
            Operation::Eq(name, value) => headers
                .get_all(name)
                .iter()
                .any(|header_value| value.eq(header_value)),
            Operation::Exist(name) => headers.get(name).is_some(),
            Operation::In(name, values) => headers
                .get_all(name)
                .iter()
                .any(|header_value| values.iter().any(|v| v.eq(header_value))),
            Operation::Contains(name, substring) => {
                headers.get_all(name).iter().any(|header_value| {
                    header_value
                        .to_str()
                        .map(|s| s.contains(substring.as_str()))
                        .unwrap_or(false)
                })
            }
            Operation::Regex(name, regex) => headers.get_all(name).iter().any(|header_value| {
                header_value
                    .to_str()
                    .map(|s| regex.is_match(s))
                    .unwrap_or(false)
            }),
        }
    }
}
