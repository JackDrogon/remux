use std::fs::{File, OpenOptions};
use std::io::{self, IsTerminal};
use std::path::Path;
use std::sync::{Arc, Mutex};

use tracing_subscriber::{
    filter::LevelFilter,
    fmt::{self, writer::MakeWriter},
    prelude::*,
};

use crate::config::{AppState, LogColor, LogLevel};

pub fn run_with<T, F>(config: &AppState, action: &str, requested_backup: Option<&str>, run: F) -> T
where
    F: FnOnce() -> T,
{
    let log_path = config.observability_log_path();

    match open_log_file(&log_path) {
        Ok(file) => {
            let console_layer = fmt::layer()
                .with_ansi(console_ansi(config.config().logging.color))
                .with_target(false)
                .with_writer(std::io::stderr)
                .with_filter(level_filter(config.config().logging.console));
            let file_layer = fmt::layer()
                .with_ansi(false)
                .with_target(true)
                .with_writer(SharedFileWriter::new(file))
                .with_filter(level_filter(config.config().logging.file));
            let subscriber = tracing_subscriber::registry()
                .with(file_layer)
                .with(console_layer);

            tracing::subscriber::with_default(subscriber, || {
                let root_span = root_span(config, action, requested_backup, &log_path, true);
                root_span.in_scope(|| {
                    tracing::info!("observability initialized");
                    run()
                })
            })
        }
        Err(source) => {
            let console_layer = fmt::layer()
                .with_ansi(console_ansi(config.config().logging.color))
                .with_target(false)
                .with_writer(std::io::stderr)
                .with_filter(level_filter(config.config().logging.console));
            let subscriber = tracing_subscriber::registry().with(console_layer);

            tracing::subscriber::with_default(subscriber, || {
                let root_span = root_span(config, action, requested_backup, &log_path, false);
                root_span.in_scope(|| {
                    tracing::warn!(
                        log_path = %log_path.display(),
                        error = %source,
                        "observability log file unavailable; continuing without file logging"
                    );
                    run()
                })
            })
        }
    }
}

fn root_span(
    config: &AppState,
    action: &str,
    requested_backup: Option<&str>,
    log_path: &Path,
    file_logging_enabled: bool,
) -> tracing::Span {
    tracing::info_span!(
        "remux",
        action,
        requested_backup = requested_backup.unwrap_or("-"),
        socket_name = config.socket_name().unwrap_or("default"),
        backup_root = %config.active_backup_path().display(),
        log_path = %log_path.display(),
        file_logging_enabled,
    )
}

fn open_log_file(path: &Path) -> Result<File, io::Error> {
    OpenOptions::new().create(true).append(true).open(path)
}

fn console_ansi(color: LogColor) -> bool {
    match color {
        LogColor::Always => true,
        LogColor::Never => false,
        LogColor::Auto => io::stderr().is_terminal(),
    }
}

#[cfg(test)]
mod tests {
    use super::{LogColor, console_ansi};

    #[test]
    fn console_color_always_and_never_are_explicit() {
        assert!(console_ansi(LogColor::Always));
        assert!(!console_ansi(LogColor::Never));
    }
}

fn level_filter(level: LogLevel) -> LevelFilter {
    match level {
        LogLevel::Off => LevelFilter::OFF,
        LogLevel::Error => LevelFilter::ERROR,
        LogLevel::Info => LevelFilter::INFO,
        LogLevel::Debug => LevelFilter::DEBUG,
    }
}

#[derive(Debug, Clone)]
struct SharedFileWriter {
    file: Arc<Mutex<File>>,
}

impl SharedFileWriter {
    fn new(file: File) -> Self {
        Self {
            file: Arc::new(Mutex::new(file)),
        }
    }
}

impl<'a> MakeWriter<'a> for SharedFileWriter {
    type Writer = SharedFileGuard;

    fn make_writer(&'a self) -> Self::Writer {
        SharedFileGuard {
            file: Arc::clone(&self.file),
        }
    }
}

struct SharedFileGuard {
    file: Arc<Mutex<File>>,
}

impl io::Write for SharedFileGuard {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file
            .lock()
            .expect("observability file mutex should not be poisoned")
            .write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file
            .lock()
            .expect("observability file mutex should not be poisoned")
            .flush()
    }
}
