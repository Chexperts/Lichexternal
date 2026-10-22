//! Chess variants supported by Lichess and the External Engine API.

use serde::{Deserialize, Serialize};

/// Chess variants recognized by the Lichess UCI protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum UciVariant {
    /// Standard chess.
    Chess,
    /// Crazyhouse variant.
    Crazyhouse,
    /// Antichess / Give-away chess.
    Antichess,
    /// Atomic chess.
    Atomic,
    /// Horde chess.
    Horde,
    /// King of the Hill variant.
    #[serde(rename = "kingOfTheHill")]
    KingOfTheHill,
    /// Racing Kings variant.
    #[serde(rename = "racingKings")]
    RacingKings,
    /// Three-Check chess.
    #[serde(rename = "3check")]
    ThreeCheck,
}

impl UciVariant {
    /// Returns the UCI string representation of the variant for the `UCI_Variant` option.
    #[must_use]
    pub const fn as_uci_str(&self) -> &'static str {
        match self {
            Self::Chess => "chess",
            Self::Crazyhouse => "crazyhouse",
            Self::Antichess => "antichess",
            Self::Atomic => "atomic",
            Self::Horde => "horde",
            Self::KingOfTheHill => "kingofthehill",
            Self::RacingKings => "racingkings",
            Self::ThreeCheck => "3check",
        }
    }
}

impl core::fmt::Display for UciVariant {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.as_uci_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variant_serialization() {
        assert_eq!(serde_json::to_string(&UciVariant::Chess).unwrap(), "\"chess\"");
        assert_eq!(serde_json::to_string(&UciVariant::KingOfTheHill).unwrap(), "\"kingOfTheHill\"");
        assert_eq!(serde_json::to_string(&UciVariant::RacingKings).unwrap(), "\"racingKings\"");
        assert_eq!(serde_json::to_string(&UciVariant::ThreeCheck).unwrap(), "\"3check\"");
    }

    #[test]
    fn test_variant_deserialization() {
        assert_eq!(serde_json::from_str::<UciVariant>("\"chess\"").unwrap(), UciVariant::Chess);
        assert_eq!(serde_json::from_str::<UciVariant>("\"atomic\"").unwrap(), UciVariant::Atomic);
        assert_eq!(serde_json::from_str::<UciVariant>("\"kingOfTheHill\"").unwrap(), UciVariant::KingOfTheHill);
        assert_eq!(serde_json::from_str::<UciVariant>("\"3check\"").unwrap(), UciVariant::ThreeCheck);
    }
}
