use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, de};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct EarthquakeEvent {
    pub source_id: Option<String>,
    pub kind: EventKind,
    pub serial: Option<u32>,
    pub is_final: bool,
    pub is_cancelled: bool,
    pub occurred_at: Option<String>,
    pub announced_at: Option<String>,
    pub hypocenter: Option<Hypocenter>,
    pub max_intensity: Option<Intensity>,
    pub magnitude: Option<Magnitude>,
    pub affected_areas: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    EewWarning,
    EewForecast,
    Earthquake,
    IntensityReport,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hypocenter {
    pub name: String,
    pub depth_km: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Magnitude(pub f32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Intensity {
    Zero,
    One,
    Two,
    Three,
    Four,
    FiveLower,
    FiveUpper,
    SixLower,
    SixUpper,
    Seven,
    Unknown,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("invalid seismic intensity: {value}")]
pub struct ParseIntensityError {
    value: String,
}

impl EarthquakeEvent {
    pub fn display_kind(&self) -> &'static str {
        match self.kind {
            EventKind::EewWarning => "緊急地震速報（警報）",
            EventKind::EewForecast => "緊急地震速報（予報）",
            EventKind::Earthquake => "地震情報",
            EventKind::IntensityReport => "震度速報",
            EventKind::Unknown => "地震関連情報",
        }
    }

    pub fn dedup_key(&self) -> String {
        if let Some(source_id) = self.source_id.as_deref() {
            return source_id.to_owned();
        }

        format!(
            "{:?}:{:?}:{:?}:{:?}:{:?}",
            self.kind,
            self.occurred_at,
            self.announced_at,
            self.hypocenter.as_ref().map(|hypocenter| &hypocenter.name),
            self.max_intensity
        )
    }
}

impl EventKind {
    pub fn config_key(self) -> &'static str {
        match self {
            Self::Earthquake => "earthquake",
            Self::IntensityReport => "intensity_report",
            Self::EewWarning => "eew_warning",
            Self::EewForecast => "eew_forecast",
            Self::Unknown => "unknown",
        }
    }
}

impl Magnitude {
    pub fn new(value: f32) -> Option<Self> {
        if value.is_finite() && value >= 0.0 {
            Some(Self(value))
        } else {
            None
        }
    }
}

impl Intensity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Zero => "0",
            Self::One => "1",
            Self::Two => "2",
            Self::Three => "3",
            Self::Four => "4",
            Self::FiveLower => "5弱",
            Self::FiveUpper => "5強",
            Self::SixLower => "6弱",
            Self::SixUpper => "6強",
            Self::Seven => "7",
            Self::Unknown => "不明",
        }
    }
}

impl fmt::Display for Intensity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

impl FromStr for Intensity {
    type Err = ParseIntensityError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let normalized = value.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "0" => Ok(Self::Zero),
            "1" => Ok(Self::One),
            "2" => Ok(Self::Two),
            "3" => Ok(Self::Three),
            "4" => Ok(Self::Four),
            "5-" | "5弱" | "5 lower" | "5_l" | "5l" => Ok(Self::FiveLower),
            "5+" | "5強" | "5 upper" | "5_u" | "5u" => Ok(Self::FiveUpper),
            "6-" | "6弱" | "6 lower" | "6_l" | "6l" => Ok(Self::SixLower),
            "6+" | "6強" | "6 upper" | "6_u" | "6u" => Ok(Self::SixUpper),
            "7" => Ok(Self::Seven),
            "unknown" | "不明" => Ok(Self::Unknown),
            _ => Err(ParseIntensityError {
                value: value.to_owned(),
            }),
        }
    }
}

impl<'de> Deserialize<'de> for Intensity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_intensity_labels() {
        assert_eq!("5-".parse::<Intensity>(), Ok(Intensity::FiveLower));
        assert_eq!("5弱".parse::<Intensity>(), Ok(Intensity::FiveLower));
        assert_eq!("6+".parse::<Intensity>(), Ok(Intensity::SixUpper));
        assert_eq!("7".parse::<Intensity>(), Ok(Intensity::Seven));
    }

    #[test]
    fn formats_japanese_intensity_labels() {
        assert_eq!(Intensity::FiveUpper.to_string(), "5強");
        assert_eq!(Intensity::Unknown.to_string(), "不明");
    }

    #[test]
    fn uses_source_id_as_dedup_key() {
        let event = EarthquakeEvent {
            source_id: Some("abc".to_owned()),
            kind: EventKind::Earthquake,
            serial: None,
            is_final: false,
            is_cancelled: false,
            occurred_at: None,
            announced_at: None,
            hypocenter: None,
            max_intensity: None,
            magnitude: None,
            affected_areas: Vec::new(),
        };

        assert_eq!(event.dedup_key(), "abc");
    }

    #[test]
    fn displays_eew_warning_and_forecast_separately() {
        let mut event = EarthquakeEvent {
            source_id: None,
            kind: EventKind::EewWarning,
            serial: None,
            is_final: false,
            is_cancelled: false,
            occurred_at: None,
            announced_at: None,
            hypocenter: None,
            max_intensity: None,
            magnitude: None,
            affected_areas: Vec::new(),
        };

        assert_eq!(event.display_kind(), "緊急地震速報（警報）");
        event.kind = EventKind::EewForecast;
        assert_eq!(event.display_kind(), "緊急地震速報（予報）");
    }
}
