use std::{fmt::Display, str::FromStr};

#[cfg(all(test, feature = "arbitrary"))]
use proptest_derive::Arbitrary;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// A unique, immutable, stable identifier for a `Note`.
///
// NOTE: I haven't looked into alternative ID schemes yet, I just figured UUIDs
// are a sane default to reach for.
// TODO: Look into whether there are more human-friendly (i.e., readable and
// writeable) alternatives.
#[cfg_attr(all(test, feature = "arbitrary"), derive(Arbitrary))]
#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct Identifier(Uuid);

impl Identifier {
    pub fn new() -> Self {
        let inner = Uuid::new_v4();
        Self(inner)
    }
}

impl Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Error)]
pub enum IdentifierError {
    #[error("Failed to parse {from} as UUID: {error}")]
    UuidParseError { from: String, error: uuid::Error },
}

impl FromStr for Identifier {
    type Err = IdentifierError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let inner = Uuid::from_str(s).map_err(|error| IdentifierError::UuidParseError {
            from: s.to_string(),
            error,
        })?;
        Ok(Identifier(inner))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        // Identifiers should round-trip through string formatting + parsing.
        fn identifier_round_trips_through_string(bytes in any::<[u8; 16]>()) {
            let uuid = Uuid::from_bytes(bytes);
            let id = Identifier(uuid);
            let parsed = Identifier::from_str(&id.to_string()).unwrap();

            prop_assert_eq!(parsed.to_string(), id.to_string());
        }

        #[test]
        // Invalid strings must fail to parse as identifiers.
        fn identifier_rejects_invalid_strings(s in ".{0,64}") {
            prop_assume!(Uuid::from_str(&s).is_err());
            let parsed = Identifier::from_str(&s);

            prop_assert!(parsed.is_err());
        }
    }
}
