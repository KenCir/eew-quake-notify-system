use crate::{
    config::{NotifyConfig, TtsConfig},
    event::{EarthquakeEvent, Intensity},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryDecision {
    Send,
    Skip(DeliverySkipReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliverySkipReason {
    Disabled,
    DisabledKind,
    BelowMinIntensity,
    UnknownIntensity,
    Duplicate,
}

pub fn should_notify(event: &EarthquakeEvent, config: &NotifyConfig) -> DeliveryDecision {
    if !config.desktop_enabled {
        return DeliveryDecision::Skip(DeliverySkipReason::Disabled);
    }

    if !config.enabled_kinds.contains(&event.kind) {
        return DeliveryDecision::Skip(DeliverySkipReason::DisabledKind);
    }

    passes_intensity_rule(event, config.min_intensity)
}

pub fn should_speak(event: &EarthquakeEvent, config: &TtsConfig) -> DeliveryDecision {
    if !config.enabled {
        return DeliveryDecision::Skip(DeliverySkipReason::Disabled);
    }

    if !config.enabled_kinds.contains(&event.kind) {
        return DeliveryDecision::Skip(DeliverySkipReason::DisabledKind);
    }

    passes_intensity_rule(event, config.min_intensity)
}

impl DeliverySkipReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::DisabledKind => "disabled_kind",
            Self::BelowMinIntensity => "below_min_intensity",
            Self::UnknownIntensity => "unknown_intensity",
            Self::Duplicate => "duplicate",
        }
    }
}

fn passes_intensity_rule(
    event: &EarthquakeEvent,
    min_intensity: Option<Intensity>,
) -> DeliveryDecision {
    if event.is_cancelled {
        return DeliveryDecision::Send;
    }

    let Some(min_intensity) = min_intensity else {
        return DeliveryDecision::Send;
    };

    match event.max_intensity {
        Some(Intensity::Unknown) | None => {
            DeliveryDecision::Skip(DeliverySkipReason::UnknownIntensity)
        }
        Some(max_intensity) if max_intensity >= min_intensity => DeliveryDecision::Send,
        Some(_) => DeliveryDecision::Skip(DeliverySkipReason::BelowMinIntensity),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{EventKind, Hypocenter, Magnitude};

    fn event(intensity: Option<Intensity>) -> EarthquakeEvent {
        EarthquakeEvent {
            source_id: Some("event-1".to_owned()),
            kind: EventKind::Earthquake,
            serial: Some(1),
            is_final: false,
            is_cancelled: false,
            occurred_at: Some("2026-06-26T12:00:00+09:00".to_owned()),
            announced_at: Some("2026-06-26T12:01:00+09:00".to_owned()),
            hypocenter: Some(Hypocenter {
                name: "東京都23区".to_owned(),
                depth_km: Some(40),
            }),
            max_intensity: intensity,
            magnitude: Some(Magnitude(5.2)),
            affected_areas: vec!["東京都".to_owned()],
        }
    }

    fn notify_config(enabled: bool, min_intensity: Option<Intensity>) -> NotifyConfig {
        NotifyConfig {
            desktop_enabled: enabled,
            enabled_kinds: default_enabled_kinds(),
            min_intensity,
        }
    }

    fn tts_config(enabled: bool, min_intensity: Option<Intensity>) -> TtsConfig {
        TtsConfig {
            enabled,
            engine: "voicevox".to_owned(),
            voicevox_url: "http://127.0.0.1:50021".to_owned(),
            speaker: 1,
            enabled_kinds: default_enabled_kinds(),
            min_intensity,
        }
    }

    fn default_enabled_kinds() -> Vec<EventKind> {
        vec![
            EventKind::Earthquake,
            EventKind::IntensityReport,
            EventKind::EewWarning,
        ]
    }

    #[test]
    fn notify_sends_when_enabled_and_intensity_matches() {
        assert_eq!(
            should_notify(
                &event(Some(Intensity::Four)),
                &notify_config(true, Some(Intensity::Three))
            ),
            DeliveryDecision::Send
        );
    }

    #[test]
    fn notify_skips_when_disabled() {
        assert_eq!(
            should_notify(&event(Some(Intensity::Four)), &notify_config(false, None)),
            DeliveryDecision::Skip(DeliverySkipReason::Disabled)
        );
    }

    #[test]
    fn notify_skips_below_threshold() {
        assert_eq!(
            should_notify(
                &event(Some(Intensity::Two)),
                &notify_config(true, Some(Intensity::Three))
            ),
            DeliveryDecision::Skip(DeliverySkipReason::BelowMinIntensity)
        );
    }

    #[test]
    fn notify_skips_unknown_intensity_when_threshold_exists() {
        assert_eq!(
            should_notify(&event(None), &notify_config(true, Some(Intensity::Three))),
            DeliveryDecision::Skip(DeliverySkipReason::UnknownIntensity)
        );
    }

    #[test]
    fn cancel_event_ignores_intensity_threshold() {
        let mut event = event(None);
        event.is_cancelled = true;

        assert_eq!(
            should_notify(&event, &notify_config(true, Some(Intensity::Seven))),
            DeliveryDecision::Send
        );
    }

    #[test]
    fn disabled_kind_skips_before_intensity_rules() {
        let mut event = event(Some(Intensity::Seven));
        event.kind = EventKind::EewForecast;

        assert_eq!(
            should_notify(&event, &notify_config(true, Some(Intensity::One))),
            DeliveryDecision::Skip(DeliverySkipReason::DisabledKind)
        );
    }

    #[test]
    fn cancel_event_does_not_bypass_disabled_kind() {
        let mut event = event(None);
        event.kind = EventKind::EewForecast;
        event.is_cancelled = true;

        assert_eq!(
            should_notify(&event, &notify_config(true, Some(Intensity::Seven))),
            DeliveryDecision::Skip(DeliverySkipReason::DisabledKind)
        );
    }

    #[test]
    fn tts_uses_own_config() {
        assert_eq!(
            should_speak(
                &event(Some(Intensity::Three)),
                &tts_config(true, Some(Intensity::Four))
            ),
            DeliveryDecision::Skip(DeliverySkipReason::BelowMinIntensity)
        );
    }
}
