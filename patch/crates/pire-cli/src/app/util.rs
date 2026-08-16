use std::{
    io::{self, IsTerminal, Read},
    path::Path,
};

use tracing_subscriber::EnvFilter;

use crate::{config::AppConfig, error::PireError};

pub(super) fn read_piped_stdin(max_bytes: usize) -> io::Result<Option<String>> {
    let stdin = io::stdin();
    if stdin.is_terminal() {
        return Ok(None);
    }
    let limit = u64::try_from(max_bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "stdin limit is too large"))?
        .saturating_add(1);
    let mut bytes = Vec::new();
    stdin.lock().take(limit).read_to_end(&mut bytes)?;
    if bytes.len() > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("stdin exceeds {max_bytes} bytes"),
        ));
    }
    if bytes.is_empty() {
        return Ok(None);
    }
    String::from_utf8(bytes).map(Some).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("stdin is not valid UTF-8: {error}"),
        )
    })
}

pub(super) fn init_tracing(config: &AppConfig) -> Result<(), PireError> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(config.logging.level.as_str()));
    if config.logging.json {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .with_writer(io::stderr)
            .json()
            .try_init()
            .map_err(|error| PireError::Message(format!("tracing initialization failed: {error}")))
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .with_writer(io::stderr)
            .try_init()
            .map_err(|error| PireError::Message(format!("tracing initialization failed: {error}")))
    }
}

#[must_use]
pub(super) fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

pub(super) fn osc52_copy(value: &str) -> io::Result<()> {
    use std::io::Write;
    let encoded = base64(value.as_bytes());
    let mut stdout = io::stdout();
    write!(stdout, "\x1b]52;c;{encoded}\x07")?;
    stdout.flush()
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        output.push(TABLE[usize::from(a >> 2)] as char);
        output.push(TABLE[usize::from(((a & 0b11) << 4) | (b >> 4))] as char);
        if chunk.len() > 1 {
            output.push(TABLE[usize::from(((b & 0b1111) << 2) | (c >> 6))] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(TABLE[usize::from(c & 0b11_1111)] as char);
        } else {
            output.push('=');
        }
    }
    output
}

#[must_use]
pub(super) fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
