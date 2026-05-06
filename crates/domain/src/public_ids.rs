use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParsePublicIdError {
    #[error("missing required prefix `{expected}`")]
    MissingPrefix { expected: &'static str },
    #[error("invalid uuid")]
    InvalidUuid(#[source] uuid::Error),
}

macro_rules! public_id_type {
    ($name:ident, $prefix:literal, $example:literal) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, ToSchema)]
        #[schema(value_type = String, example = $example)]
        pub struct $name(Uuid);

        impl $name {
            pub const PREFIX: &'static str = $prefix;

            pub const fn new(value: Uuid) -> Self {
                Self(value)
            }

            pub const fn into_uuid(self) -> Uuid {
                self.0
            }

            pub const fn as_uuid(&self) -> &Uuid {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}{}", Self::PREFIX, self.0)
            }
        }

        impl From<Uuid> for $name {
            fn from(value: Uuid) -> Self {
                Self::new(value)
            }
        }

        impl From<$name> for Uuid {
            fn from(value: $name) -> Self {
                value.into_uuid()
            }
        }

        impl FromStr for $name {
            type Err = ParsePublicIdError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                let uuid = value
                    .strip_prefix(Self::PREFIX)
                    .ok_or(ParsePublicIdError::MissingPrefix {
                        expected: Self::PREFIX,
                    })
                    .and_then(|raw| {
                        Uuid::parse_str(raw).map_err(ParsePublicIdError::InvalidUuid)
                    })?;

                Ok(Self::new(uuid))
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.to_string())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                value.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}

public_id_type!(
    BoardProjectId,
    "bp_",
    "bp_123e4567-e89b-12d3-a456-426614174000"
);
public_id_type!(BoardRunId, "br_", "br_123e4567-e89b-12d3-a456-426614174000");
public_id_type!(
    ArtifactId,
    "art_",
    "art_123e4567-e89b-12d3-a456-426614174000"
);
public_id_type!(
    ArtifactBundleId,
    "ab_",
    "ab_123e4567-e89b-12d3-a456-426614174000"
);

#[cfg(test)]
mod tests {
    use super::{ArtifactBundleId, ArtifactId, BoardProjectId, BoardRunId, ParsePublicIdError};
    use uuid::Uuid;

    fn sample_uuid() -> Uuid {
        Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap()
    }

    #[test]
    fn board_project_id_round_trip() {
        let id = BoardProjectId::new(sample_uuid());
        assert_eq!(id.to_string(), "bp_123e4567-e89b-12d3-a456-426614174000");
        assert_eq!(
            "bp_123e4567-e89b-12d3-a456-426614174000"
                .parse::<BoardProjectId>()
                .unwrap(),
            id
        );
    }

    #[test]
    fn board_run_id_round_trip() {
        let id = BoardRunId::new(sample_uuid());
        assert_eq!(id.to_string(), "br_123e4567-e89b-12d3-a456-426614174000");
        assert_eq!(
            "br_123e4567-e89b-12d3-a456-426614174000"
                .parse::<BoardRunId>()
                .unwrap(),
            id
        );
    }

    #[test]
    fn artifact_id_round_trip() {
        let id = ArtifactId::new(sample_uuid());
        assert_eq!(id.to_string(), "art_123e4567-e89b-12d3-a456-426614174000");
        assert_eq!(
            "art_123e4567-e89b-12d3-a456-426614174000"
                .parse::<ArtifactId>()
                .unwrap(),
            id
        );
    }

    #[test]
    fn artifact_bundle_id_round_trip() {
        let id = ArtifactBundleId::new(sample_uuid());
        assert_eq!(id.to_string(), "ab_123e4567-e89b-12d3-a456-426614174000");
        assert_eq!(
            "ab_123e4567-e89b-12d3-a456-426614174000"
                .parse::<ArtifactBundleId>()
                .unwrap(),
            id
        );
    }

    #[test]
    fn rejects_wrong_prefix() {
        let err = "br_123e4567-e89b-12d3-a456-426614174000"
            .parse::<BoardProjectId>()
            .unwrap_err();
        assert!(matches!(
            err,
            ParsePublicIdError::MissingPrefix { expected: "bp_" }
        ));
    }

    #[test]
    fn rejects_missing_prefix() {
        let err = "123e4567-e89b-12d3-a456-426614174000"
            .parse::<BoardRunId>()
            .unwrap_err();
        assert!(matches!(
            err,
            ParsePublicIdError::MissingPrefix { expected: "br_" }
        ));
    }

    #[test]
    fn rejects_invalid_uuid() {
        let err = "art_not-a-uuid".parse::<ArtifactId>().unwrap_err();
        assert!(matches!(err, ParsePublicIdError::InvalidUuid(_)));
    }
}
