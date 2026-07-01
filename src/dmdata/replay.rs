use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use thiserror::Error;

use super::dto::{TelegramHeadDto, WebSocketDataDto, WebSocketMessageDto, XmlReportDto};

const REPLAY_WEBSOCKET_VERSION: &str = "2.0";
const REPLAY_JSON_ENCODING: &str = "utf-8";

#[derive(Debug, Error)]
pub enum ReplayError {
    #[error("failed to read replay fixture {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse replay fixture JSON {path}: {source}")]
    ParseJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("replay fixture directory is missing telegrams.json: {0}")]
    MissingTelegramIndex(PathBuf),
    #[error("unsupported replay fixture file: {0}")]
    UnsupportedFile(PathBuf),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ControlPanelTelegramDto {
    id: String,
    classification: String,
    head: TelegramHeadDto,
    #[serde(default)]
    xml_report: Option<XmlReportDto>,
    format: Option<String>,
    filename: String,
}

pub fn load_replay_data(path: impl AsRef<Path>) -> Result<Vec<WebSocketDataDto>, ReplayError> {
    let path = path.as_ref();
    if path.is_dir() {
        return load_control_panel_directory(path);
    }

    load_websocket_data_file(path)
}

fn load_control_panel_directory(path: &Path) -> Result<Vec<WebSocketDataDto>, ReplayError> {
    let index_path = path.join("telegrams.json");
    if !index_path.exists() {
        return Err(ReplayError::MissingTelegramIndex(index_path));
    }

    let index_text = read_to_string(&index_path)?;
    let items: Vec<ControlPanelTelegramDto> =
        serde_json::from_str(&index_text).map_err(|source| ReplayError::ParseJson {
            path: index_path.clone(),
            source,
        })?;

    let mut data = Vec::new();
    for item in items {
        let body = if is_json_format(item.format.as_deref()) {
            let fixture_path = path.join(&item.filename);
            read_to_string(&fixture_path)?
        } else {
            String::new()
        };
        data.push(control_panel_item_to_websocket_data(item, body));
    }

    Ok(data)
}

fn load_websocket_data_file(path: &Path) -> Result<Vec<WebSocketDataDto>, ReplayError> {
    let text = read_to_string(path)?;

    if let Ok(message) = serde_json::from_str::<WebSocketMessageDto>(&text) {
        return match message {
            WebSocketMessageDto::Data(data) => Ok(vec![*data]),
            _ => Err(ReplayError::UnsupportedFile(path.to_owned())),
        };
    }

    serde_json::from_str::<WebSocketDataDto>(&text)
        .map(|data| vec![data])
        .map_err(|source| ReplayError::ParseJson {
            path: path.to_owned(),
            source,
        })
}

fn control_panel_item_to_websocket_data(
    item: ControlPanelTelegramDto,
    body: String,
) -> WebSocketDataDto {
    WebSocketDataDto {
        version: REPLAY_WEBSOCKET_VERSION.to_owned(),
        classification: item.classification,
        id: item.id,
        head: item.head,
        xml_report: item.xml_report,
        format: item.format,
        compression: None,
        encoding: Some(REPLAY_JSON_ENCODING.to_owned()),
        body,
    }
}

fn read_to_string(path: &Path) -> Result<String, ReplayError> {
    fs::read_to_string(path).map_err(|source| ReplayError::Read {
        path: path.to_owned(),
        source,
    })
}

fn is_json_format(format: Option<&str>) -> bool {
    format
        .map(|format| format.eq_ignore_ascii_case("json"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dmdata::websocket_data_to_event;
    use crate::event::{EventKind, Intensity};
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_replay_dir(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "eew-quake-notify-{name}-{}-{suffix}",
            std::process::id()
        ))
    }

    fn write_json(path: &Path, value: serde_json::Value) {
        fs::write(
            path,
            serde_json::to_string_pretty(&value).expect("fixture JSON should serialize"),
        )
        .expect("fixture file should write");
    }

    fn minimal_earthquake_body() -> &'static str {
        r#"{
          "_schema": { "type": "earthquake-information", "version": "1.0.0" },
          "type": "VXSE53",
          "title": "震源・震度情報",
          "status": "通常",
          "infoType": "発表",
          "reportDateTime": "2026-06-26T12:01:00+09:00",
          "eventId": "20260626120000",
          "serialNo": "1",
          "body": {
            "earthquake": {
              "originTime": "2026-06-26T12:00:00+09:00",
              "arrivalTime": "2026-06-26T12:00:00+09:00",
              "hypocenter": {
                "name": "東京都23区",
                "depth": { "unit": "km", "value": "40" }
              },
              "magnitude": { "unit": "Mj", "value": "5.2" }
            },
            "intensity": {
              "maxInt": "4",
              "regions": [{ "name": "東京都", "maxInt": "4" }]
            }
          }
        }"#
    }

    fn minimal_eew_forecast_body() -> &'static str {
        r#"{
          "_schema": { "type": "eew-information", "version": "1.0.0" },
          "type": "VXSE43",
          "title": "緊急地震速報",
          "status": "通常",
          "infoType": "発表",
          "reportDateTime": "2026-06-26T22:29:10+09:00",
          "eventId": "20260626222902",
          "serialNo": "2",
          "body": {
            "isWarning": false,
            "isLastInfo": true,
            "isCanceled": false,
            "earthquake": {
              "originTime": "2026-06-26T22:29:02+09:00",
              "arrivalTime": "2026-06-26T22:29:02+09:00",
              "hypocenter": {
                "name": "鹿児島湾",
                "depth": { "unit": "km", "value": "10" }
              },
              "magnitude": { "unit": "Mj", "value": "5.4" }
            },
            "intensity": {
              "forecastMaxInt": { "from": "5-", "to": "5-" }
            }
          }
        }"#
    }

    fn websocket_data_json(classification: &str, data_type: &str, body: &str) -> serde_json::Value {
        json!({
            "type": "data",
            "version": "2.0",
            "classification": classification,
            "id": "fixture-1",
            "head": {
                "type": data_type,
                "author": "気象庁",
                "time": "2026-06-26T13:29:00+09:00",
                "test": false,
                "xml": false
            },
            "format": "json",
            "compression": null,
            "encoding": "utf-8",
            "body": body
        })
    }

    #[test]
    fn loads_control_panel_directory_as_websocket_data() {
        let path = temp_replay_dir("control-panel");
        fs::create_dir_all(&path).expect("fixture directory should be created");
        write_json(
            &path.join("telegrams.json"),
            json!([
                {
                    "id": "binary-1",
                    "classification": "eew.warning",
                    "head": {
                        "type": "VXSE43",
                        "author": "気象庁",
                        "time": "2026-06-26T13:29:00+09:00",
                        "test": false,
                        "xml": true
                    },
                    "format": "binary",
                    "filename": "telegram.xml"
                },
                {
                    "id": "json-1",
                    "classification": "eew.warning",
                    "head": {
                        "type": "VXSE43",
                        "author": "気象庁",
                        "time": "2026-06-26T13:29:00+09:00",
                        "test": false,
                        "xml": false
                    },
                    "format": "json",
                    "filename": "telegram.json"
                }
            ]),
        );
        fs::write(path.join("telegram.json"), minimal_eew_forecast_body())
            .expect("fixture telegram should write");

        let data = load_replay_data(&path).expect("fixture should load");

        assert_eq!(data.len(), 2);
        assert_eq!(data[0].classification, "eew.warning");
        assert_eq!(data[0].head.data_type, "VXSE43");
        assert_eq!(data[1].format.as_deref(), Some("json"));
        assert!(data[1].body.contains("\"eventId\": \"20260626222902\""));

        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn loads_websocket_data_message_file() {
        let message = r#"{
          "type": "data",
          "version": "2.0",
          "classification": "eew.warning",
          "id": "fixture-1",
          "head": {
            "type": "VXSE43",
            "author": "RJTD",
            "time": "2026-06-26T13:29:00.000Z",
            "test": false
          },
          "format": "json",
          "compression": null,
          "encoding": "utf-8",
          "body": "{}"
        }"#;

        let path = std::env::temp_dir().join("eew-quake-notify-websocket-data-fixture.json");
        fs::write(&path, message).expect("fixture file should write");

        let data = load_replay_data(&path).expect("fixture should load");

        assert_eq!(data.len(), 1);
        assert_eq!(data[0].id, "fixture-1");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn loads_telegram_earthquake_directory_and_converts_json_items() {
        let path = temp_replay_dir("telegram-earthquake");
        fs::create_dir_all(&path).expect("fixture directory should be created");
        write_json(
            &path.join("telegrams.json"),
            json!([
                {
                    "id": "binary-1",
                    "classification": "telegram.earthquake",
                    "head": {
                        "type": "VXSE53",
                        "author": "気象庁",
                        "time": "2026-06-26T12:01:00+09:00",
                        "test": false,
                        "xml": true
                    },
                    "format": "binary",
                    "filename": "telegram.xml"
                },
                {
                    "id": "json-1",
                    "classification": "telegram.earthquake",
                    "head": {
                        "type": "VXSE53",
                        "author": "気象庁",
                        "time": "2026-06-26T12:01:00+09:00",
                        "test": false,
                        "xml": false
                    },
                    "format": "json",
                    "filename": "telegram.json"
                }
            ]),
        );
        fs::write(path.join("telegram.json"), minimal_earthquake_body())
            .expect("fixture telegram should write");

        let data = load_replay_data(&path).expect("fixture should load");

        assert!(!data.is_empty());
        assert!(
            data.iter()
                .any(|data| data.format.as_deref() == Some("binary") && data.body.is_empty())
        );

        let converted_count = data
            .iter()
            .map(websocket_data_to_event)
            .collect::<Result<Vec<_>, _>>()
            .expect("fixture data should normalize without errors")
            .into_iter()
            .flatten()
            .count();

        assert!(converted_count > 0);

        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn loads_eew_forecast_fixture_and_converts_event() {
        let path = temp_replay_dir("eew-forecast-file");
        fs::create_dir_all(&path).expect("fixture directory should be created");
        let fixture_path = path.join("eew-forecast_minimal.json");
        write_json(
            &fixture_path,
            websocket_data_json("eew.forecast", "VXSE43", minimal_eew_forecast_body()),
        );

        let data = load_replay_data(&fixture_path).expect("fixture should load");
        let event = websocket_data_to_event(&data[0])
            .expect("fixture should normalize")
            .expect("fixture should produce an event");

        assert_eq!(event.kind, EventKind::EewForecast);
        assert_eq!(event.max_intensity, Some(Intensity::FiveLower));

        let _ = fs::remove_dir_all(path);
    }
}
