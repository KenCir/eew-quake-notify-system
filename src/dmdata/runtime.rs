use std::{collections::BTreeMap, time::Duration};

use futures_util::{Sink, SinkExt, StreamExt};
use rand::RngExt;
use thiserror::Error;
use tokio::{sync::mpsc, sync::watch, task::JoinHandle};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        Message,
        client::IntoClientRequest,
        http::{HeaderValue, header::SEC_WEBSOCKET_PROTOCOL},
    },
};

use crate::{
    app::{AdapterOutcome, AppError, AppPipeline},
    config::AppConfig,
    event::EarthquakeEvent,
    notify::Notifier,
    tts::SpeechSynthesizer,
};

use super::{
    client::{DmdataClient, DmdataClientError, SocketStart},
    dto::{WebSocketMessageDto, WebSocketSendMessageDto},
    websocket_data_to_event,
};

const EVENT_CHANNEL_CAPACITY: usize = 64;
const DMDATA_PROTOCOL: &str = "dmdata.v2";

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("failed to start DMDATA socket: {0}")]
    Client(#[from] DmdataClientError),
    #[error("failed to set websocket protocol header: {0}")]
    ProtocolHeader(#[from] tokio_tungstenite::tungstenite::http::header::InvalidHeaderValue),
    #[error("websocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("failed to parse websocket JSON message: {0}")]
    ParseMessage(#[from] serde_json::Error),
    #[error("receiver channel is closed")]
    ChannelClosed,
    #[error("event processor task failed: {0}")]
    EventProcessorJoin(#[from] tokio::task::JoinError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SocketSessionEnd {
    Ended,
    Shutdown,
}

pub async fn run_forever<N, S>(
    config: AppConfig,
    pipeline: AppPipeline<N, S>,
) -> Result<(), RuntimeError>
where
    N: Notifier + Send + 'static,
    S: SpeechSynthesizer + Send + 'static,
{
    let (event_sender, event_receiver) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
    let event_processor = tokio::spawn(process_events(pipeline, event_receiver));
    let (shutdown_sender, shutdown_receiver) = watch::channel(false);
    let shutdown_listener = spawn_shutdown_listener(shutdown_sender.clone());

    let client = DmdataClient::new();
    let mut reconnect_delay = Duration::from_millis(config.dmdata.reconnect_initial_ms);
    let reconnect_max = Duration::from_millis(config.dmdata.reconnect_max_ms);
    let mut reconnect_attempt = 0usize;
    let mut shutdown_receiver = shutdown_receiver;

    loop {
        if *shutdown_receiver.borrow() {
            break;
        }

        tracing::info!("starting Project DM-D.S.S socket session");

        let result = run_socket_session(
            &client,
            &config,
            event_sender.clone(),
            shutdown_receiver.clone(),
        )
        .await;

        match result {
            Ok(SocketSessionEnd::Ended) => {
                reconnect_delay = Duration::from_millis(config.dmdata.reconnect_initial_ms);
                reconnect_attempt += 1;
                let delay = jittered_delay(reconnect_delay);
                tracing::warn!(
                    reconnect_attempt,
                    backoff_ms = delay.as_millis(),
                    last_error_category = "session_ended",
                    "socket session ended; reconnecting after backoff"
                );
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    _ = wait_for_shutdown(&mut shutdown_receiver) => break,
                }
                reconnect_delay = (reconnect_delay * 2).min(reconnect_max);
                continue;
            }
            Ok(SocketSessionEnd::Shutdown) => break,
            Err(error) => {
                if matches!(error, RuntimeError::ChannelClosed) {
                    shutdown_listener.abort();
                    drop(event_sender);
                    event_processor.await?;
                    return Err(error);
                }
                reconnect_attempt += 1;
                let delay = jittered_delay(reconnect_delay);
                tracing::warn!(
                    reconnect_attempt,
                    backoff_ms = delay.as_millis(),
                    last_error_category = runtime_error_category(&error),
                    %error,
                    "socket session failed; reconnecting after backoff"
                );
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    _ = wait_for_shutdown(&mut shutdown_receiver) => break,
                }
                reconnect_delay = (reconnect_delay * 2).min(reconnect_max);
                continue;
            }
        }
    }

    tracing::info!("runtime shutdown requested; waiting for event processor");
    shutdown_listener.abort();
    drop(event_sender);
    event_processor.await?;
    tracing::info!("runtime shutdown complete");

    Ok(())
}

async fn run_socket_session(
    client: &DmdataClient,
    config: &AppConfig,
    event_sender: mpsc::Sender<EarthquakeEvent>,
    shutdown: watch::Receiver<bool>,
) -> Result<SocketSessionEnd, RuntimeError> {
    let socket = client.start_socket(&config.dmdata).await?;
    let protocol = protocol_header(&socket)?;
    let mut request = socket.url.into_client_request()?;
    request
        .headers_mut()
        .insert(SEC_WEBSOCKET_PROTOCOL, protocol);

    let (stream, response) = connect_async(request).await?;
    tracing::info!(
        status = %response.status(),
        "Project DM-D.S.S websocket connected"
    );

    let (mut writer, mut reader) = stream.split();
    let mut shutdown = shutdown;

    loop {
        let message = tokio::select! {
            _ = wait_for_shutdown(&mut shutdown) => {
                tracing::info!(
                    socket_id = ?socket.id,
                    "shutdown requested; closing Project DM-D.S.S websocket session"
                );
                close_socket_session(client, config, socket.id.as_deref(), &mut writer).await;
                return Ok(SocketSessionEnd::Shutdown);
            }
            message = reader.next() => message,
        };

        let Some(message) = message else {
            break;
        };
        let message = message?;

        match message {
            Message::Text(text) => {
                let message: WebSocketMessageDto = serde_json::from_str(&text)?;
                match message {
                    WebSocketMessageDto::Start(start) => {
                        tracing::info!(
                            socket_id = %start.socket_id,
                            classifications = ?start.classifications,
                            "websocket start message received"
                        );
                    }
                    WebSocketMessageDto::Ping { ping_id } => {
                        tracing::debug!(?ping_id, "websocket ping message received");
                        let pong = serde_json::to_string(&WebSocketSendMessageDto::Pong {
                            ping_id: ping_id.clone(),
                        })?;
                        writer.send(Message::Text(pong.into())).await?;
                        tracing::debug!(?ping_id, "websocket pong message sent");
                    }
                    WebSocketMessageDto::Pong { .. } => {}
                    WebSocketMessageDto::Data(data) => {
                        tracing::debug!(
                            data_id = %data.id,
                            classification = %data.classification,
                            data_type = %data.head.data_type,
                            "websocket data message received"
                        );

                        match websocket_data_to_event(&data) {
                            Ok(Some(event)) => event_sender
                                .send(event)
                                .await
                                .map_err(|_| RuntimeError::ChannelClosed)?,
                            Ok(None) => {
                                tracing::debug!(
                                    data_id = %data.id,
                                    classification = %data.classification,
                                    "websocket data message skipped"
                                );
                            }
                            Err(error) => {
                                tracing::warn!(
                                    data_id = %data.id,
                                    classification = %data.classification,
                                    %error,
                                    "failed to normalize websocket data message"
                                );
                            }
                        }
                    }
                    WebSocketMessageDto::Error(error) => {
                        tracing::warn!(
                            code = error.code,
                            close = error.close,
                            message = %error.error,
                            "websocket error message received"
                        );
                        if error.close {
                            break;
                        }
                    }
                }
            }
            Message::Close(frame) => {
                tracing::info!(?frame, "websocket close frame received");
                break;
            }
            Message::Ping(payload) => writer.send(Message::Pong(payload)).await?,
            Message::Pong(_) => {}
            Message::Binary(_) => {
                tracing::debug!("binary websocket message skipped");
            }
            Message::Frame(_) => {}
        }
    }

    Ok(SocketSessionEnd::Ended)
}

async fn process_events<N, S>(
    mut pipeline: AppPipeline<N, S>,
    mut receiver: mpsc::Receiver<EarthquakeEvent>,
) where
    N: Notifier,
    S: SpeechSynthesizer,
{
    let mut received_count = 0usize;
    let mut processed_count = 0usize;
    let mut notification_failures = 0usize;
    let mut speech_failures = 0usize;
    let mut notification_skips = BTreeMap::new();
    let mut speech_skips = BTreeMap::new();

    while let Some(event) = receiver.recv().await {
        received_count += 1;
        match pipeline.process_event(&event).await {
            Ok(outcome) => {
                processed_count += 1;
                tracing::info!(
                    source_id = ?event.source_id,
                    kind = ?event.kind,
                    ?outcome,
                    "event processed"
                );
                count_adapter_outcome(
                    &mut notification_skips,
                    &mut notification_failures,
                    &outcome.notification,
                );
                count_adapter_outcome(&mut speech_skips, &mut speech_failures, &outcome.speech);
            }
            Err(AppError::Notify(error)) => {
                notification_failures += 1;
                tracing::warn!(source_id = ?event.source_id, %error, "desktop notification failed");
            }
            Err(AppError::Speech(error)) => {
                speech_failures += 1;
                tracing::warn!(source_id = ?event.source_id, %error, "speech synthesis failed");
            }
        }
    }

    pipeline.flush_dedup_state();
    tracing::info!(
        received_count,
        processed_count,
        notification_skips = ?notification_skips,
        speech_skips = ?speech_skips,
        notification_failures,
        speech_failures,
        "event processor stopped"
    );
}

fn spawn_shutdown_listener(shutdown_sender: watch::Sender<bool>) -> JoinHandle<()> {
    tokio::spawn(async move {
        match tokio::signal::ctrl_c().await {
            Ok(()) => {
                tracing::info!("shutdown signal received");
                let _ = shutdown_sender.send(true);
            }
            Err(error) => {
                tracing::warn!(%error, "failed to listen for shutdown signal");
                let _ = shutdown_sender.send(true);
            }
        }
    })
}

async fn wait_for_shutdown(shutdown: &mut watch::Receiver<bool>) {
    if *shutdown.borrow() {
        return;
    }

    while shutdown.changed().await.is_ok() {
        if *shutdown.borrow() {
            return;
        }
    }
}

async fn close_socket_session<W>(
    client: &DmdataClient,
    config: &AppConfig,
    socket_id: Option<&str>,
    writer: &mut W,
) where
    W: Sink<Message> + Unpin,
    W::Error: std::fmt::Display,
{
    if let Some(socket_id) = socket_id {
        match client.close_socket(&config.dmdata, socket_id).await {
            Ok(()) => tracing::info!(socket_id, "Project DM-D.S.S socket.close completed"),
            Err(error) => tracing::warn!(socket_id, %error, "Project DM-D.S.S socket.close failed"),
        }
    } else {
        tracing::debug!("socket.close skipped because socket id is unavailable");
    }

    if let Err(error) = writer.close().await {
        tracing::debug!(%error, "websocket close frame could not be sent");
    }
}

fn protocol_header(socket: &SocketStart) -> Result<HeaderValue, RuntimeError> {
    let protocol = if socket
        .protocols
        .iter()
        .any(|protocol| protocol == DMDATA_PROTOCOL)
    {
        DMDATA_PROTOCOL.to_owned()
    } else {
        socket.protocols.join(", ")
    };

    Ok(HeaderValue::from_str(&protocol)?)
}

fn jittered_delay(base: Duration) -> Duration {
    let jitter_ms = rand::rng().random_range(0..=base.as_millis().min(1_000) as u64);
    base + Duration::from_millis(jitter_ms)
}

fn count_adapter_outcome(
    skips: &mut BTreeMap<&'static str, usize>,
    failures: &mut usize,
    outcome: &AdapterOutcome,
) {
    match outcome {
        AdapterOutcome::Skipped(reason) => {
            *skips.entry(reason.as_str()).or_default() += 1;
        }
        AdapterOutcome::Failed(_) => *failures += 1,
        AdapterOutcome::Sent => {}
    }
}

fn runtime_error_category(error: &RuntimeError) -> &'static str {
    match error {
        RuntimeError::Client(error) => dmdata_client_error_category(error),
        RuntimeError::ProtocolHeader(_) => "protocol_header",
        RuntimeError::WebSocket(_) => "websocket",
        RuntimeError::ParseMessage(_) => "parse_message",
        RuntimeError::ChannelClosed => "channel_closed",
        RuntimeError::EventProcessorJoin(_) => "event_processor_join",
    }
}

fn dmdata_client_error_category(error: &DmdataClientError) -> &'static str {
    match error {
        DmdataClientError::Request(_) => "dmdata_request",
        DmdataClientError::MissingEnvironmentVariable { .. } => "missing_environment_variable",
        DmdataClientError::UnsupportedAuthMode { .. } => "unsupported_auth_mode",
        DmdataClientError::OAuthHttpStatus { .. } => "oauth_http_status",
        DmdataClientError::UnsupportedTokenType { .. } => "unsupported_token_type",
        DmdataClientError::EmptyAccessToken => "empty_access_token",
        DmdataClientError::HttpStatus { .. } => "socket_start_http_status",
        DmdataClientError::SocketCloseHttpStatus { .. } => "socket_close_http_status",
        DmdataClientError::Api { .. } => "socket_start_api_error",
        DmdataClientError::MissingWebSocket => "missing_websocket",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_dmdata_protocol_header() {
        let socket = SocketStart {
            id: Some("socket-1".to_owned()),
            url: "wss://example.invalid/socket".to_owned(),
            protocols: vec!["dmdata.v2".to_owned()],
        };

        assert_eq!(
            protocol_header(&socket).unwrap(),
            HeaderValue::from_static("dmdata.v2")
        );
    }

    #[test]
    fn jitter_keeps_delay_at_or_above_base() {
        let base = Duration::from_millis(100);
        assert!(jittered_delay(base) >= base);
    }
}
