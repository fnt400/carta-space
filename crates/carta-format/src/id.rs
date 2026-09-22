use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::{Uuid, Version};

use crate::IdError;

macro_rules! define_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(Uuid);

        impl $name {
            pub fn new_v7() -> Self {
                Self(Uuid::now_v7())
            }

            pub fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl FromStr for $name {
            type Err = IdError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                let uuid = Uuid::parse_str(value).map_err(|source| IdError::InvalidSyntax {
                    value: value.to_owned(),
                    source,
                })?;

                if uuid.hyphenated().to_string() != value {
                    return Err(IdError::NonCanonical(value.to_owned()));
                }
                if uuid.get_version() != Some(Version::SortRand) {
                    return Err(IdError::NotVersion7(value.to_owned()));
                }

                Ok(Self(uuid))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.hyphenated().fmt(formatter)
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

define_id!(ArchiveId);
define_id!(DocumentId);
define_id!(WorkId);
