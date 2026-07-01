use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SocketStartRequestDto {
    pub classifications: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub types: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formats: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SocketStartResponseDto {
    pub response_id: String,
    pub response_time: String,
    pub status: String,
    #[serde(default)]
    pub ticket: Option<String>,
    #[serde(default)]
    pub websocket: Option<WebSocketInfoDto>,
    #[serde(default)]
    pub error: Option<ApiErrorDto>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct OAuthTokenResponseDto {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    #[serde(default)]
    pub scope: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct WebSocketInfoDto {
    pub id: NumberOrString,
    pub url: String,
    pub protocol: Vec<String>,
    pub expiration: u64,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ApiErrorDto {
    pub message: String,
    pub code: u16,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WebSocketMessageDto {
    #[serde(rename = "start")]
    Start(WebSocketStartDto),
    #[serde(rename = "ping")]
    Ping {
        #[serde(rename = "pingId")]
        ping_id: Option<String>,
    },
    #[serde(rename = "pong")]
    Pong {
        #[serde(rename = "pingId")]
        ping_id: Option<String>,
    },
    #[serde(rename = "data")]
    Data(Box<WebSocketDataDto>),
    #[serde(rename = "error")]
    Error(WebSocketErrorDto),
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WebSocketSendMessageDto {
    #[serde(rename = "ping")]
    Ping {
        #[serde(rename = "pingId")]
        #[serde(skip_serializing_if = "Option::is_none")]
        ping_id: Option<String>,
    },
    #[serde(rename = "pong")]
    Pong {
        #[serde(rename = "pingId")]
        #[serde(skip_serializing_if = "Option::is_none")]
        ping_id: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketStartDto {
    pub socket_id: NumberOrString,
    pub classifications: Vec<String>,
    pub types: Option<Vec<String>>,
    pub test: String,
    pub formats: Vec<String>,
    pub app_name: Option<String>,
    pub time: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketDataDto {
    pub version: String,
    pub classification: String,
    pub id: String,
    pub head: TelegramHeadDto,
    #[serde(default)]
    pub xml_report: Option<XmlReportDto>,
    pub format: Option<String>,
    pub compression: Option<String>,
    pub encoding: Option<String>,
    pub body: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JsonTelegramDto {
    #[serde(default)]
    pub schema: Option<JsonSchemaDto>,
    #[serde(default, rename = "_schema")]
    pub underscore_schema: Option<JsonSchemaDto>,
    #[serde(rename = "type")]
    pub data_type: String,
    pub title: String,
    pub status: String,
    #[serde(default)]
    pub info_type: Option<String>,
    #[serde(default)]
    pub report_date_time: Option<String>,
    #[serde(default)]
    pub target_date_time: Option<String>,
    #[serde(default)]
    pub event_id: Option<String>,
    #[serde(default)]
    pub serial_no: Option<String>,
    #[serde(default)]
    pub body: Option<JsonTelegramBodyDto>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct JsonSchemaDto {
    #[serde(rename = "type")]
    pub data_type: String,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JsonTelegramBodyDto {
    #[serde(default)]
    pub earthquake: Option<EewEarthquakeDto>,
    #[serde(default)]
    pub intensity: Option<JsonIntensityDto>,
    #[serde(default)]
    pub is_last_info: Option<bool>,
    #[serde(default)]
    pub is_canceled: Option<bool>,
    #[serde(default)]
    pub is_warning: Option<bool>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub prefectures: Vec<JsonAreaDto>,
    #[serde(default)]
    pub regions: Vec<JsonAreaDto>,
    #[serde(default)]
    pub zones: Vec<JsonAreaDto>,
    #[serde(default)]
    pub cities: Vec<JsonAreaDto>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JsonIntensityDto {
    #[serde(default)]
    pub max_int: Option<String>,
    #[serde(default)]
    pub forecast_max_int: Option<IntensityForecastDto>,
    #[serde(default)]
    pub prefectures: Vec<JsonAreaDto>,
    #[serde(default)]
    pub regions: Vec<JsonAreaDto>,
    #[serde(default)]
    pub zones: Vec<JsonAreaDto>,
    #[serde(default)]
    pub cities: Vec<JsonAreaDto>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct JsonAreaDto {
    pub name: String,
    #[serde(default)]
    pub max_int: Option<String>,
    #[serde(default)]
    pub kind: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct WebSocketErrorDto {
    pub error: String,
    pub code: u16,
    pub close: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TelegramListResponseDto {
    pub response_id: String,
    pub response_time: String,
    pub status: String,
    #[serde(default)]
    pub items: Vec<TelegramItemDto>,
    #[serde(default)]
    pub next_token: Option<String>,
    #[serde(default)]
    pub next_pooling: Option<String>,
    #[serde(default)]
    pub next_pooling_interval: Option<u64>,
    #[serde(default)]
    pub error: Option<ApiErrorDto>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TelegramItemDto {
    pub serial: NumberOrString,
    pub classification: String,
    pub id: String,
    pub head: TelegramHeadDto,
    pub received_time: String,
    #[serde(default)]
    pub xml_report: Option<XmlReportDto>,
    pub format: Option<String>,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct TelegramHeadDto {
    #[serde(rename = "type")]
    pub data_type: String,
    pub author: String,
    pub time: String,
    #[serde(default)]
    pub designation: Option<String>,
    pub test: bool,
    #[serde(default)]
    pub xml: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct XmlReportDto {
    #[serde(default)]
    pub control: Option<XmlControlDto>,
    #[serde(default)]
    pub head: Option<XmlHeadDto>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct XmlControlDto {
    pub title: Option<String>,
    pub date_time: Option<String>,
    pub status: Option<String>,
    pub editorial_office: Option<String>,
    pub publishing_office: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct XmlHeadDto {
    pub title: Option<String>,
    pub report_date_time: Option<String>,
    pub target_date_time: Option<String>,
    pub event_id: Option<String>,
    pub serial: Option<String>,
    pub info_type: Option<String>,
    pub info_kind: Option<String>,
    pub info_kind_version: Option<String>,
    pub headline: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GdEarthquakeListResponseDto {
    pub response_id: String,
    pub response_time: String,
    pub status: String,
    #[serde(default)]
    pub items: Vec<GdEarthquakeItemDto>,
    #[serde(default)]
    pub next_token: Option<String>,
    #[serde(default)]
    pub next_pooling: Option<String>,
    #[serde(default)]
    pub next_pooling_interval: Option<u64>,
    #[serde(default)]
    pub error: Option<ApiErrorDto>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GdEarthquakeItemDto {
    pub id: NumberOrString,
    #[serde(default, rename = "type")]
    pub item_type: Option<String>,
    pub event_id: String,
    #[serde(default)]
    pub origin_time: Option<String>,
    pub arrival_time: String,
    #[serde(default)]
    pub hypocenter: Option<HypocenterDto>,
    #[serde(default)]
    pub magnitude: Option<MagnitudeDto>,
    #[serde(default)]
    pub max_int: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GdEewListResponseDto {
    pub response_id: String,
    pub response_time: String,
    pub status: String,
    #[serde(default)]
    pub items: Vec<GdEewItemDto>,
    #[serde(default)]
    pub next_token: Option<String>,
    #[serde(default)]
    pub error: Option<ApiErrorDto>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GdEewEventResponseDto {
    pub response_id: String,
    pub response_time: String,
    pub status: String,
    #[serde(default)]
    pub items: Vec<GdEewItemDto>,
    #[serde(default)]
    pub error: Option<ApiErrorDto>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GdEewItemDto {
    pub id: NumberOrString,
    pub event_id: String,
    pub serial: u32,
    pub date_time: String,
    pub is_last_info: bool,
    pub is_canceled: bool,
    #[serde(default)]
    pub is_warning: Option<bool>,
    #[serde(default)]
    pub earthquake: Option<EewEarthquakeDto>,
    #[serde(default)]
    pub intensity: Option<GdEewIntensityDto>,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EewEarthquakeDto {
    #[serde(default)]
    pub origin_time: Option<String>,
    pub arrival_time: String,
    #[serde(default)]
    pub hypocenter: Option<HypocenterDto>,
    #[serde(default)]
    pub magnitude: Option<MagnitudeDto>,
}

impl EewEarthquakeDto {
    pub fn origin_or_arrival_time(&self) -> Option<String> {
        self.origin_time
            .clone()
            .or_else(|| Some(self.arrival_time.clone()))
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GdEewIntensityDto {
    #[serde(default)]
    pub forecast_max_int: Option<IntensityForecastDto>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct IntensityForecastDto {
    pub from: String,
    #[serde(default)]
    pub to: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct HypocenterDto {
    pub name: String,
    #[serde(default)]
    pub depth: Option<DepthDto>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct DepthDto {
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub condition: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct MagnitudeDto {
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub condition: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum NumberOrString {
    Number(u64),
    String(String),
}

impl fmt::Display for NumberOrString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Number(value) => write!(formatter, "{value}"),
            Self::String(value) => formatter.write_str(value),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_websocket_ping_message() {
        let message: WebSocketMessageDto =
            serde_json::from_str(r#"{"type":"ping","pingId":"012345"}"#)
                .expect("ping should parse");

        assert_eq!(
            message,
            WebSocketMessageDto::Ping {
                ping_id: Some("012345".to_owned())
            }
        );
    }

    #[test]
    fn serializes_websocket_pong_message() {
        let message = serde_json::to_string(&WebSocketSendMessageDto::Pong {
            ping_id: Some("012345".to_owned()),
        })
        .expect("pong should serialize");

        assert_eq!(message, r#"{"type":"pong","pingId":"012345"}"#);
    }

    #[test]
    fn parses_number_or_string_ids() {
        let number: NumberOrString = serde_json::from_str("1584").expect("number id should parse");
        let string: NumberOrString =
            serde_json::from_str(r#""1584""#).expect("string id should parse");

        assert_eq!(number.to_string(), "1584");
        assert_eq!(string.to_string(), "1584");
    }

    #[test]
    fn parses_oauth_token_response() {
        let response: OAuthTokenResponseDto = serde_json::from_str(
            r#"{
              "access_token": "ATn.placeholder",
              "token_type": "Bearer",
              "expires_in": 21600,
              "scope": "socket.start telegram.get.earthquake"
            }"#,
        )
        .expect("token response should parse");

        assert_eq!(response.access_token, "ATn.placeholder");
        assert_eq!(response.token_type, "Bearer");
        assert_eq!(response.expires_in, 21600);
    }

    #[test]
    fn parses_gd_earthquake_list_response() {
        let response: GdEarthquakeListResponseDto = serde_json::from_str(
            r#"{
              "responseId": "3750ccf70651e928",
              "responseTime": "2021-04-01T00:00:00.000Z",
              "status": "ok",
              "items": [{
                "id": 1584,
                "type": "normal",
                "eventId": "20210808085414",
                "originTime": "2021-08-08T08:54:00+09:00",
                "arrivalTime": "2021-08-08T08:54:00+09:00",
                "hypocenter": {
                  "code": "787",
                  "name": "鹿児島湾",
                  "depth": { "type": "深さ", "unit": "km", "value": "0", "condition": "ごく浅い" }
                },
                "magnitude": { "type": "マグニチュード", "unit": "Mj", "value": "2.6" },
                "maxInt": "2"
              }],
              "nextPooling": "token",
              "nextPoolingInterval": 2000
            }"#,
        )
        .expect("GD earthquake response should parse");

        assert_eq!(response.items[0].event_id, "20210808085414");
        assert_eq!(response.items[0].item_type.as_deref(), Some("normal"));
        assert_eq!(response.items[0].max_int.as_deref(), Some("2"));
    }

    #[test]
    fn parses_gd_eew_cancel_response() {
        let response: GdEewEventResponseDto = serde_json::from_str(
            r#"{
              "responseId": "3750ccf70651e928",
              "responseTime": "2021-04-01T00:00:00.000Z",
              "status": "ok",
              "items": [{
                "id": 3,
                "eventId": "20160801170904",
                "serial": 2,
                "dateTime": "2016-08-01T17:09:19+09:00",
                "isLastInfo": true,
                "isCanceled": true,
                "text": "先ほどの、緊急地震速報（予報）を取り消します。",
                "telegrams": []
              }]
            }"#,
        )
        .expect("GD EEW cancel response should parse");

        assert!(response.items[0].is_canceled);
        assert!(response.items[0].earthquake.is_none());
    }

    #[test]
    fn parses_json_telegram_body() {
        let telegram: JsonTelegramDto = serde_json::from_str(
            r#"{
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
            }"#,
        )
        .expect("JSON telegram should parse");

        assert_eq!(telegram.data_type, "VXSE51");
        assert_eq!(
            telegram.body.unwrap().intensity.unwrap().max_int.as_deref(),
            Some("4")
        );
    }
}
