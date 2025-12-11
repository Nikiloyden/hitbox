use hitbox::predicate::Predicate;
use hitbox_http::predicates::version::{
    HttpVersion, Operation as VersionOperation, VersionPredicate,
};
use http::Version;
use serde::{Deserialize, Serialize};

use crate::error::ConfigError;

/// HTTP version operation for predicates
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(untagged)]
pub enum VersionOperationConfig {
    /// Match exact HTTP version
    Eq(String),
    /// Match if version is in the list
    In(Vec<String>),
}

impl VersionOperationConfig {
    /// Convert to hitbox-http version operation
    pub fn into_operation(self) -> Result<VersionOperation, ConfigError> {
        match self {
            VersionOperationConfig::Eq(version) => {
                let v = parse_version(&version)?;
                Ok(VersionOperation::Eq(v))
            }
            VersionOperationConfig::In(versions) => {
                let vs = parse_versions(&versions)?;
                Ok(VersionOperation::In(vs))
            }
        }
    }
}

/// Convert version operation config into predicates for any subject that implements HasVersion
pub fn into_predicates<P>(
    operation: VersionOperationConfig,
    inner: P,
) -> Result<HttpVersion<P>, ConfigError>
where
    P: Predicate,
{
    let op = operation.into_operation()?;
    Ok(inner.version(op))
}

fn parse_version(version: &str) -> Result<Version, ConfigError> {
    match version.to_uppercase().as_str() {
        "HTTP/0.9" | "0.9" => Ok(Version::HTTP_09),
        "HTTP/1.0" | "1.0" => Ok(Version::HTTP_10),
        "HTTP/1.1" | "1.1" => Ok(Version::HTTP_11),
        "HTTP/2" | "HTTP/2.0" | "2" | "2.0" => Ok(Version::HTTP_2),
        "HTTP/3" | "HTTP/3.0" | "3" | "3.0" => Ok(Version::HTTP_3),
        _ => Err(ConfigError::InvalidVersion(version.to_string())),
    }
}

fn parse_versions(versions: &[String]) -> Result<Vec<Version>, ConfigError> {
    versions.iter().map(|v| parse_version(v)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version_full_format() {
        assert_eq!(parse_version("HTTP/1.1").unwrap(), Version::HTTP_11);
        assert_eq!(parse_version("HTTP/2").unwrap(), Version::HTTP_2);
        assert_eq!(parse_version("HTTP/3").unwrap(), Version::HTTP_3);
    }

    #[test]
    fn test_parse_version_short_format() {
        assert_eq!(parse_version("1.1").unwrap(), Version::HTTP_11);
        assert_eq!(parse_version("2").unwrap(), Version::HTTP_2);
        assert_eq!(parse_version("2.0").unwrap(), Version::HTTP_2);
    }

    #[test]
    fn test_parse_version_case_insensitive() {
        assert_eq!(parse_version("http/1.1").unwrap(), Version::HTTP_11);
        assert_eq!(parse_version("Http/2").unwrap(), Version::HTTP_2);
    }

    #[test]
    fn test_parse_version_invalid() {
        assert!(parse_version("HTTP/4").is_err());
        assert!(parse_version("invalid").is_err());
    }
}
