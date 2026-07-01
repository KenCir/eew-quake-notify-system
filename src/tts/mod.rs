mod voicevox;

use std::{future::Future, pin::Pin};

use thiserror::Error;

pub use voicevox::{VoicevoxError, VoicevoxSpeechSynthesizer};

pub trait SpeechSynthesizer {
    fn speak<'a>(
        &'a self,
        text: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), SpeechError>> + Send + 'a>>;
}

#[derive(Debug, Error)]
pub enum SpeechError {
    #[error("text to speech is disabled in config")]
    Disabled,
    #[error(transparent)]
    Voicevox(#[from] VoicevoxError),
    #[error("failed to open default audio output: {0}")]
    AudioOutput(#[from] rodio::DeviceSinkError),
    #[error("failed to play synthesized audio: {0}")]
    AudioPlayback(#[from] rodio::PlayError),
    #[error("audio playback task failed: {0}")]
    PlaybackTask(#[from] tokio::task::JoinError),
}

pub async fn speak_if_enabled<S: SpeechSynthesizer>(
    synthesizer: &S,
    enabled: bool,
    text: &str,
) -> Result<(), SpeechError> {
    if !enabled {
        return Err(SpeechError::Disabled);
    }

    synthesizer.speak(text).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Clone)]
    struct FakeSpeechSynthesizer {
        spoken_texts: Arc<Mutex<Vec<String>>>,
    }

    impl FakeSpeechSynthesizer {
        fn new() -> Self {
            Self {
                spoken_texts: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl SpeechSynthesizer for FakeSpeechSynthesizer {
        fn speak<'a>(
            &'a self,
            text: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<(), SpeechError>> + Send + 'a>> {
            self.spoken_texts.lock().unwrap().push(text.to_owned());
            Box::pin(async { Ok(()) })
        }
    }

    #[tokio::test]
    async fn speaks_when_enabled() {
        let synthesizer = FakeSpeechSynthesizer::new();

        speak_if_enabled(&synthesizer, true, "テストです。")
            .await
            .expect("speech should be sent");

        assert_eq!(
            synthesizer.spoken_texts.lock().unwrap().as_slice(),
            &["テストです。"]
        );
    }

    #[tokio::test]
    async fn rejects_when_disabled() {
        let synthesizer = FakeSpeechSynthesizer::new();

        let error = speak_if_enabled(&synthesizer, false, "テストです。")
            .await
            .expect_err("speech should be disabled");

        assert!(matches!(error, SpeechError::Disabled));
        assert!(synthesizer.spoken_texts.lock().unwrap().is_empty());
    }
}
