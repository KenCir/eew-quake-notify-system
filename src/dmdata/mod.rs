pub mod client;
pub mod dto;
pub mod replay;
pub mod runtime;

use std::{
    io::{Cursor, Read},
    num::ParseIntError,
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use flate2::read::GzDecoder;
use thiserror::Error;
use zip::ZipArchive;

use crate::event::{EarthquakeEvent, EventKind, Hypocenter, Intensity, Magnitude};

use self::dto::{
    DepthDto, EewEarthquakeDto, GdEarthquakeItemDto, GdEewItemDto, IntensityForecastDto,
    JsonAreaDto, JsonIntensityDto, JsonTelegramBodyDto, JsonTelegramDto, MagnitudeDto,
    WebSocketDataDto,
};

#[derive(Debug, Error, Clone, PartialEq)]
pub enum ConvertError {
    #[error("unsupported telegram format")]
    UnsupportedFormat,
    #[error("unsupported telegram encoding: {0}")]
    UnsupportedEncoding(String),
    #[error("unsupported telegram compression: {0}")]
    UnsupportedCompression(String),
    #[error("failed to decode base64 telegram body: {0}")]
    DecodeBase64(String),
    #[error("failed to decompress gzip telegram body: {0}")]
    DecodeGzip(String),
    #[error("failed to decompress zip telegram body: {0}")]
    DecodeZip(String),
    #[error("failed to decode telegram body as UTF-8: {0}")]
    DecodeUtf8(String),
    #[error("failed to parse JSON telegram body: {0}")]
    ParseJson(String),
    #[error("invalid intensity {value}: {source}")]
    InvalidIntensity {
        value: String,
        source: crate::event::ParseIntensityError,
    },
    #[error("invalid depth {value}: {source}")]
    InvalidDepth {
        value: String,
        #[source]
        source: ParseIntError,
    },
    #[error("invalid magnitude value: {value}")]
    InvalidMagnitude { value: String },
}

pub fn websocket_data_to_event(
    data: &WebSocketDataDto,
) -> Result<Option<EarthquakeEvent>, ConvertError> {
    if !is_json_format(data.format.as_deref()) {
        return Ok(None);
    }

    let body = decode_telegram_body(data)?;
    let telegram: JsonTelegramDto =
        serde_json::from_str(&body).map_err(|error| ConvertError::ParseJson(error.to_string()))?;

    json_telegram_to_event(
        &data.id,
        &data.classification,
        &data.head.data_type,
        telegram,
    )
}

fn decode_telegram_body(data: &WebSocketDataDto) -> Result<String, ConvertError> {
    let compression = data
        .compression
        .as_deref()
        .filter(|value| !value.is_empty());
    let encoding = data.encoding.as_deref().filter(|value| !value.is_empty());

    match compression {
        Some(compression) if compression.eq_ignore_ascii_case("gzip") => {
            decode_compressed_body(&data.body, encoding, |compressed| {
                decode_gzip_body(&compressed)
            })
        }
        Some(compression) if compression.eq_ignore_ascii_case("zip") => {
            decode_compressed_body(&data.body, encoding, |compressed| {
                decode_zip_body(&compressed)
            })
        }
        Some(compression) => Err(ConvertError::UnsupportedCompression(compression.to_owned())),
        None => match encoding {
            Some(encoding)
                if encoding.eq_ignore_ascii_case("utf-8")
                    || encoding.eq_ignore_ascii_case("utf8") =>
            {
                Ok(data.body.clone())
            }
            Some(encoding) if encoding.eq_ignore_ascii_case("base64") => {
                let decoded = BASE64_STANDARD
                    .decode(data.body.trim())
                    .map_err(|error| ConvertError::DecodeBase64(error.to_string()))?;
                String::from_utf8(decoded)
                    .map_err(|error| ConvertError::DecodeUtf8(error.to_string()))
            }
            Some(encoding) => Err(ConvertError::UnsupportedEncoding(encoding.to_owned())),
            None => Ok(data.body.clone()),
        },
    }
}

fn decode_compressed_body(
    body: &str,
    encoding: Option<&str>,
    decode: impl FnOnce(Vec<u8>) -> Result<Vec<u8>, ConvertError>,
) -> Result<String, ConvertError> {
    if let Some(encoding) = encoding
        && !encoding.eq_ignore_ascii_case("base64")
    {
        return Err(ConvertError::UnsupportedEncoding(encoding.to_owned()));
    }

    let compressed = BASE64_STANDARD
        .decode(body.trim())
        .map_err(|error| ConvertError::DecodeBase64(error.to_string()))?;
    let decompressed = decode(compressed)?;
    String::from_utf8(decompressed).map_err(|error| ConvertError::DecodeUtf8(error.to_string()))
}

fn decode_gzip_body(compressed: &[u8]) -> Result<Vec<u8>, ConvertError> {
    let mut decoder = GzDecoder::new(compressed);
    let mut decompressed = Vec::new();
    decoder
        .read_to_end(&mut decompressed)
        .map_err(|error| ConvertError::DecodeGzip(error.to_string()))?;
    Ok(decompressed)
}

fn decode_zip_body(compressed: &[u8]) -> Result<Vec<u8>, ConvertError> {
    let reader = Cursor::new(compressed);
    let mut archive =
        ZipArchive::new(reader).map_err(|error| ConvertError::DecodeZip(error.to_string()))?;

    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| ConvertError::DecodeZip(error.to_string()))?;
        if file.is_dir() {
            continue;
        }

        let mut decompressed = Vec::new();
        file.read_to_end(&mut decompressed)
            .map_err(|error| ConvertError::DecodeZip(error.to_string()))?;
        return Ok(decompressed);
    }

    Err(ConvertError::DecodeZip(
        "zip archive does not contain a file".to_owned(),
    ))
}

pub fn json_telegram_to_event(
    data_id: &str,
    classification: &str,
    head_type: &str,
    telegram: JsonTelegramDto,
) -> Result<Option<EarthquakeEvent>, ConvertError> {
    let schema_type = telegram
        .underscore_schema
        .as_ref()
        .or(telegram.schema.as_ref())
        .map(|schema| schema.data_type.as_str());
    let body = telegram.body.as_ref();

    let kind = event_kind(
        classification,
        head_type,
        schema_type,
        body.and_then(|body| body.is_warning),
    );
    if matches!(kind, EventKind::Unknown) {
        return Ok(None);
    }

    let source_id = telegram
        .event_id
        .as_ref()
        .map(|event_id| {
            format!(
                "telegram:{}:{}:{}",
                event_id,
                telegram.serial_no.as_deref().unwrap_or(""),
                data_id
            )
        })
        .unwrap_or_else(|| format!("telegram:{data_id}"));

    Ok(Some(EarthquakeEvent {
        source_id: Some(source_id),
        kind,
        serial: telegram
            .serial_no
            .as_deref()
            .and_then(|serial| serial.parse::<u32>().ok()),
        is_final: is_final(&telegram, body),
        is_cancelled: is_cancelled(&telegram, body),
        occurred_at: body
            .and_then(|body| body.earthquake.as_ref())
            .and_then(EewEarthquakeDto::origin_or_arrival_time),
        announced_at: telegram.report_date_time.or(telegram.target_date_time),
        hypocenter: body
            .and_then(|body| body.earthquake.as_ref())
            .and_then(|earthquake| earthquake.hypocenter.as_ref())
            .map(hypocenter_from_dto)
            .transpose()?,
        max_intensity: body
            .and_then(max_intensity_from_json_body)
            .map(parse_intensity)
            .transpose()?,
        magnitude: body
            .and_then(|body| body.earthquake.as_ref())
            .and_then(|earthquake| earthquake.magnitude.as_ref())
            .map(magnitude_from_dto)
            .transpose()?
            .flatten(),
        affected_areas: body.map(affected_area_names).unwrap_or_default(),
    }))
}

pub fn earthquake_item_to_event(
    item: GdEarthquakeItemDto,
) -> Result<EarthquakeEvent, ConvertError> {
    Ok(EarthquakeEvent {
        source_id: Some(format!("gd-earthquake:{}:{}", item.event_id, item.id)),
        kind: EventKind::Earthquake,
        serial: None,
        is_final: true,
        is_cancelled: false,
        occurred_at: item.origin_time,
        announced_at: Some(item.arrival_time),
        hypocenter: item
            .hypocenter
            .as_ref()
            .map(hypocenter_from_dto)
            .transpose()?,
        max_intensity: parse_optional_intensity(item.max_int.as_deref())?,
        magnitude: item
            .magnitude
            .as_ref()
            .map(magnitude_from_dto)
            .transpose()?
            .flatten(),
        affected_areas: Vec::new(),
    })
}

pub fn eew_item_to_event(item: GdEewItemDto) -> Result<EarthquakeEvent, ConvertError> {
    let earthquake = item.earthquake.as_ref();

    Ok(EarthquakeEvent {
        source_id: Some(format!(
            "gd-eew:{}:{}:{}",
            item.event_id, item.serial, item.id
        )),
        kind: if item.is_warning.unwrap_or(false) {
            EventKind::EewWarning
        } else {
            EventKind::EewForecast
        },
        serial: Some(item.serial),
        is_final: item.is_last_info,
        is_cancelled: item.is_canceled,
        occurred_at: earthquake.and_then(EewEarthquakeDto::origin_or_arrival_time),
        announced_at: Some(item.date_time),
        hypocenter: earthquake
            .and_then(|earthquake| earthquake.hypocenter.as_ref())
            .map(hypocenter_from_dto)
            .transpose()?,
        max_intensity: item
            .intensity
            .as_ref()
            .and_then(|intensity| intensity.forecast_max_int.as_ref())
            .map(intensity_from_forecast)
            .transpose()?,
        magnitude: earthquake
            .and_then(|earthquake| earthquake.magnitude.as_ref())
            .map(magnitude_from_dto)
            .transpose()?
            .flatten(),
        affected_areas: Vec::new(),
    })
}

fn hypocenter_from_dto(value: &dto::HypocenterDto) -> Result<Hypocenter, ConvertError> {
    Ok(Hypocenter {
        name: value.name.clone(),
        depth_km: value
            .depth
            .as_ref()
            .map(depth_km_from_dto)
            .transpose()?
            .flatten(),
    })
}

fn depth_km_from_dto(value: &DepthDto) -> Result<Option<u32>, ConvertError> {
    let Some(depth) = value.value.as_deref() else {
        return Ok(None);
    };

    let depth = depth.parse().map_err(|source| ConvertError::InvalidDepth {
        value: depth.to_owned(),
        source,
    })?;
    Ok(Some(depth))
}

fn magnitude_from_dto(value: &MagnitudeDto) -> Result<Option<Magnitude>, ConvertError> {
    let Some(magnitude) = value.value.as_deref() else {
        return Ok(None);
    };

    let parsed = magnitude
        .parse::<f32>()
        .ok()
        .and_then(Magnitude::new)
        .ok_or_else(|| ConvertError::InvalidMagnitude {
            value: magnitude.to_owned(),
        })?;

    Ok(Some(parsed))
}

fn intensity_from_forecast(value: &IntensityForecastDto) -> Result<Intensity, ConvertError> {
    parse_intensity(&value.from)
}

fn parse_optional_intensity(value: Option<&str>) -> Result<Option<Intensity>, ConvertError> {
    value.map(parse_intensity).transpose()
}

fn parse_intensity(value: &str) -> Result<Intensity, ConvertError> {
    value
        .parse()
        .map_err(|source| ConvertError::InvalidIntensity {
            value: value.to_owned(),
            source,
        })
}

fn is_json_format(format: Option<&str>) -> bool {
    format
        .map(|format| format.eq_ignore_ascii_case("json"))
        .unwrap_or(true)
}

fn event_kind(
    classification: &str,
    head_type: &str,
    schema_type: Option<&str>,
    is_warning: Option<bool>,
) -> EventKind {
    if classification == "eew.warning" || is_warning == Some(true) {
        return EventKind::EewWarning;
    }

    if classification == "eew.forecast"
        || classification.starts_with("eew")
        || matches!(schema_type, Some("eew-information"))
    {
        return EventKind::EewForecast;
    }

    if matches!(schema_type, Some("earthquake-information")) || head_type.starts_with("VXSE") {
        return match head_type {
            "VXSE51" => EventKind::IntensityReport,
            _ => EventKind::Earthquake,
        };
    }

    EventKind::Unknown
}

fn is_final(telegram: &JsonTelegramDto, body: Option<&JsonTelegramBodyDto>) -> bool {
    if body.and_then(|body| body.is_last_info).unwrap_or(false) {
        return true;
    }

    telegram
        .info_type
        .as_deref()
        .map(|info_type| info_type.contains("最終") || info_type.contains("取消"))
        .unwrap_or(false)
}

fn is_cancelled(telegram: &JsonTelegramDto, body: Option<&JsonTelegramBodyDto>) -> bool {
    if body.and_then(|body| body.is_canceled).unwrap_or(false) {
        return true;
    }

    telegram
        .info_type
        .as_deref()
        .map(|info_type| info_type.contains("取消"))
        .unwrap_or(false)
}

fn max_intensity_from_json_body(body: &JsonTelegramBodyDto) -> Option<&str> {
    body.intensity.as_ref().and_then(max_intensity_from_json)
}

fn max_intensity_from_json(intensity: &JsonIntensityDto) -> Option<&str> {
    intensity
        .forecast_max_int
        .as_ref()
        .map(|forecast| forecast.from.as_str())
        .or(intensity.max_int.as_deref())
        .or_else(|| max_area_intensity(&intensity.prefectures))
        .or_else(|| max_area_intensity(&intensity.regions))
        .or_else(|| max_area_intensity(&intensity.zones))
        .or_else(|| max_area_intensity(&intensity.cities))
}

fn max_area_intensity(areas: &[JsonAreaDto]) -> Option<&str> {
    areas
        .iter()
        .filter_map(|area| area.max_int.as_deref())
        .max()
}

fn affected_area_names(body: &JsonTelegramBodyDto) -> Vec<String> {
    let mut names = Vec::new();
    collect_area_names(&mut names, &body.prefectures);
    collect_area_names(&mut names, &body.regions);
    collect_area_names(&mut names, &body.zones);
    collect_area_names(&mut names, &body.cities);

    if let Some(intensity) = body.intensity.as_ref() {
        collect_area_names(&mut names, &intensity.prefectures);
        collect_area_names(&mut names, &intensity.regions);
        collect_area_names(&mut names, &intensity.zones);
        collect_area_names(&mut names, &intensity.cities);
    }

    names.sort();
    names.dedup();
    names
}

fn collect_area_names(names: &mut Vec<String>, areas: &[JsonAreaDto]) {
    names.extend(
        areas
            .iter()
            .map(|area| area.name.trim())
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dmdata::dto::{
        GdEarthquakeItemDto, GdEewIntensityDto, HypocenterDto, NumberOrString, TelegramHeadDto,
    };
    use flate2::{Compression, write::GzEncoder};
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn depth(value: &str) -> DepthDto {
        DepthDto {
            unit: Some("km".to_owned()),
            value: Some(value.to_owned()),
            condition: None,
        }
    }

    fn magnitude(value: Option<&str>) -> MagnitudeDto {
        MagnitudeDto {
            unit: Some("Mj".to_owned()),
            value: value.map(ToOwned::to_owned),
            condition: None,
        }
    }

    fn hypocenter() -> HypocenterDto {
        HypocenterDto {
            name: "鹿児島湾".to_owned(),
            depth: Some(depth("10")),
        }
    }

    fn gzip_base64(value: &str) -> String {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(value.as_bytes())
            .expect("gzip input should write");
        BASE64_STANDARD.encode(encoder.finish().expect("gzip should finish"))
    }

    fn zip_base64(value: &str) -> String {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            writer
                .start_file("telegram.json", SimpleFileOptions::default())
                .expect("zip file entry should start");
            writer
                .write_all(value.as_bytes())
                .expect("zip input should write");
            writer.finish().expect("zip should finish");
        }

        BASE64_STANDARD.encode(cursor.into_inner())
    }

    fn eew_websocket_data(
        classification: &str,
        is_warning: bool,
        is_canceled: bool,
    ) -> WebSocketDataDto {
        WebSocketDataDto {
            version: "2.0".to_owned(),
            classification: classification.to_owned(),
            id: "eew-fixture-1".to_owned(),
            head: TelegramHeadDto {
                data_type: "VXSE43".to_owned(),
                author: "気象庁".to_owned(),
                time: "2026-06-26T13:29:00+09:00".to_owned(),
                designation: None,
                test: false,
                xml: Some(false),
            },
            xml_report: None,
            format: Some("json".to_owned()),
            compression: None,
            encoding: Some("utf-8".to_owned()),
            body: format!(
                r#"{{
                  "_schema": {{ "type": "eew-information", "version": "1.0.0" }},
                  "type": "VXSE43",
                  "title": "緊急地震速報",
                  "status": "通常",
                  "infoType": "発表",
                  "reportDateTime": "2026-06-26T22:29:10+09:00",
                  "eventId": "20260626222902",
                  "serialNo": "2",
                  "body": {{
                    "isWarning": {is_warning},
                    "isLastInfo": true,
                    "isCanceled": {is_canceled},
                    "earthquake": {{
                      "originTime": "2026-06-26T22:29:02+09:00",
                      "arrivalTime": "2026-06-26T22:29:02+09:00",
                      "hypocenter": {{
                        "name": "鹿児島湾",
                        "depth": {{ "unit": "km", "value": "10" }}
                      }},
                      "magnitude": {{ "unit": "Mj", "value": "5.4" }}
                    }},
                    "intensity": {{
                      "forecastMaxInt": {{ "from": "5-", "to": "5-" }}
                    }}
                  }}
                }}"#
            ),
        }
    }

    #[test]
    fn converts_gd_earthquake_item() {
        let event = earthquake_item_to_event(GdEarthquakeItemDto {
            id: NumberOrString::Number(1584),
            item_type: Some("normal".to_owned()),
            event_id: "20210808085414".to_owned(),
            origin_time: Some("2021-08-08T08:54:00+09:00".to_owned()),
            arrival_time: "2021-08-08T08:54:00+09:00".to_owned(),
            hypocenter: Some(hypocenter()),
            magnitude: Some(magnitude(Some("2.6"))),
            max_int: Some("2".to_owned()),
        })
        .expect("earthquake event should convert");

        assert_eq!(
            event.source_id.as_deref(),
            Some("gd-earthquake:20210808085414:1584")
        );
        assert_eq!(event.kind, EventKind::Earthquake);
        assert_eq!(event.max_intensity, Some(Intensity::Two));
        assert_eq!(event.magnitude, Some(Magnitude(2.6)));
        assert_eq!(event.hypocenter.unwrap().depth_km, Some(10));
    }

    #[test]
    fn converts_gd_eew_item() {
        let event = eew_item_to_event(GdEewItemDto {
            id: NumberOrString::String("3".to_owned()),
            event_id: "20160801170904".to_owned(),
            serial: 2,
            date_time: "2016-08-01T17:09:19+09:00".to_owned(),
            is_last_info: true,
            is_canceled: false,
            is_warning: Some(false),
            earthquake: Some(EewEarthquakeDto {
                origin_time: Some("2016-08-01T17:09:04+09:00".to_owned()),
                arrival_time: "2016-08-01T17:09:04+09:00".to_owned(),
                hypocenter: Some(hypocenter()),
                magnitude: Some(magnitude(Some("9.2"))),
            }),
            intensity: Some(GdEewIntensityDto {
                forecast_max_int: Some(IntensityForecastDto {
                    from: "5-".to_owned(),
                    to: Some("over".to_owned()),
                }),
            }),
            text: None,
        })
        .expect("eew event should convert");

        assert_eq!(
            event.source_id.as_deref(),
            Some("gd-eew:20160801170904:2:3")
        );
        assert_eq!(event.kind, EventKind::EewForecast);
        assert_eq!(event.serial, Some(2));
        assert!(event.is_final);
        assert_eq!(event.max_intensity, Some(Intensity::FiveLower));
    }

    #[test]
    fn converts_gd_eew_warning_item() {
        let event = eew_item_to_event(GdEewItemDto {
            id: NumberOrString::String("3".to_owned()),
            event_id: "20160801170904".to_owned(),
            serial: 2,
            date_time: "2016-08-01T17:09:19+09:00".to_owned(),
            is_last_info: true,
            is_canceled: false,
            is_warning: Some(true),
            earthquake: Some(EewEarthquakeDto {
                origin_time: Some("2016-08-01T17:09:04+09:00".to_owned()),
                arrival_time: "2016-08-01T17:09:04+09:00".to_owned(),
                hypocenter: Some(hypocenter()),
                magnitude: Some(magnitude(Some("9.2"))),
            }),
            intensity: Some(GdEewIntensityDto {
                forecast_max_int: Some(IntensityForecastDto {
                    from: "5-".to_owned(),
                    to: Some("over".to_owned()),
                }),
            }),
            text: None,
        })
        .expect("eew warning event should convert");

        assert_eq!(event.kind, EventKind::EewWarning);
        assert_eq!(event.max_intensity, Some(Intensity::FiveLower));
    }

    #[test]
    fn converts_eew_cancel_without_earthquake_details() {
        let event = eew_item_to_event(GdEewItemDto {
            id: NumberOrString::Number(3),
            event_id: "20160801170904".to_owned(),
            serial: 2,
            date_time: "2016-08-01T17:09:19+09:00".to_owned(),
            is_last_info: true,
            is_canceled: true,
            is_warning: None,
            earthquake: None,
            intensity: None,
            text: Some("先ほどの、緊急地震速報（予報）を取り消します。".to_owned()),
        })
        .expect("cancel event should convert");

        assert!(event.is_cancelled);
        assert_eq!(event.kind, EventKind::EewForecast);
        assert_eq!(event.max_intensity, None);
        assert_eq!(event.hypocenter, None);
    }

    #[test]
    fn converts_unknown_magnitude_to_none() {
        let event = eew_item_to_event(GdEewItemDto {
            id: NumberOrString::Number(1),
            event_id: "20160801170904".to_owned(),
            serial: 1,
            date_time: "2016-08-01T17:09:06+09:00".to_owned(),
            is_last_info: false,
            is_canceled: false,
            is_warning: Some(false),
            earthquake: Some(EewEarthquakeDto {
                origin_time: None,
                arrival_time: "2016-08-01T17:09:04+09:00".to_owned(),
                hypocenter: Some(hypocenter()),
                magnitude: Some(MagnitudeDto {
                    unit: Some("Mj".to_owned()),
                    value: None,
                    condition: Some("Ｍ不明".to_owned()),
                }),
            }),
            intensity: None,
            text: None,
        })
        .expect("unknown magnitude should be recoverable");

        assert_eq!(event.magnitude, None);
    }

    #[test]
    fn converts_json_telegram_to_event() {
        let data = WebSocketDataDto {
            version: "1.0".to_owned(),
            classification: "telegram.earthquake".to_owned(),
            id: "raw-1".to_owned(),
            head: TelegramHeadDto {
                data_type: "VXSE51".to_owned(),
                author: "気象庁".to_owned(),
                time: "2026-06-26T12:01:00+09:00".to_owned(),
                designation: None,
                test: false,
                xml: Some(false),
            },
            xml_report: None,
            format: Some("json".to_owned()),
            compression: None,
            encoding: Some("utf-8".to_owned()),
            body: r#"{
              "_schema": { "type": "earthquake-information", "version": "1.0.0" },
              "type": "VXSE51",
              "title": "震度速報",
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
            .to_owned(),
        };

        let event = websocket_data_to_event(&data)
            .expect("websocket data should convert")
            .expect("earthquake telegram should produce an event");

        assert_eq!(event.kind, EventKind::IntensityReport);
        assert_eq!(event.serial, Some(1));
        assert_eq!(event.max_intensity, Some(Intensity::Four));
        assert_eq!(event.hypocenter.unwrap().name, "東京都23区");
        assert_eq!(event.affected_areas, ["東京都"]);
    }

    #[test]
    fn converts_vxse53_json_telegram_to_earthquake_event() {
        let data = WebSocketDataDto {
            version: "1.0".to_owned(),
            classification: "telegram.earthquake".to_owned(),
            id: "raw-vxse53".to_owned(),
            head: TelegramHeadDto {
                data_type: "VXSE53".to_owned(),
                author: "気象庁".to_owned(),
                time: "2026-06-26T12:01:00+09:00".to_owned(),
                designation: None,
                test: false,
                xml: Some(false),
            },
            xml_report: None,
            format: Some("json".to_owned()),
            compression: None,
            encoding: Some("utf-8".to_owned()),
            body: r#"{
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
                  "maxInt": "1",
                  "regions": [{ "name": "東京都", "maxInt": "1" }]
                }
              }
            }"#
            .to_owned(),
        };

        let event = websocket_data_to_event(&data)
            .expect("websocket data should convert")
            .expect("earthquake telegram should produce an event");

        assert_eq!(event.kind, EventKind::Earthquake);
        assert_eq!(event.max_intensity, Some(Intensity::One));
        assert_eq!(event.display_kind(), "地震情報");
    }

    #[test]
    fn converts_vxse52_json_telegram_to_earthquake_event() {
        let data = WebSocketDataDto {
            version: "1.0".to_owned(),
            classification: "telegram.earthquake".to_owned(),
            id: "raw-vxse52".to_owned(),
            head: TelegramHeadDto {
                data_type: "VXSE52".to_owned(),
                author: "気象庁".to_owned(),
                time: "2026-06-26T12:01:00+09:00".to_owned(),
                designation: None,
                test: false,
                xml: Some(false),
            },
            xml_report: None,
            format: Some("json".to_owned()),
            compression: None,
            encoding: Some("utf-8".to_owned()),
            body: r#"{
              "_schema": { "type": "earthquake-information", "version": "1.0.0" },
              "type": "VXSE52",
              "title": "震源に関する情報",
              "status": "通常",
              "infoType": "発表",
              "reportDateTime": "2026-06-26T12:01:00+09:00",
              "eventId": "20260626120000",
              "body": {
                "earthquake": {
                  "originTime": "2026-06-26T12:00:00+09:00",
                  "arrivalTime": "2026-06-26T12:00:00+09:00",
                  "hypocenter": {
                    "name": "東京都23区",
                    "depth": { "unit": "km", "value": "40" }
                  },
                  "magnitude": { "unit": "Mj", "value": "5.2" }
                }
              }
            }"#
            .to_owned(),
        };

        let event = websocket_data_to_event(&data)
            .expect("websocket data should convert")
            .expect("earthquake telegram should produce an event");

        assert_eq!(event.kind, EventKind::Earthquake);
        assert_eq!(event.display_kind(), "地震情報");
    }

    #[test]
    fn converts_eew_warning_json_telegram_to_warning_event() {
        let data = eew_websocket_data("eew.warning", true, false);

        let event = websocket_data_to_event(&data)
            .expect("eew warning data should convert")
            .expect("eew warning telegram should produce an event");

        assert_eq!(event.kind, EventKind::EewWarning);
        assert_eq!(event.max_intensity, Some(Intensity::FiveLower));
        assert_eq!(event.display_kind(), "緊急地震速報（警報）");
    }

    #[test]
    fn converts_eew_forecast_json_telegram_to_forecast_event() {
        let data = eew_websocket_data("eew.forecast", false, false);

        let event = websocket_data_to_event(&data)
            .expect("eew forecast data should convert")
            .expect("eew forecast telegram should produce an event");

        assert_eq!(event.kind, EventKind::EewForecast);
        assert_eq!(event.max_intensity, Some(Intensity::FiveLower));
        assert_eq!(event.display_kind(), "緊急地震速報（予報）");
    }

    #[test]
    fn converts_eew_cancel_preserving_classification_kind() {
        let data = eew_websocket_data("eew.warning", true, true);

        let event = websocket_data_to_event(&data)
            .expect("eew cancel data should convert")
            .expect("eew cancel telegram should produce an event");

        assert_eq!(event.kind, EventKind::EewWarning);
        assert!(event.is_cancelled);
    }

    #[test]
    fn skips_non_json_websocket_data() {
        let data = WebSocketDataDto {
            version: "1.0".to_owned(),
            classification: "telegram.earthquake".to_owned(),
            id: "raw-1".to_owned(),
            head: TelegramHeadDto {
                data_type: "VXSE51".to_owned(),
                author: "気象庁".to_owned(),
                time: "2026-06-26T12:01:00+09:00".to_owned(),
                designation: None,
                test: false,
                xml: Some(true),
            },
            xml_report: None,
            format: Some("xml".to_owned()),
            compression: None,
            encoding: Some("utf-8".to_owned()),
            body: "<Report />".to_owned(),
        };

        assert_eq!(websocket_data_to_event(&data), Ok(None));
    }

    #[test]
    fn converts_gzip_base64_json_telegram_to_event() {
        let body = r#"{
          "_schema": { "type": "earthquake-information", "version": "1.0.0" },
          "type": "VXSE53",
          "title": "震源・震度情報",
          "status": "通常",
          "infoType": "発表",
          "reportDateTime": "2026-06-27T11:54:10+09:00",
          "eventId": "20260627115300",
          "serialNo": "1",
          "body": {
            "earthquake": {
              "originTime": "2026-06-27T11:53:00+09:00",
              "arrivalTime": "2026-06-27T11:53:00+09:00",
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
        }"#;
        let data = WebSocketDataDto {
            version: "2.0".to_owned(),
            classification: "telegram.earthquake".to_owned(),
            id: "gzip-raw-1".to_owned(),
            head: TelegramHeadDto {
                data_type: "VXSE53".to_owned(),
                author: "気象庁".to_owned(),
                time: "2026-06-27T02:54:10.000Z".to_owned(),
                designation: None,
                test: false,
                xml: Some(false),
            },
            xml_report: None,
            format: Some("json".to_owned()),
            compression: Some("gzip".to_owned()),
            encoding: Some("base64".to_owned()),
            body: gzip_base64(body),
        };

        let event = websocket_data_to_event(&data)
            .expect("gzip websocket data should convert")
            .expect("earthquake telegram should produce an event");

        assert_eq!(event.kind, EventKind::Earthquake);
        assert_eq!(event.serial, Some(1));
        assert_eq!(event.max_intensity, Some(Intensity::Four));
        assert_eq!(event.hypocenter.unwrap().name, "東京都23区");
    }

    #[test]
    fn converts_zip_base64_json_telegram_to_event() {
        let body = r#"{
          "_schema": { "type": "earthquake-information", "version": "1.0.0" },
          "type": "VXSE53",
          "title": "震源・震度情報",
          "status": "通常",
          "infoType": "発表",
          "reportDateTime": "2026-06-27T11:54:10+09:00",
          "eventId": "20260627115300",
          "serialNo": "1",
          "body": {
            "earthquake": {
              "originTime": "2026-06-27T11:53:00+09:00",
              "arrivalTime": "2026-06-27T11:53:00+09:00",
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
        }"#;
        let data = WebSocketDataDto {
            version: "2.0".to_owned(),
            classification: "telegram.earthquake".to_owned(),
            id: "zip-raw-1".to_owned(),
            head: TelegramHeadDto {
                data_type: "VXSE53".to_owned(),
                author: "気象庁".to_owned(),
                time: "2026-06-27T02:54:10.000Z".to_owned(),
                designation: None,
                test: false,
                xml: Some(false),
            },
            xml_report: None,
            format: Some("json".to_owned()),
            compression: Some("zip".to_owned()),
            encoding: Some("base64".to_owned()),
            body: zip_base64(body),
        };

        let event = websocket_data_to_event(&data)
            .expect("zip websocket data should convert")
            .expect("earthquake telegram should produce an event");

        assert_eq!(event.kind, EventKind::Earthquake);
        assert_eq!(event.serial, Some(1));
        assert_eq!(event.max_intensity, Some(Intensity::Four));
        assert_eq!(event.hypocenter.unwrap().name, "東京都23区");
    }
}
