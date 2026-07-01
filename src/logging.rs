use std::{
    fs::{self, File, OpenOptions},
    path::PathBuf,
};

use thiserror::Error;
use tracing_subscriber::{
    EnvFilter,
    fmt::{self, MakeWriter, time::LocalTime},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

use crate::config::LogConfig;

#[derive(Debug, Error)]
pub enum LoggingError {
    #[error("invalid log filter: {0}")]
    Filter(#[from] tracing_subscriber::filter::ParseError),
    #[error("failed to create log directory {path}: {source}")]
    CreateDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to initialize global logger: {0}")]
    Init(#[from] tracing_subscriber::util::TryInitError),
}

pub fn init(config: &LogConfig) -> Result<(), LoggingError> {
    let filter = EnvFilter::try_new(&config.level)?;
    let file_writer = if config.file_enabled {
        let path = PathBuf::from(config.file_path.trim());
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|source| LoggingError::CreateDirectory {
                path: parent.to_owned(),
                source,
            })?;
        }
        Some(LogFileWriter { path })
    } else {
        None
    };

    let console_layer = config.console_enabled.then(|| {
        fmt::layer()
            .with_timer(LocalTime::rfc_3339())
            .with_target(false)
    });

    let file_layer = file_writer.map(|writer| {
        fmt::layer()
            .with_timer(LocalTime::rfc_3339())
            .with_target(false)
            .with_ansi(false)
            .with_writer(writer)
    });

    tracing_subscriber::registry()
        .with(filter)
        .with(console_layer)
        .with(file_layer)
        .try_init()?;

    Ok(())
}

#[derive(Debug, Clone)]
struct LogFileWriter {
    path: PathBuf,
}

impl<'writer> MakeWriter<'writer> for LogFileWriter {
    type Writer = File;

    fn make_writer(&'writer self) -> Self::Writer {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .expect("validated log file path should be writable")
    }
}
