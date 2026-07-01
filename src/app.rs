use std::path::PathBuf;

use thiserror::Error;

use crate::{
    config::AppConfig,
    event::EarthquakeEvent,
    message,
    notify::{Notifier, NotifyError},
    rules::{DeliveryDecision, should_notify, should_speak},
    state::{DedupDecision, DedupState},
    tts::{SpeechError, SpeechSynthesizer},
};

#[derive(Debug)]
pub struct AppPipeline<N, S> {
    config: AppConfig,
    dedup_state: DedupState,
    dedup_state_file: Option<PathBuf>,
    notifier: N,
    speech_synthesizer: S,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessOutcome {
    pub dedup: DedupDecision,
    pub notification: AdapterOutcome,
    pub speech: AdapterOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterOutcome {
    Sent,
    Skipped(crate::rules::DeliverySkipReason),
    Failed(String),
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("failed to send notification: {0}")]
    Notify(#[from] NotifyError),
    #[error("failed to synthesize speech: {0}")]
    Speech(#[from] SpeechError),
}

impl<N, S> AppPipeline<N, S>
where
    N: Notifier,
    S: SpeechSynthesizer,
{
    pub fn new(config: AppConfig, notifier: N, speech_synthesizer: S) -> Self {
        Self {
            config,
            dedup_state: DedupState::new(),
            dedup_state_file: None,
            notifier,
            speech_synthesizer,
        }
    }

    pub fn new_with_persistent_state(
        config: AppConfig,
        notifier: N,
        speech_synthesizer: S,
    ) -> Self {
        let dedup_state_file = config
            .state
            .enabled
            .then(|| PathBuf::from(config.state.file_path.trim()));
        let dedup_state = dedup_state_file
            .as_ref()
            .map(
                |path| match DedupState::load_from_path(path, config.state.max_entries) {
                    Ok(state) => {
                        tracing::info!(
                            path = %path.display(),
                            entries = state.len(),
                            "dedup state loaded"
                        );
                        state
                    }
                    Err(error) => {
                        tracing::warn!(
                            path = %path.display(),
                            %error,
                            "failed to load dedup state; starting with empty state"
                        );
                        DedupState::new()
                    }
                },
            )
            .unwrap_or_else(DedupState::new);

        Self {
            config,
            dedup_state,
            dedup_state_file,
            notifier,
            speech_synthesizer,
        }
    }

    pub fn with_dedup_state(
        config: AppConfig,
        dedup_state: DedupState,
        notifier: N,
        speech_synthesizer: S,
    ) -> Self {
        Self {
            config,
            dedup_state,
            dedup_state_file: None,
            notifier,
            speech_synthesizer,
        }
    }

    pub async fn process_event(
        &mut self,
        event: &EarthquakeEvent,
    ) -> Result<ProcessOutcome, AppError> {
        let dedup = self.dedup_state.should_process_and_mark(event);
        if matches!(dedup, DedupDecision::Duplicate) {
            return Ok(ProcessOutcome {
                dedup,
                notification: AdapterOutcome::Skipped(crate::rules::DeliverySkipReason::Duplicate),
                speech: AdapterOutcome::Skipped(crate::rules::DeliverySkipReason::Duplicate),
            });
        }

        let notification = match should_notify(event, &self.config.notify) {
            DeliveryDecision::Send => {
                let message = message::notification_message(event);
                match self.notifier.notify(&message) {
                    Ok(()) => AdapterOutcome::Sent,
                    Err(error) => {
                        tracing::warn!(source_id = ?event.source_id, %error, "desktop notification failed");
                        AdapterOutcome::Failed(error.to_string())
                    }
                }
            }
            DeliveryDecision::Skip(reason) => AdapterOutcome::Skipped(reason),
        };

        let speech = match should_speak(event, &self.config.tts) {
            DeliveryDecision::Send => {
                let text = message::speech_text(event);
                match self.speech_synthesizer.speak(&text).await {
                    Ok(()) => AdapterOutcome::Sent,
                    Err(error) => {
                        tracing::warn!(source_id = ?event.source_id, %error, "speech synthesis failed");
                        AdapterOutcome::Failed(error.to_string())
                    }
                }
            }
            DeliveryDecision::Skip(reason) => AdapterOutcome::Skipped(reason),
        };

        self.flush_dedup_state();

        Ok(ProcessOutcome {
            dedup,
            notification,
            speech,
        })
    }

    pub fn flush_dedup_state(&mut self) {
        let Some(path) = self.dedup_state_file.as_ref() else {
            return;
        };

        if let Err(error) = self
            .dedup_state
            .save_to_path(path, self.config.state.max_entries)
        {
            tracing::warn!(
                path = %path.display(),
                %error,
                "failed to save dedup state"
            );
        } else {
            tracing::debug!(
                path = %path.display(),
                entries = self.dedup_state.len(),
                "dedup state saved"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        future::Future,
        pin::Pin,
        sync::{Arc, Mutex},
    };

    use crate::{
        config::{AppConfig, DmdataConfig, LogConfig, NotifyConfig, TtsConfig},
        config::{RuntimeConfig, StateConfig},
        event::{EventKind, Hypocenter, Intensity, Magnitude},
        message::NotificationMessage,
        rules::DeliverySkipReason,
        tts::SpeechError,
    };

    #[derive(Debug, Clone)]
    struct FakeNotifier {
        messages: Arc<Mutex<Vec<NotificationMessage>>>,
        fail: bool,
    }

    #[derive(Debug, Clone)]
    struct FakeSpeechSynthesizer {
        texts: Arc<Mutex<Vec<String>>>,
        fail: bool,
    }

    impl FakeNotifier {
        fn new() -> Self {
            Self {
                messages: Arc::new(Mutex::new(Vec::new())),
                fail: false,
            }
        }

        fn failing() -> Self {
            Self {
                messages: Arc::new(Mutex::new(Vec::new())),
                fail: true,
            }
        }
    }

    impl FakeSpeechSynthesizer {
        fn new() -> Self {
            Self {
                texts: Arc::new(Mutex::new(Vec::new())),
                fail: false,
            }
        }

        fn failing() -> Self {
            Self {
                texts: Arc::new(Mutex::new(Vec::new())),
                fail: true,
            }
        }
    }

    impl Notifier for FakeNotifier {
        fn notify(&self, message: &NotificationMessage) -> Result<(), NotifyError> {
            if self.fail {
                return Err(NotifyError::Disabled);
            }
            self.messages.lock().unwrap().push(message.clone());
            Ok(())
        }
    }

    impl SpeechSynthesizer for FakeSpeechSynthesizer {
        fn speak<'a>(
            &'a self,
            text: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<(), SpeechError>> + Send + 'a>> {
            if self.fail {
                return Box::pin(async { Err(SpeechError::Disabled) });
            }
            self.texts.lock().unwrap().push(text.to_owned());
            Box::pin(async { Ok(()) })
        }
    }

    fn config(notify_min: Option<Intensity>, tts_min: Option<Intensity>) -> AppConfig {
        AppConfig {
            dmdata: DmdataConfig {
                socket_start_url: "https://api.dmdata.jp/v2/socket".to_owned(),
                token_endpoint_url: "https://manager.dmdata.jp/account/oauth2/v1/token".to_owned(),
                websocket_url: Some("wss://example.invalid/dmdata-websocket".to_owned()),
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
            },
            notify: NotifyConfig {
                desktop_enabled: true,
                enabled_kinds: vec![
                    EventKind::Earthquake,
                    EventKind::IntensityReport,
                    EventKind::EewWarning,
                ],
                min_intensity: notify_min,
            },
            tts: TtsConfig {
                enabled: true,
                engine: "voicevox".to_owned(),
                voicevox_url: "http://127.0.0.1:50021".to_owned(),
                speaker: 1,
                enabled_kinds: vec![
                    EventKind::Earthquake,
                    EventKind::IntensityReport,
                    EventKind::EewWarning,
                ],
                min_intensity: tts_min,
            },
            state: StateConfig {
                enabled: false,
                file_path: "state/dedup-state.json".to_owned(),
                max_entries: 1000,
            },
            runtime: RuntimeConfig {
                single_instance: true,
                lock_file_path: "state/app.lock".to_owned(),
            },
            log: LogConfig {
                level: "info".to_owned(),
                console_enabled: true,
                file_enabled: false,
                file_path: "logs/eew-quake-notify.log".to_owned(),
            },
        }
    }

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
            affected_areas: vec!["東京都".to_owned(), "神奈川県".to_owned()],
        }
    }

    #[tokio::test]
    async fn sends_notification_and_speech_for_new_matching_event() {
        let notifier = FakeNotifier::new();
        let speech = FakeSpeechSynthesizer::new();
        let mut pipeline = AppPipeline::new(
            config(Some(Intensity::Three), Some(Intensity::Three)),
            notifier.clone(),
            speech.clone(),
        );

        let outcome = pipeline
            .process_event(&event(Some(Intensity::Four)))
            .await
            .expect("event should process");

        assert_eq!(outcome.dedup, DedupDecision::New);
        assert_eq!(outcome.notification, AdapterOutcome::Sent);
        assert_eq!(outcome.speech, AdapterOutcome::Sent);
        assert_eq!(notifier.messages.lock().unwrap().len(), 1);
        assert_eq!(speech.texts.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn skips_duplicate_event() {
        let notifier = FakeNotifier::new();
        let speech = FakeSpeechSynthesizer::new();
        let mut pipeline = AppPipeline::new(
            config(Some(Intensity::Three), Some(Intensity::Three)),
            notifier.clone(),
            speech.clone(),
        );
        let event = event(Some(Intensity::Four));

        pipeline
            .process_event(&event)
            .await
            .expect("first event should process");
        let outcome = pipeline
            .process_event(&event)
            .await
            .expect("duplicate should skip cleanly");

        assert_eq!(outcome.dedup, DedupDecision::Duplicate);
        assert_eq!(
            outcome.notification,
            AdapterOutcome::Skipped(DeliverySkipReason::Duplicate)
        );
        assert_eq!(notifier.messages.lock().unwrap().len(), 1);
        assert_eq!(speech.texts.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn applies_notification_and_speech_thresholds_independently() {
        let notifier = FakeNotifier::new();
        let speech = FakeSpeechSynthesizer::new();
        let mut pipeline = AppPipeline::new(
            config(Some(Intensity::Three), Some(Intensity::FiveLower)),
            notifier.clone(),
            speech.clone(),
        );

        let outcome = pipeline
            .process_event(&event(Some(Intensity::Four)))
            .await
            .expect("event should process");

        assert_eq!(outcome.notification, AdapterOutcome::Sent);
        assert_eq!(
            outcome.speech,
            AdapterOutcome::Skipped(DeliverySkipReason::BelowMinIntensity)
        );
        assert_eq!(notifier.messages.lock().unwrap().len(), 1);
        assert!(speech.texts.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn tries_speech_when_notification_fails() {
        let notifier = FakeNotifier::failing();
        let speech = FakeSpeechSynthesizer::new();
        let mut pipeline = AppPipeline::new(
            config(Some(Intensity::Three), Some(Intensity::Three)),
            notifier,
            speech.clone(),
        );

        let outcome = pipeline
            .process_event(&event(Some(Intensity::Four)))
            .await
            .expect("adapter failures should be reported in outcome");

        assert!(matches!(outcome.notification, AdapterOutcome::Failed(_)));
        assert_eq!(outcome.speech, AdapterOutcome::Sent);
        assert_eq!(speech.texts.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn keeps_notification_result_when_speech_fails() {
        let notifier = FakeNotifier::new();
        let speech = FakeSpeechSynthesizer::failing();
        let mut pipeline = AppPipeline::new(
            config(Some(Intensity::Three), Some(Intensity::Three)),
            notifier.clone(),
            speech,
        );

        let outcome = pipeline
            .process_event(&event(Some(Intensity::Four)))
            .await
            .expect("adapter failures should be reported in outcome");

        assert_eq!(outcome.notification, AdapterOutcome::Sent);
        assert!(matches!(outcome.speech, AdapterOutcome::Failed(_)));
        assert_eq!(notifier.messages.lock().unwrap().len(), 1);
    }
}
