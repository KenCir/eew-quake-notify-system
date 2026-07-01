use std::{fs, path::Path};

use serde::Deserialize;
use thiserror::Error;
use url::Url;

use crate::event::{EventKind, Intensity};

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub dmdata: DmdataConfig,
    pub notify: NotifyConfig,
    pub tts: TtsConfig,
    #[serde(default)]
    pub runtime: RuntimeConfig,
    #[serde(default)]
    pub state: StateConfig,
    #[serde(default)]
    pub log: LogConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DmdataConfig {
    #[serde(default = "default_socket_start_url")]
    pub socket_start_url: String,
    #[serde(default = "default_token_endpoint_url")]
    pub token_endpoint_url: String,
    #[serde(default)]
    pub websocket_url: Option<String>,
    #[serde(default = "default_auth_mode")]
    pub auth_mode: String,
    #[serde(default = "default_api_token_env")]
    pub api_token_env: String,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default = "default_client_id_env")]
    pub client_id_env: String,
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default = "default_client_secret_env")]
    pub client_secret_env: String,
    #[serde(default)]
    pub oauth_scopes: Vec<String>,
    #[serde(default = "default_dmdata_classifications")]
    pub classifications: Vec<String>,
    #[serde(default = "default_dmdata_types")]
    pub types: Vec<String>,
    #[serde(default = "default_dmdata_test")]
    pub test: String,
    #[serde(default = "default_dmdata_app_name")]
    pub app_name: String,
    #[serde(default = "default_dmdata_format_mode")]
    pub format_mode: String,
    #[serde(default = "default_reconnect_initial_ms")]
    pub reconnect_initial_ms: u64,
    #[serde(default = "default_reconnect_max_ms")]
    pub reconnect_max_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NotifyConfig {
    #[serde(default = "default_true")]
    pub desktop_enabled: bool,
    #[serde(default = "default_enabled_event_kinds")]
    pub enabled_kinds: Vec<EventKind>,
    pub min_intensity: Option<Intensity>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TtsConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_tts_engine")]
    pub engine: String,
    #[serde(default = "default_voicevox_url")]
    pub voicevox_url: String,
    #[serde(default = "default_voicevox_speaker")]
    pub speaker: u32,
    #[serde(default = "default_enabled_event_kinds")]
    pub enabled_kinds: Vec<EventKind>,
    pub min_intensity: Option<Intensity>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StateConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_state_file")]
    pub file_path: String,
    #[serde(default = "default_state_max_entries")]
    pub max_entries: usize,
}

impl Default for StateConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            file_path: default_state_file(),
            max_entries: default_state_max_entries(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeConfig {
    #[serde(default = "default_true")]
    pub single_instance: bool,
    #[serde(default = "default_lock_file")]
    pub lock_file_path: String,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            single_instance: default_true(),
            lock_file_path: default_lock_file(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_true")]
    pub console_enabled: bool,
    #[serde(default)]
    pub file_enabled: bool,
    #[serde(default = "default_log_file")]
    pub file_path: String,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            console_enabled: default_true(),
            file_enabled: false,
            file_path: default_log_file(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse config TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("invalid config: {0}")]
    Invalid(String),
}

impl AppConfig {
    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.display().to_string(),
            source,
        })?;
        Self::load_from_str(&text)
    }

    pub fn load_from_str(text: &str) -> Result<Self, ConfigError> {
        let config: Self = toml::from_str(text)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_url(
            "dmdata.socket_start_url",
            &self.dmdata.socket_start_url,
            &["https", "http"],
        )?;
        validate_url(
            "dmdata.token_endpoint_url",
            &self.dmdata.token_endpoint_url,
            &["https", "http"],
        )?;
        if let Some(websocket_url) = self.dmdata.websocket_url.as_deref() {
            validate_url("dmdata.websocket_url", websocket_url, &["wss", "ws"])?;
        }
        validate_url(
            "tts.voicevox_url",
            &self.tts.voicevox_url,
            &["http", "https"],
        )?;

        match self.dmdata.auth_mode.as_str() {
            "access_token" => {
                if self.dmdata.api_token_env.trim().is_empty() {
                    return Err(ConfigError::Invalid(
                        "dmdata.api_token_env must not be empty when auth_mode is 'access_token'"
                            .to_owned(),
                    ));
                }
            }
            "client_credentials" => {
                if optional_secret_is_empty(self.dmdata.client_id.as_deref())
                    && self.dmdata.client_id_env.trim().is_empty()
                {
                    return Err(ConfigError::Invalid(
                        "dmdata.client_id or dmdata.client_id_env must be set when auth_mode is 'client_credentials'"
                            .to_owned(),
                    ));
                }

                if optional_secret_is_empty(self.dmdata.client_secret.as_deref())
                    && self.dmdata.client_secret_env.trim().is_empty()
                {
                    return Err(ConfigError::Invalid(
                        "dmdata.client_secret or dmdata.client_secret_env must be set when auth_mode is 'client_credentials'"
                            .to_owned(),
                    ));
                }
            }
            _ => {
                return Err(ConfigError::Invalid(
                    "dmdata.auth_mode must be 'client_credentials' or 'access_token'".to_owned(),
                ));
            }
        }

        if self.dmdata.classifications.is_empty() {
            return Err(ConfigError::Invalid(
                "dmdata.classifications must not be empty".to_owned(),
            ));
        }

        for classification in &self.dmdata.classifications {
            validate_dmdata_classification(classification)?;
        }

        validate_oauth_scopes(&self.dmdata)?;
        validate_enabled_kinds("notify.enabled_kinds", &self.notify.enabled_kinds)?;
        validate_enabled_kinds("tts.enabled_kinds", &self.tts.enabled_kinds)?;

        for data_type in &self.dmdata.types {
            if data_type.trim().is_empty() {
                return Err(ConfigError::Invalid(
                    "dmdata.types must not contain empty entries".to_owned(),
                ));
            }
        }

        if self.dmdata.types.len() > 30 {
            return Err(ConfigError::Invalid(
                "dmdata.types must contain 30 or fewer entries".to_owned(),
            ));
        }

        if !matches!(self.dmdata.test.as_str(), "no" | "including") {
            return Err(ConfigError::Invalid(
                "dmdata.test must be 'no' or 'including'".to_owned(),
            ));
        }

        if self.dmdata.app_name.len() > 24 {
            return Err(ConfigError::Invalid(
                "dmdata.app_name must be 24 bytes or fewer".to_owned(),
            ));
        }

        if self.dmdata.format_mode != "json" {
            return Err(ConfigError::Invalid(
                "dmdata.format_mode currently supports only 'json'".to_owned(),
            ));
        }

        if self.dmdata.reconnect_initial_ms == 0 {
            return Err(ConfigError::Invalid(
                "dmdata.reconnect_initial_ms must be greater than 0".to_owned(),
            ));
        }

        if self.dmdata.reconnect_initial_ms > self.dmdata.reconnect_max_ms {
            return Err(ConfigError::Invalid(
                "dmdata.reconnect_initial_ms must be less than or equal to reconnect_max_ms"
                    .to_owned(),
            ));
        }

        if self.tts.engine != "voicevox" {
            return Err(ConfigError::Invalid(
                "tts.engine currently supports only 'voicevox'".to_owned(),
            ));
        }

        if self.state.enabled {
            if self.state.file_path.trim().is_empty() {
                return Err(ConfigError::Invalid(
                    "state.file_path must not be empty when state.enabled is true".to_owned(),
                ));
            }

            if self.state.max_entries == 0 {
                return Err(ConfigError::Invalid(
                    "state.max_entries must be greater than 0 when state.enabled is true"
                        .to_owned(),
                ));
            }
        }

        if self.runtime.single_instance && self.runtime.lock_file_path.trim().is_empty() {
            return Err(ConfigError::Invalid(
                "runtime.lock_file_path must not be empty when runtime.single_instance is true"
                    .to_owned(),
            ));
        }

        if !self.log.console_enabled && !self.log.file_enabled {
            return Err(ConfigError::Invalid(
                "log.console_enabled and log.file_enabled must not both be false".to_owned(),
            ));
        }

        if self.log.file_enabled && self.log.file_path.trim().is_empty() {
            return Err(ConfigError::Invalid(
                "log.file_path must not be empty when log.file_enabled is true".to_owned(),
            ));
        }

        Ok(())
    }
}

fn validate_url(field: &str, value: &str, allowed_schemes: &[&str]) -> Result<(), ConfigError> {
    let url = Url::parse(value)
        .map_err(|error| ConfigError::Invalid(format!("{field} is not a valid URL: {error}")))?;
    if !allowed_schemes.contains(&url.scheme()) {
        return Err(ConfigError::Invalid(format!(
            "{field} must use one of these schemes: {}",
            allowed_schemes.join(", ")
        )));
    }
    Ok(())
}

fn optional_secret_is_empty(value: Option<&str>) -> bool {
    value.map(|value| value.trim().is_empty()).unwrap_or(true)
}

fn validate_dmdata_classification(classification: &str) -> Result<(), ConfigError> {
    match classification.trim() {
        "telegram.earthquake" | "eew.forecast" | "eew.warning" => Ok(()),
        "" => Err(ConfigError::Invalid(
            "dmdata.classifications must not contain empty entries".to_owned(),
        )),
        value => Err(ConfigError::Invalid(format!(
            "unsupported dmdata.classifications entry for this app: {value}"
        ))),
    }
}

fn validate_oauth_scopes(config: &DmdataConfig) -> Result<(), ConfigError> {
    if config.auth_mode != "client_credentials" || config.oauth_scopes.is_empty() {
        return Ok(());
    }

    let required = required_oauth_scopes(&config.classifications);
    for scope in required {
        if !config.oauth_scopes.iter().any(|value| value == &scope) {
            return Err(ConfigError::Invalid(format!(
                "dmdata.oauth_scopes must include '{scope}' when scopes are set explicitly"
            )));
        }
    }

    Ok(())
}

fn validate_enabled_kinds(field: &str, kinds: &[EventKind]) -> Result<(), ConfigError> {
    if kinds.iter().any(|kind| matches!(kind, EventKind::Unknown)) {
        return Err(ConfigError::Invalid(format!(
            "{field} must not contain 'unknown'"
        )));
    }

    Ok(())
}

fn required_oauth_scopes(classifications: &[String]) -> Vec<String> {
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

fn default_true() -> bool {
    true
}

fn default_socket_start_url() -> String {
    "https://api.dmdata.jp/v2/socket".to_owned()
}

fn default_token_endpoint_url() -> String {
    "https://manager.dmdata.jp/account/oauth2/v1/token".to_owned()
}

fn default_auth_mode() -> String {
    "access_token".to_owned()
}

fn default_api_token_env() -> String {
    "DMDATA_API_TOKEN".to_owned()
}

fn default_client_id_env() -> String {
    "DMDATA_CLIENT_ID".to_owned()
}

fn default_client_secret_env() -> String {
    "DMDATA_CLIENT_SECRET".to_owned()
}

fn default_dmdata_classifications() -> Vec<String> {
    vec!["telegram.earthquake".to_owned(), "eew.warning".to_owned()]
}

fn default_dmdata_types() -> Vec<String> {
    vec![
        "VXSE43".to_owned(),
        "VXSE44".to_owned(),
        "VXSE45".to_owned(),
        "VXSE51".to_owned(),
        "VXSE52".to_owned(),
        "VXSE53".to_owned(),
        "VXSE62".to_owned(),
    ]
}

fn default_dmdata_test() -> String {
    "no".to_owned()
}

fn default_dmdata_app_name() -> String {
    "eew-quake-notify".to_owned()
}

fn default_dmdata_format_mode() -> String {
    "json".to_owned()
}

fn default_reconnect_initial_ms() -> u64 {
    1_000
}

fn default_reconnect_max_ms() -> u64 {
    30_000
}

fn default_tts_engine() -> String {
    "voicevox".to_owned()
}

fn default_voicevox_url() -> String {
    "http://127.0.0.1:50021".to_owned()
}

fn default_voicevox_speaker() -> u32 {
    1
}

fn default_enabled_event_kinds() -> Vec<EventKind> {
    vec![
        EventKind::Earthquake,
        EventKind::IntensityReport,
        EventKind::EewWarning,
    ]
}

fn default_state_file() -> String {
    "state/dedup-state.json".to_owned()
}

fn default_lock_file() -> String {
    "state/app.lock".to_owned()
}

fn default_state_max_entries() -> usize {
    1_000
}

fn default_log_level() -> String {
    "info".to_owned()
}

fn default_log_file() -> String {
    "logs/eew-quake-notify.log".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_CONFIG: &str = r#"
[dmdata]
socket_start_url = "https://api.dmdata.jp/v2/socket"
token_endpoint_url = "https://manager.dmdata.jp/account/oauth2/v1/token"
auth_mode = "client_credentials"
api_token_env = "DMDATA_API_TOKEN"
client_id_env = "DMDATA_CLIENT_ID"
client_secret_env = "DMDATA_CLIENT_SECRET"
oauth_scopes = []
classifications = ["telegram.earthquake", "eew.forecast", "eew.warning"]
types = ["VXSE43", "VXSE44", "VXSE45", "VXSE51", "VXSE52", "VXSE53", "VXSE62"]
test = "no"
app_name = "eew-quake-notify"
format_mode = "json"
reconnect_initial_ms = 1000
reconnect_max_ms = 30000

[notify]
desktop_enabled = true
enabled_kinds = ["earthquake", "intensity_report", "eew_warning"]
min_intensity = "3"

[tts]
enabled = true
engine = "voicevox"
voicevox_url = "http://127.0.0.1:50021"
speaker = 1
enabled_kinds = ["earthquake", "intensity_report", "eew_warning"]
min_intensity = "4"

[state]
enabled = true
file_path = "state/dedup-state.json"
max_entries = 1000

[runtime]
single_instance = true
lock_file_path = "state/app.lock"

[log]
level = "info"
console_enabled = true
file_enabled = false
file_path = "logs/eew-quake-notify.log"
"#;

    #[test]
    fn loads_valid_config() {
        let config = AppConfig::load_from_str(VALID_CONFIG).expect("config should load");

        assert_eq!(config.dmdata.auth_mode, "client_credentials");
        assert_eq!(config.dmdata.client_id, None);
        assert_eq!(config.dmdata.client_id_env, "DMDATA_CLIENT_ID");
        assert_eq!(
            config.dmdata.classifications,
            ["telegram.earthquake", "eew.forecast", "eew.warning"]
        );
        assert_eq!(config.notify.min_intensity, Some(Intensity::Three));
        assert_eq!(
            config.notify.enabled_kinds,
            [
                EventKind::Earthquake,
                EventKind::IntensityReport,
                EventKind::EewWarning
            ]
        );
        assert_eq!(config.tts.min_intensity, Some(Intensity::Four));
        assert!(config.runtime.single_instance);
        assert_eq!(config.runtime.lock_file_path, "state/app.lock");
        assert!(config.state.enabled);
        assert_eq!(config.state.max_entries, 1000);
        assert!(config.log.console_enabled);
        assert!(!config.log.file_enabled);
    }

    #[test]
    fn accepts_client_credentials_from_config_values() {
        let text = VALID_CONFIG
            .replace(
                "client_id_env = \"DMDATA_CLIENT_ID\"",
                "client_id_env = \"\"",
            )
            .replace(
                "client_secret_env = \"DMDATA_CLIENT_SECRET\"",
                "client_secret_env = \"\"",
            )
            .replace(
                "client_id_env = \"\"",
                "client_id = \"CId.placeholder\"\nclient_id_env = \"\"",
            )
            .replace(
                "client_secret_env = \"\"",
                "client_secret = \"CSt.placeholder\"\nclient_secret_env = \"\"",
            );

        let config = AppConfig::load_from_str(&text).expect("config should load");

        assert_eq!(config.dmdata.client_id.as_deref(), Some("CId.placeholder"));
        assert_eq!(
            config.dmdata.client_secret.as_deref(),
            Some("CSt.placeholder")
        );
    }

    #[test]
    fn rejects_invalid_socket_start_scheme() {
        let text = VALID_CONFIG.replace(
            "https://api.dmdata.jp/v2/socket",
            "wss://example.invalid/socket",
        );

        let error = AppConfig::load_from_str(&text).expect_err("config should fail");

        assert!(error.to_string().contains("dmdata.socket_start_url"));
    }

    #[test]
    fn rejects_invalid_reconnect_range() {
        let text = VALID_CONFIG.replace(
            "reconnect_initial_ms = 1000",
            "reconnect_initial_ms = 60000",
        );

        let error = AppConfig::load_from_str(&text).expect_err("config should fail");

        assert!(error.to_string().contains("reconnect_initial_ms"));
    }

    #[test]
    fn rejects_invalid_auth_mode() {
        let text = VALID_CONFIG.replace(
            "auth_mode = \"client_credentials\"",
            "auth_mode = \"api_key\"",
        );

        let error = AppConfig::load_from_str(&text).expect_err("config should fail");

        assert!(error.to_string().contains("dmdata.auth_mode"));
    }

    #[test]
    fn rejects_empty_state_path_when_enabled() {
        let text =
            VALID_CONFIG.replace("file_path = \"state/dedup-state.json\"", "file_path = \"\"");

        let error = AppConfig::load_from_str(&text).expect_err("config should fail");

        assert!(error.to_string().contains("state.file_path"));
    }

    #[test]
    fn rejects_empty_lock_path_when_single_instance_enabled() {
        let text = VALID_CONFIG.replace(
            "lock_file_path = \"state/app.lock\"",
            "lock_file_path = \"\"",
        );

        let error = AppConfig::load_from_str(&text).expect_err("config should fail");

        assert!(error.to_string().contains("runtime.lock_file_path"));
    }

    #[test]
    fn rejects_empty_log_file_path_when_file_log_enabled() {
        let text = VALID_CONFIG
            .replace("file_enabled = false", "file_enabled = true")
            .replace(
                "file_path = \"logs/eew-quake-notify.log\"",
                "file_path = \"\"",
            );

        let error = AppConfig::load_from_str(&text).expect_err("config should fail");

        assert!(error.to_string().contains("log.file_path"));
    }

    #[test]
    fn rejects_unsupported_classification() {
        let text = VALID_CONFIG.replace(
            "classifications = [\"telegram.earthquake\", \"eew.forecast\", \"eew.warning\"]",
            "classifications = [\"telegram.weather\"]",
        );

        let error = AppConfig::load_from_str(&text).expect_err("config should fail");

        assert!(error.to_string().contains("dmdata.classifications"));
    }

    #[test]
    fn rejects_explicit_oauth_scopes_missing_socket_close() {
        let text = VALID_CONFIG.replace(
            "oauth_scopes = []",
            "oauth_scopes = [\"socket.start\", \"telegram.get.earthquake\", \"eew.get.forecast\", \"eew.get.warning\"]",
        );

        let error = AppConfig::load_from_str(&text).expect_err("config should fail");

        assert!(error.to_string().contains("socket.close"));
    }

    #[test]
    fn rejects_unknown_enabled_kind() {
        let text = VALID_CONFIG.replace(
            "enabled_kinds = [\"earthquake\", \"intensity_report\", \"eew_warning\"]",
            "enabled_kinds = [\"unknown\"]",
        );

        let error = AppConfig::load_from_str(&text).expect_err("config should fail");

        assert!(error.to_string().contains("enabled_kinds"));
    }
}
