use reqwest::{StatusCode, header::CONTENT_TYPE};
use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;

use crate::config::DmdataConfig;

use super::dto::{
    ApiErrorDto, OAuthTokenResponseDto, SocketStartRequestDto, SocketStartResponseDto,
};

#[derive(Debug, Clone)]
pub struct DmdataClient {
    http_client: reqwest::Client,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketStart {
    pub id: Option<String>,
    pub url: String,
    pub protocols: Vec<String>,
}

#[derive(Debug, Error)]
pub enum DmdataClientError {
    #[error("failed to request socket start: {0}")]
    Request(#[from] reqwest::Error),
    #[error("environment variable {name} is not set")]
    MissingEnvironmentVariable { name: String },
    #[error("unsupported dmdata.auth_mode: {mode}")]
    UnsupportedAuthMode { mode: String },
    #[error("OAuth token request failed with HTTP status {status}: {message}")]
    OAuthHttpStatus { status: StatusCode, message: String },
    #[error("OAuth token response has unsupported token_type: {token_type}")]
    UnsupportedTokenType { token_type: String },
    #[error("OAuth token response did not include access_token")]
    EmptyAccessToken,
    #[error("socket start failed with HTTP status {status}: {message}")]
    HttpStatus { status: StatusCode, message: String },
    #[error("socket close failed with HTTP status {status}: {message}")]
    SocketCloseHttpStatus { status: StatusCode, message: String },
    #[error("socket start API error {code}: {message}")]
    Api { code: u16, message: String },
    #[error("socket start response did not include websocket info")]
    MissingWebSocket,
}

impl DmdataClient {
    pub fn new() -> Self {
        Self {
            http_client: reqwest::Client::new(),
        }
    }

    pub async fn start_socket(
        &self,
        config: &DmdataConfig,
    ) -> Result<SocketStart, DmdataClientError> {
        if let Some(websocket_url) = config
            .websocket_url
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            return Ok(SocketStart {
                id: None,
                url: websocket_url.to_owned(),
                protocols: vec!["dmdata.v2".to_owned()],
            });
        }

        let request = SocketStartRequestDto {
            classifications: config.classifications.clone(),
            types: (!config.types.is_empty()).then(|| config.types.clone()),
            test: Some(config.test.clone()),
            app_name: (!config.app_name.is_empty()).then(|| config.app_name.clone()),
            format_mode: Some(config.format_mode.clone()),
            formats: None,
        };

        let access_token = self.access_token(config).await?;
        let response = self
            .http_client
            .post(&config.socket_start_url)
            .bearer_auth(access_token.expose_secret())
            .json(&request)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await?;
            return Err(DmdataClientError::HttpStatus {
                status,
                message: http_error_message(&body),
            });
        }

        let response: SocketStartResponseDto = response.json().await?;
        if response.status != "ok" {
            return Err(api_error(response.error));
        }

        let websocket = response
            .websocket
            .ok_or(DmdataClientError::MissingWebSocket)?;
        Ok(SocketStart {
            id: Some(websocket.id.to_string()),
            url: websocket.url,
            protocols: websocket.protocol,
        })
    }

    pub async fn close_socket(
        &self,
        config: &DmdataConfig,
        socket_id: &str,
    ) -> Result<(), DmdataClientError> {
        let access_token = self.access_token(config).await?;
        let response = self
            .http_client
            .delete(socket_close_url(&config.socket_start_url, socket_id))
            .bearer_auth(access_token.expose_secret())
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await?;
            return Err(DmdataClientError::SocketCloseHttpStatus {
                status,
                message: http_error_message(&body),
            });
        }

        Ok(())
    }

    async fn access_token(&self, config: &DmdataConfig) -> Result<SecretString, DmdataClientError> {
        match config.auth_mode.as_str() {
            "access_token" => secret_from_env(&config.api_token_env),
            "client_credentials" => self.client_credentials_token(config).await,
            mode => Err(DmdataClientError::UnsupportedAuthMode {
                mode: mode.to_owned(),
            }),
        }
    }

    async fn client_credentials_token(
        &self,
        config: &DmdataConfig,
    ) -> Result<SecretString, DmdataClientError> {
        let client_id = config
            .client_id
            .as_deref()
            .and_then(non_empty_trimmed)
            .map(ToOwned::to_owned)
            .map(Ok)
            .unwrap_or_else(|| env_value(&config.client_id_env))?;
        let client_secret = config
            .client_secret
            .as_deref()
            .and_then(non_empty_trimmed)
            .map(|value| SecretString::new(value.to_owned().into()))
            .map(Ok)
            .unwrap_or_else(|| secret_from_env(&config.client_secret_env))?;
        let scope = oauth_scope(config);
        let form = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("grant_type", "client_credentials")
            .append_pair("client_id", &client_id)
            .append_pair("client_secret", client_secret.expose_secret())
            .append_pair("scope", &scope)
            .finish();

        let response = self
            .http_client
            .post(&config.token_endpoint_url)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(form)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await?;
            return Err(DmdataClientError::OAuthHttpStatus {
                status,
                message: http_error_message(&body),
            });
        }

        let token: OAuthTokenResponseDto = response.json().await?;
        if token.access_token.trim().is_empty() {
            return Err(DmdataClientError::EmptyAccessToken);
        }

        if !token.token_type.eq_ignore_ascii_case("bearer") {
            return Err(DmdataClientError::UnsupportedTokenType {
                token_type: token.token_type,
            });
        }

        Ok(SecretString::new(token.access_token.into()))
    }
}

impl Default for DmdataClient {
    fn default() -> Self {
        Self::new()
    }
}

fn api_error(error: Option<ApiErrorDto>) -> DmdataClientError {
    match error {
        Some(error) => DmdataClientError::Api {
            code: error.code,
            message: error.message,
        },
        None => DmdataClientError::Api {
            code: 0,
            message: "unknown API error".to_owned(),
        },
    }
}

fn secret_from_env(name: &str) -> Result<SecretString, DmdataClientError> {
    env_value(name).map(|value| SecretString::new(value.into()))
}

fn env_value(name: &str) -> Result<String, DmdataClientError> {
    std::env::var(name)
        .map(|value| value.trim().to_owned())
        .ok()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| DmdataClientError::MissingEnvironmentVariable {
            name: name.to_owned(),
        })
}

fn non_empty_trimmed(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn oauth_scope(config: &DmdataConfig) -> String {
    if !config.oauth_scopes.is_empty() {
        return config.oauth_scopes.join(" ");
    }

    derived_oauth_scopes(&config.classifications).join(" ")
}

fn derived_oauth_scopes(classifications: &[String]) -> Vec<String> {
    let mut scopes = vec!["socket.close".to_owned(), "socket.start".to_owned()];

    for classification in classifications {
        match classification.as_str() {
            "telegram.earthquake" => scopes.push("telegram.get.earthquake".to_owned()),
            "eew.forecast" => scopes.push("eew.get.forecast".to_owned()),
            "eew.warning" => scopes.push("eew.get.warning".to_owned()),
            _ => {}
        }
    }

    scopes.sort();
    scopes.dedup();
    scopes
}

fn socket_close_url(socket_start_url: &str, socket_id: &str) -> String {
    format!(
        "{}/{}",
        socket_start_url.trim_end_matches('/'),
        socket_id.trim()
    )
}

fn http_error_message(body: &str) -> String {
    serde_json::from_str::<SocketStartResponseDto>(body)
        .ok()
        .and_then(|response| response.error)
        .map(|error| format!("API error {}: {}", error.code, error.message))
        .or_else(|| {
            serde_json::from_str::<ApiErrorDto>(body)
                .ok()
                .map(|error| format!("API error {}: {}", error.code, error.message))
        })
        .unwrap_or_else(|| short_body(body))
}

fn short_body(body: &str) -> String {
    let body = body.trim();
    if body.is_empty() {
        return "empty response body".to_owned();
    }

    const MAX_LEN: usize = 300;
    let shortened: String = body.chars().take(MAX_LEN).collect();
    if body.chars().count() > MAX_LEN {
        format!("{shortened}...")
    } else {
        shortened
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> DmdataConfig {
        DmdataConfig {
            socket_start_url: "https://api.dmdata.jp/v2/socket".to_owned(),
            token_endpoint_url: "https://manager.dmdata.jp/account/oauth2/v1/token".to_owned(),
            websocket_url: Some("wss://example.invalid/socket?ticket=placeholder".to_owned()),
            auth_mode: "access_token".to_owned(),
            api_token_env: "DMDATA_API_TOKEN".to_owned(),
            client_id: None,
            client_id_env: "DMDATA_CLIENT_ID".to_owned(),
            client_secret: None,
            client_secret_env: "DMDATA_CLIENT_SECRET".to_owned(),
            oauth_scopes: Vec::new(),
            classifications: vec!["telegram.earthquake".to_owned()],
            types: vec!["VXSE51".to_owned()],
            test: "no".to_owned(),
            app_name: "eew-quake-notify".to_owned(),
            format_mode: "json".to_owned(),
            reconnect_initial_ms: 1_000,
            reconnect_max_ms: 30_000,
        }
    }

    #[tokio::test]
    async fn uses_websocket_override_without_network() {
        let client = DmdataClient::new();

        let start = client
            .start_socket(&config())
            .await
            .expect("override should produce socket start");

        assert_eq!(start.id, None);
        assert_eq!(start.url, "wss://example.invalid/socket?ticket=placeholder");
        assert_eq!(start.protocols, ["dmdata.v2"]);
    }

    #[test]
    fn extracts_http_error_message_from_socket_start_response() {
        let message = http_error_message(
            r#"{
              "responseId": "abc",
              "responseTime": "2026-06-26T14:09:22.000Z",
              "status": "error",
              "error": {
                "message": "Unauthorized",
                "code": 401
              }
            }"#,
        );

        assert_eq!(message, "API error 401: Unauthorized");
    }

    #[test]
    fn uses_short_body_for_non_json_http_error() {
        assert_eq!(http_error_message("invalid token"), "invalid token");
        assert_eq!(http_error_message(""), "empty response body");
    }

    #[test]
    fn derives_oauth_scopes_from_classifications() {
        let scopes = derived_oauth_scopes(&[
            "telegram.earthquake".to_owned(),
            "eew.warning".to_owned(),
            "eew.forecast".to_owned(),
            "eew.warning".to_owned(),
        ]);

        assert_eq!(
            scopes,
            [
                "eew.get.forecast",
                "eew.get.warning",
                "socket.close",
                "socket.start",
                "telegram.get.earthquake"
            ]
        );
    }

    #[test]
    fn builds_socket_close_url_from_start_url() {
        assert_eq!(
            socket_close_url("https://api.dmdata.jp/v2/socket/", "12345"),
            "https://api.dmdata.jp/v2/socket/12345"
        );
    }
}
