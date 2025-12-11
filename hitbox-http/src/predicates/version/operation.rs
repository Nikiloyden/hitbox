use http::Version;

/// Operations for matching HTTP versions
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    /// Match exact HTTP version
    Eq(Version),
    /// Match if version is in the list
    In(Vec<Version>),
}

impl Operation {
    /// Check if the operation matches the given version
    pub fn check(&self, version: Version) -> bool {
        match self {
            Operation::Eq(expected) => version == *expected,
            Operation::In(versions) => versions.contains(&version),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eq_matches() {
        let op = Operation::Eq(Version::HTTP_11);
        assert!(op.check(Version::HTTP_11));
        assert!(!op.check(Version::HTTP_2));
    }

    #[test]
    fn test_in_matches() {
        let op = Operation::In(vec![Version::HTTP_11, Version::HTTP_2]);
        assert!(op.check(Version::HTTP_11));
        assert!(op.check(Version::HTTP_2));
        assert!(!op.check(Version::HTTP_10));
    }
}
