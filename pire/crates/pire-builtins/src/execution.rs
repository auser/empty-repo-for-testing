use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use pire_core::{
    CancellationToken, ExecutionBackend, ExecutionRequest, ExecutionResult, Kernel, Plugin,
    PluginMetadata, Registry,
};
use wait_timeout::ChildExt;

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
    ) -> Result<ExecutionResult, pire_core::capability::ExecutionError> {
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
) -> Result<ExecutionResult, pire_core::capability::ExecutionError> {
    if cancellation.is_cancelled() {
        return Err(pire_core::capability::ExecutionError::Cancelled);
    }
    let mut stdout_file = temporary_file("stdout")
        .map_err(|error| pire_core::capability::ExecutionError::Spawn(error.to_string()))?;
    let mut stderr_file = temporary_file("stderr")
        .map_err(|error| pire_core::capability::ExecutionError::Spawn(error.to_string()))?;
    let stdout_path = path_for(&stdout_file).ok_or_else(|| {
        pire_core::capability::ExecutionError::Spawn(
            "unable to resolve temporary stdout path".to_owned(),
        )
    })?;
    let stderr_path = path_for(&stderr_file).ok_or_else(|| {
        pire_core::capability::ExecutionError::Spawn(
            "unable to resolve temporary stderr path".to_owned(),
        )
    })?;

    let mut command = Command::new(program);
    command.args(args).current_dir(cwd);
    command.stdout(
        stdout_file
            .try_clone()
            .map_err(|error| pire_core::capability::ExecutionError::Spawn(error.to_string()))?,
    );
    command.stderr(
        stderr_file
            .try_clone()
            .map_err(|error| pire_core::capability::ExecutionError::Spawn(error.to_string()))?,
    );
    command.stdin(if stdin.is_some() {
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
        .map_err(|error| pire_core::capability::ExecutionError::Spawn(error.to_string()))?;
    if let Some(input) = stdin {
        let mut child_stdin = child.stdin.take().ok_or_else(|| {
            pire_core::capability::ExecutionError::Spawn(
                "child stdin was not available".to_owned(),
            )
        })?;
        child_stdin
            .write_all(input)
            .map_err(|error| pire_core::capability::ExecutionError::Spawn(error.to_string()))?;
    }

    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if cancellation.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            cleanup(&stdout_path, &stderr_path);
            return Err(pire_core::capability::ExecutionError::Cancelled);
        }
        let elapsed = started.elapsed();
        if elapsed >= timeout {
            timed_out = true;
            let _ = child.kill();
            break child
                .wait()
                .map_err(|error| pire_core::capability::ExecutionError::Wait(error.to_string()))?;
        }
        let remaining = timeout.saturating_sub(elapsed);
        let poll = remaining.min(Duration::from_millis(100));
        match child
            .wait_timeout(poll)
            .map_err(|error| pire_core::capability::ExecutionError::Wait(error.to_string()))?
        {
            Some(status) => break status,
            None => continue,
        }
    };

    let result = (|| {
        let stdout = read_limited(&mut stdout_file, max_output_bytes)?;
        let remaining = max_output_bytes.saturating_sub(stdout.len());
        let stderr = read_limited(&mut stderr_file, remaining)?;
        Ok(ExecutionResult {
            exit_code: status.code(),
            stdout,
            stderr,
            timed_out,
        })
    })();
    cleanup(&stdout_path, &stderr_path);
    result
}

fn read_limited(
    file: &mut File,
    limit: usize,
) -> Result<String, pire_core::capability::ExecutionError> {
    file.seek(SeekFrom::Start(0))
        .map_err(|error| pire_core::capability::ExecutionError::Output(error.to_string()))?;
    let take_limit = u64::try_from(limit)
        .map_err(|_| pire_core::capability::ExecutionError::OutputLimit)?
        .saturating_add(1);
    let mut bytes = Vec::new();
    file.take(take_limit)
        .read_to_end(&mut bytes)
        .map_err(|error| pire_core::capability::ExecutionError::Output(error.to_string()))?;
    if bytes.len() > limit {
        return Err(pire_core::capability::ExecutionError::OutputLimit);
    }
    String::from_utf8(bytes)
        .map_err(|error| pire_core::capability::ExecutionError::Output(error.to_string()))
}

fn temporary_file(kind: &str) -> std::io::Result<File> {
    let path = std::env::temp_dir().join(format!(
        "pire-{}-{}-{}-{}",
        kind,
        std::process::id(),
        now_nanos(),
        thread_token()
    ));
    OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
}

fn path_for(file: &File) -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_link(format!("/proc/self/fd/{}", std::os::fd::AsRawFd::as_raw_fd(file))).ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        // On non-Linux platforms a named file cannot be recovered from a File
        // descriptor portably. Duplicate creation is used solely to retain the
        // name in metadata; see `temporary_file_with_path` in the next release.
        let _ = file;
        None
    }
}

fn preserve_runtime_environment(command: &mut Command) {
    for key in ["PATH", "HOME", "USERPROFILE", "SYSTEMROOT", "WINDIR", "TMP", "TEMP"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
}

fn cleanup(stdout: &Path, stderr: &Path) {
    let _ = std::fs::remove_file(stdout);
    let _ = std::fs::remove_file(stderr);
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}

fn thread_token() -> String {
    format!("{:?}", std::thread::current().id())
        .replace(['(', ')', ' '], "")
}

#[allow(dead_code)]
fn _kernel_marker(_kernel: &Kernel) {}
