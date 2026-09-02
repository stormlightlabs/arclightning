use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ulid::Ulid;

/// Errors returned when parsing a prefixed Arc Lightning identifier.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum IdError {
    #[error("invalid {entity} ID `{value}`; expected prefix `{expected}`")]
    InvalidPrefix {
        entity: &'static str,
        expected: &'static str,
        value: String,
    },
    #[error("invalid ULID suffix for {entity} ID `{value}`: {reason}")]
    InvalidUlid {
        entity: &'static str,
        value: String,
        reason: String,
    },
    #[error("{entity} ID `{value}` is not in canonical ULID form")]
    NonCanonical { entity: &'static str, value: String },
}

macro_rules! define_id {
    ($name:ident, $entity:literal, $prefix:literal) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(Ulid);

        impl $name {
            pub const PREFIX: &'static str = $prefix;

            pub fn new() -> Self {
                Self(Ulid::generate())
            }

            pub fn parse(value: &str) -> Result<Self, IdError> {
                let suffix = value
                    .strip_prefix(Self::PREFIX)
                    .ok_or_else(|| IdError::InvalidPrefix {
                        entity: $entity,
                        expected: Self::PREFIX,
                        value: value.to_owned(),
                    })?;
                let ulid = Ulid::from_string(suffix).map_err(|error| IdError::InvalidUlid {
                    entity: $entity,
                    value: value.to_owned(),
                    reason: error.to_string(),
                })?;
                if ulid.to_string() != suffix {
                    return Err(IdError::NonCanonical { entity: $entity, value: value.to_owned() });
                }
                Ok(Self(ulid))
            }

            pub const fn from_ulid(ulid: Ulid) -> Self {
                Self(ulid)
            }

            pub const fn ulid(self) -> Ulid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "{}{}", Self::PREFIX, self.0)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str(&self.to_string())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::parse(&value).map_err(serde::de::Error::custom)
            }
        }
    };
}

define_id!(IdeaId, "idea", "arcl-i-");
define_id!(ReleaseId, "release", "arcl-r-");
define_id!(EpicId, "epic", "arcl-e-");
define_id!(MilestoneId, "milestone", "arcl-m-");
define_id!(TaskId, "task", "arcl-t-");
define_id!(ProjectId, "project", "arcl-pj-");
define_id!(CaptureId, "capture", "arcl-c-");
define_id!(SpecId, "spec", "arcl-s-");
define_id!(PlanId, "plan", "arcl-pl-");
define_id!(PhaseId, "phase", "arcl-ph-");
define_id!(NoteId, "note", "arcl-n-");

#[cfg(test)]
mod tests {
    use super::{IdError, IdeaId};

    #[test]
    fn ids_round_trip_with_their_full_prefix() {
        let id = IdeaId::new();
        let encoded = id.to_string();
        assert!(encoded.starts_with(IdeaId::PREFIX));
        assert_eq!(IdeaId::parse(&encoded), Ok(id));
    }

    #[test]
    fn ids_reject_wrong_prefixes() {
        let error = IdeaId::parse("arcl-t-01ARZ3NDEKTSV4RRFFQ69G5FAV").expect_err("prefix is wrong");
        assert!(matches!(error, IdError::InvalidPrefix { .. }));
    }
}
