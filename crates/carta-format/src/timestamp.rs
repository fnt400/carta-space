use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Datelike, FixedOffset, Local, SecondsFormat};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::TimestampError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(DateTime<FixedOffset>);

impl Timestamp {
    pub fn now_local() -> Self {
        Self(Local::now().fixed_offset())
    }

    pub fn year(self) -> i32 {
        self.0.year()
    }

    pub fn month(self) -> u32 {
        self.0.month()
    }

    pub fn as_datetime(self) -> DateTime<FixedOffset> {
        self.0
    }

    /// Returns the smallest representable timestamp after this one.
    pub fn successor(self) -> Self {
        Self(self.0 + chrono::Duration::nanoseconds(1))
    }
}

impl FromStr for Timestamp {
    type Err = TimestampError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        DateTime::parse_from_rfc3339(value)
            .map(Self)
            .map_err(|source| TimestampError::Invalid {
                value: value.to_owned(),
                source,
            })
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0.to_rfc3339_opts(SecondsFormat::AutoSi, true))
    }
}

impl Serialize for Timestamp {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}
