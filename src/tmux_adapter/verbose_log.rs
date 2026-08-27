use std::fmt;
use std::io::{self, Write};
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum VerboseLogLevel {
    #[default]
    Off = 0,
    Debug1 = 1,
    Debug2 = 2,
}

impl VerboseLogLevel {
    pub fn from_flag_count(flag_count: u8) -> Self {
        match flag_count {
            0 => Self::Off,
            1 => Self::Debug1,
            _ => Self::Debug2,
        }
    }

    const fn as_u8(self) -> u8 {
        self as u8
    }

    const fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Debug1,
            2 => Self::Debug2,
            _ => Self::Off,
        }
    }
}

pub fn init(level: VerboseLogLevel) {
    global_level().store(level.as_u8(), Ordering::Relaxed);
}

fn enabled(level: VerboseLogLevel) -> bool {
    if level == VerboseLogLevel::Off {
        return false;
    }

    level <= VerboseLogLevel::from_u8(global_level().load(Ordering::Relaxed))
}

pub fn log(level: VerboseLogLevel, args: fmt::Arguments<'_>) {
    if !enabled(level) {
        return;
    }

    let rendered = args.to_string();
    if let Err(error) = write_stderr_line(&rendered) {
        tracing::debug!(error = %error, line = rendered, "verbose logger dropped a line");
    }
}

fn global_level() -> &'static AtomicU8 {
    static LEVEL: AtomicU8 = AtomicU8::new(VerboseLogLevel::Off as u8);
    &LEVEL
}

fn write_stderr_line(line: &str) -> io::Result<()> {
    let mut stderr = io::stderr().lock();
    stderr.write_all(line.as_bytes())?;
    stderr.flush()
}
