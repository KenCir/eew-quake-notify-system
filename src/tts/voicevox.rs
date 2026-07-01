use std::{future::Future, io::Cursor, pin::Pin};

use reqwest::{Client, StatusCode, Url};
use thiserror::Error;

use super::{SpeechError, SpeechSynthesizer};

#[derive(Debug, Clone)]
pub struct VoicevoxSpeechSynthesizer {
    client: Client,
    base_url: Url,
    speaker: u32,
}

#[derive(Debug, Error)]
pub enum VoicevoxError {
    #[error("VOICEVOX request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("VOICEVOX {endpoint} returned {status}: {body}")]
    Status {
        endpoint: &'static str,
        status: StatusCode,
        body: String,
    },
}

impl VoicevoxSpeechSynthesizer {
    pub fn new(base_url: &str, speaker: u32) -> Result<Self, url::ParseError> {
        Ok(Self {
            client: Client::new(),
            base_url: Url::parse(base_url)?,
            speaker,
        })
    }

    pub async fn initialize_speaker(&self) -> Result<(), VoicevoxError> {
        let response = self
            .client
            .post(self.initialize_speaker_url())
            .send()
            .await?;
        response_empty("initialize_speaker", response).await
    }

    pub async fn check_ready(&self) -> Result<(), VoicevoxError> {
        let response = self.client.get(self.version_url()).send().await?;
        response_empty("version", response).await
    }

    async fn synthesize(&self, text: &str) -> Result<Vec<u8>, SpeechError> {
        let audio_query_response = self
            .client
            .post(self.audio_query_url(text))
            .send()
            .await
            .map_err(VoicevoxError::from)?;
        let audio_query = response_json("audio_query", audio_query_response).await?;

        let synthesis_response = self
            .client
            .post(self.synthesis_url())
            .json(&audio_query)
            .send()
            .await
            .map_err(VoicevoxError::from)?;
        let wav_bytes = response_bytes("synthesis", synthesis_response).await?;

        Ok(wav_bytes)
    }

    fn audio_query_url(&self, text: &str) -> Url {
        let mut url = self.endpoint("audio_query");
        url.query_pairs_mut()
            .append_pair("text", text)
            .append_pair("speaker", &self.speaker.to_string());
        url
    }

    fn initialize_speaker_url(&self) -> Url {
        let mut url = self.endpoint("initialize_speaker");
        url.query_pairs_mut()
            .append_pair("speaker", &self.speaker.to_string())
            .append_pair("skip_reinit", "true");
        url
    }

    fn synthesis_url(&self) -> Url {
        let mut url = self.endpoint("synthesis");
        url.query_pairs_mut()
            .append_pair("speaker", &self.speaker.to_string());
        url
    }

    fn version_url(&self) -> Url {
        self.endpoint("version")
    }

    fn endpoint(&self, path: &str) -> Url {
        self.base_url
            .join(path)
            .expect("validated VOICEVOX base URL should join endpoint paths")
    }
}

impl SpeechSynthesizer for VoicevoxSpeechSynthesizer {
    fn speak<'a>(
        &'a self,
        text: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), SpeechError>> + Send + 'a>> {
        Box::pin(async move {
            let wav_bytes = self.synthesize(text).await?;
            tokio::task::spawn_blocking(move || play_wav(wav_bytes)).await??;
            Ok(())
        })
    }
}

async fn response_json(
    endpoint: &'static str,
    response: reqwest::Response,
) -> Result<serde_json::Value, VoicevoxError> {
    let response = response_with_success_status(endpoint, response).await?;
    response.json().await.map_err(VoicevoxError::from)
}

async fn response_empty(
    endpoint: &'static str,
    response: reqwest::Response,
) -> Result<(), VoicevoxError> {
    response_with_success_status(endpoint, response).await?;
    Ok(())
}

async fn response_bytes(
    endpoint: &'static str,
    response: reqwest::Response,
) -> Result<Vec<u8>, VoicevoxError> {
    let response = response_with_success_status(endpoint, response).await?;
    let bytes = response.bytes().await.map_err(VoicevoxError::from)?;
    Ok(bytes.to_vec())
}

async fn response_with_success_status(
    endpoint: &'static str,
    response: reqwest::Response,
) -> Result<reqwest::Response, VoicevoxError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }

    let body = response
        .text()
        .await
        .unwrap_or_else(|error| format!("<failed to read response body: {error}>"));

    Err(VoicevoxError::Status {
        endpoint,
        status,
        body,
    })
}

fn play_wav(wav_bytes: Vec<u8>) -> Result<(), SpeechError> {
    let mut stream_handle = rodio::DeviceSinkBuilder::open_default_sink()?;
    stream_handle.log_on_drop(false);
    let player = rodio::play(stream_handle.mixer(), Cursor::new(wav_bytes))?;
    player.sleep_until_end();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_voicevox_endpoints() {
        let synthesizer =
            VoicevoxSpeechSynthesizer::new("http://127.0.0.1:50021", 1).expect("url should parse");

        assert_eq!(
            synthesizer.audio_query_url("テストです。").as_str(),
            "http://127.0.0.1:50021/audio_query?text=%E3%83%86%E3%82%B9%E3%83%88%E3%81%A7%E3%81%99%E3%80%82&speaker=1"
        );
        assert_eq!(
            synthesizer.initialize_speaker_url().as_str(),
            "http://127.0.0.1:50021/initialize_speaker?speaker=1&skip_reinit=true"
        );
        assert_eq!(
            synthesizer.synthesis_url().as_str(),
            "http://127.0.0.1:50021/synthesis?speaker=1"
        );
        assert_eq!(
            synthesizer.version_url().as_str(),
            "http://127.0.0.1:50021/version"
        );
    }
}
