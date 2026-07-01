use crate::event::{EarthquakeEvent, EventKind, Intensity};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationMessage {
    pub title: String,
    pub body: String,
}

pub fn notification_message(event: &EarthquakeEvent) -> NotificationMessage {
    let title = if event.is_cancelled {
        format!("{} キャンセル", event.display_kind())
    } else if event.is_final {
        format!("{} 最終報", event.display_kind())
    } else {
        event.display_kind().to_owned()
    };

    let mut lines = Vec::new();
    push_if_some(
        &mut lines,
        "最大震度",
        event.max_intensity.map(|value| value.to_string()),
    );
    push_if_some(
        &mut lines,
        "震源",
        event.hypocenter.as_ref().map(|value| match value.depth_km {
            Some(depth) => format!("{} 深さ{}km", value.name, depth),
            None => value.name.clone(),
        }),
    );
    push_if_some(
        &mut lines,
        "M",
        event.magnitude.map(|value| format!("{:.1}", value.0)),
    );

    if !event.affected_areas.is_empty() {
        lines.push(format!("地域: {}", event.affected_areas.join("、")));
    }

    push_if_some(&mut lines, "発表", event.announced_at.clone());

    let body = if lines.is_empty() {
        "詳細情報はまだありません。".to_owned()
    } else {
        lines.join("\n")
    };

    NotificationMessage { title, body }
}

pub fn speech_text(event: &EarthquakeEvent) -> String {
    if event.is_cancelled {
        return format!("{}はキャンセルされました。", event.display_kind());
    }

    let mut parts = Vec::new();
    parts.push(event.display_kind().to_owned());

    if event.is_final {
        parts.push("最終報です".to_owned());
    }

    if let Some(intensity) = event.max_intensity {
        parts.push(intensity_speech(event.kind, intensity));
    }

    if let Some(hypocenter) = event.hypocenter.as_ref() {
        parts.push(format!("震源は{}", hypocenter.name));
    }

    if let Some(magnitude) = event.magnitude {
        parts.push(format!("マグニチュード{:.1}", magnitude.0));
    }

    /* 長すぎるので一旦コメントアウトする
    if !event.affected_areas.is_empty() {
        parts.push(format!("対象地域は{}", event.affected_areas.join("、")));
    }
    */

    format!("{}。", parts.join("、"))
}

fn push_if_some(lines: &mut Vec<String>, label: &str, value: Option<String>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        lines.push(format!("{label}: {value}"));
    }
}

fn intensity_speech(kind: EventKind, intensity: Intensity) -> String {
    if matches!(kind, EventKind::EewForecast) {
        format!("最大震度{}程度", intensity)
    } else {
        format!("最大震度{}", intensity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{EventKind, Hypocenter, Intensity, Magnitude};

    fn sample_event() -> EarthquakeEvent {
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
            max_intensity: Some(Intensity::Four),
            magnitude: Some(Magnitude(5.2)),
            affected_areas: vec!["東京都".to_owned(), "神奈川県".to_owned()],
        }
    }

    #[test]
    fn builds_notification_message() {
        let message = notification_message(&sample_event());

        assert_eq!(message.title, "地震情報");
        assert!(message.body.contains("最大震度: 4"));
        assert!(message.body.contains("震源: 東京都23区 深さ40km"));
        assert!(message.body.contains("M: 5.2"));
    }

    #[test]
    fn builds_speech_text() {
        let text = speech_text(&sample_event());

        assert_eq!(
            text,
            "地震情報、最大震度4、震源は東京都23区、マグニチュード5.2。"
        );
    }

    #[test]
    fn builds_cancel_speech_text() {
        let mut event = sample_event();
        event.is_cancelled = true;

        assert_eq!(speech_text(&event), "地震情報はキャンセルされました。");
    }

    #[test]
    fn forecast_speech_uses_estimate_wording() {
        let mut event = sample_event();
        event.kind = EventKind::EewForecast;
        event.max_intensity = Some(Intensity::FiveLower);

        assert_eq!(
            speech_text(&event),
            "緊急地震速報（予報）、最大震度5弱程度、震源は東京都23区、マグニチュード5.2。"
        );
    }
}
