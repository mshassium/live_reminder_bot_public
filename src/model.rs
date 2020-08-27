use std::fmt;
use std::fmt::Formatter;
use serde::{Deserialize, Serialize};
use mongodb::{bson::{doc, Bson, Array}, bson};

pub(crate) struct UserPhrase {
    words: Vec<Bson>
}

impl UserPhrase {
    pub fn new(arr: &Array) -> UserPhrase {
        UserPhrase {
            words: arr.clone()
        }
    }
    pub fn from_str(single_str: &str) -> UserPhrase {
        UserPhrase {
            words: vec![bson::Bson::from(single_str)]
        }
    }
}

impl fmt::Display for UserPhrase {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "\n🔸🔸🔸🔸🔸🔸🔸🔸🔸\n\n")?;
        for (pos, v) in self.words.iter().enumerate() {
            write!(f, "{}) {}", pos + 1, v)?;
            write!(f, "\n\n")?;
        }
        write!(f, "🔸🔸🔸🔸🔸🔸🔸🔸🔸")
    }
}

#[derive(Serialize, Deserialize)]
pub struct TimeZoneRawData {
    pub status: String,
    pub message: String,
    #[serde(rename = "countryCode")]
    pub country_code: String,
    #[serde(rename = "countryName")]
    pub country_name: String,
    #[serde(rename = "zoneName")]
    pub zone_name: String,
    pub timestamp: u64,
    pub formatted: String,
}

#[derive(Serialize, Deserialize)]
pub struct ConvertedTimeZone {
    pub status: String,
    #[serde(rename = "fromZoneName")]
    pub from_zone_name: String,
    #[serde(rename = "toZoneName")]
    pub to_zone_name: String,
    #[serde(rename = "fromTimestamp")]
    pub from_timestamp: u64,
    #[serde(rename = "toTimestamp")]
    pub to_timestamp: u64,
    pub offset: i64,
}
