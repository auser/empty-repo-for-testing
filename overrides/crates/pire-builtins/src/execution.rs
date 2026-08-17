use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use pire_core::{
    CancellationToken, ExecutionBackend, ExecutionError, ExecutionRequest, ExecutionResult, Plugin,
    PluginMetadata, Registry,
};
use wait_timeout::ChildExt;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Default)]
pub struct HostExecutionBackend;

impl ExecutionBackend for HostExecutionBackend {
    fn id(&self) -> &str {
        "host"
    }

    fn execute(
        &self,
        request: &ExecutionRequest,
        cancellation: &CancellationToken,
    ) -> Result<ExecutionResult, ExecutionError> {
        run_bounded_command(
            &request.program,
            &request.args,
            &request.cwd,
            &request.env,
            request.clear_env,
            None,
            request.timeout,
            request.max_output_bytes,
            cancellation,
        )
    }
}

pub struct HostExecutionPlugin;

impl Plugin for HostExecutionPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(
            "pire.execution.host",
            env!("CARGO_PKG_VERSION"),
            "bounded host process execution",
        )
    }

    fn mount(&mut self, registry: &mut Registry) -> Result<(), String> {
        registry
            .register_execution(Arc::new(HostExecutionBackend))
            .map_err(|error| error.to_string())
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run_bounded_command(
    program: &str,
    args: &[String],
    cwd: &Path,
    env: &BTreeMap<String, String>,
    clear_env: bool,
    stdin: Option<&[u8]>,
    timeout: Duration,
    max_output_bytes: usize,
    cancellation: &CancellationToken,
) -> Result<ExecutionResult, ExecutionError> {
    if cancellation.is_cancelled() {
        return Err(ExecutionError::Cancelled);
    }
    if timeout.is_zero() || max_output_bytes == 0 {
        return Err(ExecutionError::Spawn(
            "execution timeout and output limit must be greater than zero".to_owned(),
        ));
    }

    let (stdout_path, stdout_file) = temporary_file("stdout")?;
    let (stderr_path, stderr_file) = temporary_file("stderr")?;

    let result = run_child(
        program,
        args,
        cwd,
        env,
        clear_env,
        stdin,
        timeout,
        max_output_bytes,
        cancellation,
        &stdout_path,
        &stderr_path,
        stdout_file,
        stderr_file,
    );

    let _ = std::fs::remove_file(&stdout_path);
    let _ = std::fs::remove_file(&stderr_path);
    result
}

#[allow(clippy::too_many_arguments)]
fn run_child(
    program: &str,
    args: &[String],
    cwd: &Path,
    env: &BTreeMap<String, String>,
    clear_env: bool,
    stdin: Option<&[u8]>,
    timeout: Duration,
    max_output_bytes: usize,
    cancellation: &CancellationToken,
    stdout_path: &Path,
    stderr_path: &Path,
    stdout_file: File,
    stderr_file: File,
) -> Result<ExecutionResult, ExecutionError> {
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });

    if clear_env {
        command.env_clear();
        preserve_runtime_environment(&mut command);
    }
    command.envs(env);

    let mut child = command
        .spawn()
        .map_err(|error| ExecutionError::Spawn(error.to_string()))?;

    if let Some(input) = stdin {
        let mut child_stdin = child
            .stdin
            .take()
            .ok_or_else(|| ExecutionError::Spawn("child stdin was unavailable".to_owned()))?;
        child_stdin
            .write_all(input)
            .map_err(|error| ExecutionError::Spawn(error.to_string()))?;
    }

    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if cancellation.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ExecutionError::Cancelled);
        }

        let elapsed = started.elapsed();
        if elapsed >= timeout {
            timed_out = true;
            let _ = child.kill();
            break child
                .wait()
                .map_err(|error| ExecutionError::Wait(error.to_string()))?;
        }

        let remaining = timeout.saturating_sub(elapsed);
        let poll = remaining.min(Duration::from_millis(100));
        match child
            .wait_timeout(poll)
            .map_err(|error| ExecutionError::Wait(error.to_string()))?
        {
            Some(status) => break status,
            None => {}
        }
    };

    let stdout_bytes = read_limited(stdout_path, max_output_bytes)?;
    let remaining = max_output_bytes.saturating_sub(stdout_bytes.len());
    let stderr_bytes = read_limited(stderr_path, remaining)?;

    Ok(ExecutionResult {
        exit_code: status.code(),
        stdout: String::from_utf8(stdout_bytes)
            .map_err(|error| ExecutionError::Output(error.to_string()))?,
        stderr: String::from_utf8(stderr_bytes)
            .map_err(|error| ExecutionError::Output(error.to_string()))?,
        timed_out,
    })
}

fn read_limited(path: &Path, limit: usize) -> Result<Vec<u8>, ExecutionError> {
    let file = OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(|error| ExecutionError::Output(error.to_string()))?;
    let take_limit = u64::try_from(limit)
        .map_err(|_| ExecutionError::OutputLimit)?
        .saturating_add(1);
    let mut bytes = Vec::new();
    file.take(take_limit)
        .read_to_end(&mut bytes)
        .map_err(|error| ExecutionError::Output(error.to_string()))?;
    if bytes.len() > limit {
        return Err(ExecutionError::OutputLimit);
    }
    Ok(bytes)
}

fn temporary_file(kind: &str) -> Result<(PathBuf, File), ExecutionError> {
    let directory = std::env::temp_dir();
    for _ in 0..32 {
        let path = directory.join(format!(
            "pire-{kind}-{}-{}-{}",
            std::process::id(),
            now_nanos(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        match OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(ExecutionError::Spawn(error.to_string())),
        }
    }
    Err(ExecutionError::Spawn(
        "unable to allocate a unique temporary output file".to_owned(),
    ))
}

fn preserve_runtime_environment(command: &mut Command) {
    for key in [
        "PATH",
        "HOME",
        "USERPROFILE",
        "SYSTEMROOT",
        "WINDIR",
        "TMP",
        "TEMP",
    ] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}
