use std::path::Path;

use anyhow::{Context, bail};
use eew_quake_notify_system::{
    app::AppPipeline,
    config::AppConfig,
    dmdata::{replay::load_replay_data, runtime::run_forever, websocket_data_to_event},
    event::{EarthquakeEvent, EventKind, Hypocenter, Intensity, Magnitude},
    instance::InstanceLock,
    message,
    notify::{DesktopNotifier, notify_if_enabled},
    tts::{VoicevoxSpeechSynthesizer, speak_if_enabled},
};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    Run,
    TestNotify,
    TestSpeech,
    TestAlert,
    ReplayFixture(String),
    ValidateConfig,
    Doctor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Cli {
    config_path: String,
    command: Command,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse(std::env::args().skip(1))?;
    let config = AppConfig::load_from_path(&cli.config_path)
        .with_context(|| format!("failed to load config from {}", cli.config_path))?;
    eew_quake_notify_system::logging::init(&config.log)?;

    tracing::info!(
        socket_start_url = %config.dmdata.socket_start_url,
        auth_mode = %config.dmdata.auth_mode,
        classifications = ?config.dmdata.classifications,
        types = ?config.dmdata.types,
        desktop_notifications = config.notify.desktop_enabled,
        notify_enabled_kinds = ?config.notify.enabled_kinds,
        tts_enabled = config.tts.enabled,
        tts_enabled_kinds = ?config.tts.enabled_kinds,
        state_enabled = config.state.enabled,
        state_path = %config.state.file_path,
        runtime_single_instance = config.runtime.single_instance,
        runtime_lock_file_path = %config.runtime.lock_file_path,
        log_console_enabled = config.log.console_enabled,
        log_file_enabled = config.log.file_enabled,
        log_file_path = %config.log.file_path,
        "configuration loaded"
    );

    match cli.command {
        Command::Run => {
            let _instance_lock = InstanceLock::acquire(&config.runtime)
                .context("failed to acquire single instance lock")?;
            let notifier = DesktopNotifier::new("EEW Quake Notify System");
            let synthesizer = build_voicevox_speech_synthesizer(&config).await?;
            let pipeline =
                AppPipeline::new_with_persistent_state(config.clone(), notifier, synthesizer);
            run_forever(config, pipeline)
                .await
                .context("Project DM-D.S.S runtime stopped unexpectedly")?;
        }
        Command::TestNotify => {
            let event = sample_test_event();
            let notification = message::notification_message(&event);
            let notifier = DesktopNotifier::new("EEW Quake Notify System");
            notify_if_enabled(&notifier, config.notify.desktop_enabled, &notification)
                .context("failed to send test desktop notification")?;
            tracing::info!("test desktop notification sent");
        }
        Command::TestSpeech => {
            let event = sample_test_event();
            let text = message::speech_text(&event);
            let synthesizer = build_voicevox_speech_synthesizer(&config).await?;
            speak_if_enabled(&synthesizer, config.tts.enabled, &text)
                .await
                .context("failed to play test speech with VOICEVOX")?;
            tracing::info!("test speech played");
        }
        Command::TestAlert => {
            let notifier = DesktopNotifier::new("EEW Quake Notify System");
            let synthesizer = build_voicevox_speech_synthesizer(&config).await?;
            let mut pipeline = AppPipeline::new(config, notifier, synthesizer);
            let outcome = pipeline
                .process_event(&sample_test_event())
                .await
                .context("failed to process test alert")?;
            tracing::info!(?outcome, "test alert processed");
        }
        Command::ReplayFixture(path) => {
            replay_fixture(config, &path)
                .await
                .with_context(|| format!("failed to replay fixture from {path}"))?;
        }
        Command::ValidateConfig => {
            tracing::info!(config_path = %cli.config_path, "configuration validation succeeded");
        }
        Command::Doctor => {
            run_doctor(&config).await.context("doctor check failed")?;
        }
    }

    Ok(())
}

impl Cli {
    fn parse(args: impl IntoIterator<Item = String>) -> anyhow::Result<Self> {
        let mut config_path = "config.toml".to_owned();
        let mut command = Command::Run;
        let mut args = args.into_iter();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--test-notify" => command = Command::TestNotify,
                "--test-speech" => command = Command::TestSpeech,
                "--test-alert" => command = Command::TestAlert,
                "--validate-config" => command = Command::ValidateConfig,
                "--doctor" => command = Command::Doctor,
                "--replay-fixture" => {
                    command = Command::ReplayFixture(
                        args.next()
                            .context("--replay-fixture requires a file or directory path")?,
                    );
                }
                "--config" => {
                    config_path = args.next().context("--config requires a path argument")?;
                }
                "--help" | "-h" => bail!(usage()),
                value if value.starts_with('-') => bail!("unknown option: {value}\n{}", usage()),
                value => config_path = value.to_owned(),
            }
        }

        Ok(Self {
            config_path,
            command,
        })
    }
}

fn usage() -> &'static str {
    "usage: eew-quake-notify-system [--config <path>] [--test-notify] [--test-speech] [--test-alert] [--replay-fixture <path>] [--validate-config] [--doctor]"
}

async fn build_voicevox_speech_synthesizer(
    config: &AppConfig,
) -> anyhow::Result<VoicevoxSpeechSynthesizer> {
    let synthesizer = VoicevoxSpeechSynthesizer::new(&config.tts.voicevox_url, config.tts.speaker)
        .context("failed to initialize VOICEVOX speech synthesizer")?;

    if config.tts.enabled {
        tracing::info!(
            speaker = config.tts.speaker,
            "initializing VOICEVOX speaker"
        );
        synthesizer
            .initialize_speaker()
            .await
            .context("failed to initialize VOICEVOX speaker")?;
        tracing::info!(speaker = config.tts.speaker, "VOICEVOX speaker initialized");
    }

    Ok(synthesizer)
}

fn sample_test_event() -> EarthquakeEvent {
    EarthquakeEvent {
        source_id: Some("manual-test-notification".to_owned()),
        kind: EventKind::Earthquake,
        serial: Some(1),
        is_final: false,
        is_cancelled: false,
        occurred_at: Some("テスト".to_owned()),
        announced_at: Some("テスト通知".to_owned()),
        hypocenter: Some(Hypocenter {
            name: "テスト震源".to_owned(),
            depth_km: Some(10),
        }),
        max_intensity: Some(Intensity::Four),
        magnitude: Some(Magnitude(5.0)),
        affected_areas: vec!["テスト地域".to_owned()],
    }
}

async fn replay_fixture(config: AppConfig, path: &str) -> anyhow::Result<()> {
    let data_messages = load_replay_data(path)?;
    let notifier = DesktopNotifier::new("EEW Quake Notify System");
    let synthesizer = build_voicevox_speech_synthesizer(&config).await?;
    let mut pipeline = AppPipeline::new(config, notifier, synthesizer);
    let mut processed_count = 0usize;
    let mut skipped_count = 0usize;

    for data in data_messages {
        tracing::info!(
            data_id = %data.id,
            classification = %data.classification,
            data_type = %data.head.data_type,
            format = ?data.format,
            "replaying websocket data fixture"
        );

        let Some(event) = websocket_data_to_event(&data).with_context(|| {
            format!(
                "failed to normalize replay fixture data {} ({})",
                data.id, data.head.data_type
            )
        })?
        else {
            skipped_count += 1;
            tracing::info!(
                data_id = %data.id,
                classification = %data.classification,
                data_type = %data.head.data_type,
                "replay fixture skipped because it is not a supported JSON telegram"
            );
            continue;
        };

        let outcome = pipeline.process_event(&event).await.with_context(|| {
            format!(
                "failed to process replay fixture event {:?}",
                event.source_id
            )
        })?;
        processed_count += 1;
        tracing::info!(
            source_id = ?event.source_id,
            kind = ?event.kind,
            ?outcome,
            "replay fixture event processed"
        );
    }

    if processed_count == 0 {
        bail!("replay fixture did not produce any supported event");
    }

    tracing::info!(processed_count, skipped_count, "replay fixture completed");

    Ok(())
}

async fn run_doctor(config: &AppConfig) -> anyhow::Result<()> {
    tracing::info!("doctor: configuration loaded and validated");

    ensure_parent_directory("state.file_path", &config.state.file_path)?;
    if config.log.file_enabled {
        ensure_parent_directory("log.file_path", &config.log.file_path)?;
    }

    let _lock = InstanceLock::acquire(&config.runtime).context("doctor: single instance lock")?;
    tracing::info!("doctor: single instance lock check succeeded");

    if config.tts.enabled {
        let synthesizer =
            VoicevoxSpeechSynthesizer::new(&config.tts.voicevox_url, config.tts.speaker)
                .context("doctor: failed to build VOICEVOX client")?;
        synthesizer
            .check_ready()
            .await
            .context("doctor: VOICEVOX API is not reachable")?;
        tracing::info!("doctor: VOICEVOX API check succeeded");
    } else {
        tracing::info!("doctor: VOICEVOX API check skipped because tts.enabled is false");
    }

    tracing::info!("doctor: all checks succeeded");

    Ok(())
}

fn ensure_parent_directory(label: &str, path: &str) -> anyhow::Result<()> {
    let path = Path::new(path.trim());
    let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    else {
        tracing::info!(%label, path = %path.display(), "doctor: parent directory check skipped");
        return Ok(());
    };

    std::fs::create_dir_all(parent).with_context(|| {
        format!(
            "doctor: failed to create parent directory for {label}: {}",
            parent.display()
        )
    })?;
    tracing::info!(
        %label,
        path = %path.display(),
        directory = %parent.display(),
        "doctor: parent directory is available"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_run_command() {
        let cli = Cli::parse(Vec::new()).expect("cli should parse");

        assert_eq!(
            cli,
            Cli {
                config_path: "config.toml".to_owned(),
                command: Command::Run,
            }
        );
    }

    #[test]
    fn parses_config_and_replay_fixture_command() {
        let cli = Cli::parse([
            "--config".to_owned(),
            "config.toml.example".to_owned(),
            "--replay-fixture".to_owned(),
            "test-data/eew-warning_20260626".to_owned(),
        ])
        .expect("cli should parse");

        assert_eq!(
            cli,
            Cli {
                config_path: "config.toml.example".to_owned(),
                command: Command::ReplayFixture("test-data/eew-warning_20260626".to_owned()),
            }
        );
    }

    #[test]
    fn parses_config_and_test_notify_command() {
        let cli = Cli::parse([
            "--config".to_owned(),
            "config.toml.example".to_owned(),
            "--test-notify".to_owned(),
        ])
        .expect("cli should parse");

        assert_eq!(
            cli,
            Cli {
                config_path: "config.toml.example".to_owned(),
                command: Command::TestNotify,
            }
        );
    }

    #[test]
    fn parses_config_and_test_speech_command() {
        let cli = Cli::parse([
            "--config".to_owned(),
            "config.toml.example".to_owned(),
            "--test-speech".to_owned(),
        ])
        .expect("cli should parse");

        assert_eq!(
            cli,
            Cli {
                config_path: "config.toml.example".to_owned(),
                command: Command::TestSpeech,
            }
        );
    }

    #[test]
    fn parses_config_and_test_alert_command() {
        let cli = Cli::parse([
            "--config".to_owned(),
            "config.toml.example".to_owned(),
            "--test-alert".to_owned(),
        ])
        .expect("cli should parse");

        assert_eq!(
            cli,
            Cli {
                config_path: "config.toml.example".to_owned(),
                command: Command::TestAlert,
            }
        );
    }

    #[test]
    fn parses_config_and_validate_config_command() {
        let cli = Cli::parse([
            "--config".to_owned(),
            "config.toml.example".to_owned(),
            "--validate-config".to_owned(),
        ])
        .expect("cli should parse");

        assert_eq!(
            cli,
            Cli {
                config_path: "config.toml.example".to_owned(),
                command: Command::ValidateConfig,
            }
        );
    }

    #[test]
    fn parses_config_and_doctor_command() {
        let cli = Cli::parse([
            "--config".to_owned(),
            "config.toml.example".to_owned(),
            "--doctor".to_owned(),
        ])
        .expect("cli should parse");

        assert_eq!(
            cli,
            Cli {
                config_path: "config.toml.example".to_owned(),
                command: Command::Doctor,
            }
        );
    }
}
