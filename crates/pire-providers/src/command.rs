use std::{
    io::{self, Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use pire_core::{CompletionRequest, Provider, ProviderError, ProviderResponse};
use wait_timeout::ChildExt;

use crate::ProviderBuildError;

pub struct CommandProvider {
    arguments: Vec<String>,
    timeout: Duration,
    max_output_bytes: usize,
    current_dir: PathBuf,
}

impl CommandProvider {
    pub fn new(
        command: String,
        timeout: Duration,
        max_output_bytes: usize,
        current_dir: PathBuf,
    ) -> Result<Self, ProviderBuildError> {
        let arguments = shlex::split(&command)
            .ok_or_else(|| ProviderBuildError::Invalid("invalid command quoting".to_owned()))?;
        if arguments.is_empty() {
            return Err(ProviderBuildError::Invalid(
                "command provider requires an executable".to_owned(),
            ));
        }
        if max_output_bytes == 0 {
            return Err(ProviderBuildError::Invalid(
                "max_output_bytes must be greater than zero".to_owned(),
            ));
        }
        Ok(Self {
            arguments,
            timeout,
            max_output_bytes,
            current_dir,
        })
    }
}

impl Provider for CommandProvider {
    fn name(&self) -> &str {
        "command"
    }

    fn complete(&self, request: &CompletionRequest) -> Result<ProviderResponse, ProviderError> {
        let executable = &self.arguments[0];
        let mut child = Command::new(executable)
            .args(&self.arguments[1..])
            .current_dir(&self.current_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| ProviderError::new(format!("unable to start {executable}: {error}")))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ProviderError::new("command provider stdout was unavailable"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| ProviderError::new("command provider stderr was unavailable"))?;
        let stdout_reader = read_stream(stdout, self.max_output_bytes);
        let stderr_reader = read_stream(stderr, self.max_output_bytes);

        {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| ProviderError::new("command provider stdin was unavailable"))?;
            serde_json::to_writer(&mut stdin, request)
                .map_err(|error| ProviderError::new(format!("unable to encode request: {error}")))?;
            stdin
                .write_all(b"\n")
                .map_err(|error| ProviderError::new(format!("unable to write request: {error}")))?;
        }

        let status = child
            .wait_timeout(self.timeout)
            .map_err(|error| ProviderError::new(format!("unable to wait for provider: {error}")))?;
        let status = match status {
            Some(status) => status,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ProviderError::new(format!(
                    "command provider exceeded a {}-second timeout",
                    self.timeout.as_secs()
                )));
            }
        };

        let stdout = join_reader(stdout_reader, "stdout")?;
        let stderr = join_reader(stderr_reader, "stderr")?;
        if !status.success() {
            return Err(ProviderError::new(format!(
                "command provider exited with {status}: {}",
                String::from_utf8_lossy(&stderr)
            )));
        }
        serde_json::from_slice(&stdout)
            .map_err(|error| ProviderError::new(format!("invalid command response: {error}")))
    }
}

fn read_stream<R>(reader: R, max_bytes: usize) -> thread::JoinHandle<io::Result<Vec<u8>>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut bytes = Vec::new();
        reader
            .take(max_bytes.saturating_add(1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > max_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("provider output exceeded {max_bytes} bytes"),
            ));
        }
        Ok(bytes)
    })
}

fn join_reader(
    handle: thread::JoinHandle<io::Result<Vec<u8>>>,
    name: &str,
) -> Result<Vec<u8>, ProviderError> {
    handle
        .join()
        .map_err(|_| ProviderError::new(format!("provider {name} reader panicked")))?
        .map_err(|error| ProviderError::new(format!("unable to read provider {name}: {error}")))
}
