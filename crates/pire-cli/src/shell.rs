use std::{
    io::{self, Read},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use pire_core::{ApprovalAction, ApprovalPolicy, ApprovalRequest, Workspace};
use wait_timeout::ChildExt;

use crate::{config::ToolsConfig, error::PireError};

pub struct ShellResult {
    pub success: bool,
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

pub fn run(
    command: &str,
    workspace: &Workspace,
    config: &ToolsConfig,
    approval: &mut dyn ApprovalPolicy,
) -> Result<ShellResult, PireError> {
    if !config.allow_process {
        return Err(PireError::Message(
            "process execution is disabled by tools.allow_process".to_owned(),
        ));
    }
    let approved = approval
        .approve(&ApprovalRequest {
            action: ApprovalAction::Process,
            tool: "interactive_shell".to_owned(),
            summary: command.to_owned(),
        })
        .map_err(|error| PireError::Message(error.to_string()))?;
    if !approved {
        return Err(PireError::Message("shell command was denied".to_owned()));
    }

    #[cfg(windows)]
    let mut child = Command::new("cmd")
        .args(["/C", command])
        .current_dir(workspace.root())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    #[cfg(not(windows))]
    let mut child = Command::new("sh")
        .args(["-lc", command])
        .current_dir(workspace.root())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| PireError::Message("shell stdout was unavailable".to_owned()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| PireError::Message("shell stderr was unavailable".to_owned()))?;
    let stdout_reader = read_stream(stdout, config.max_process_output_bytes);
    let stderr_reader = read_stream(stderr, config.max_process_output_bytes);
    let timeout = Duration::from_secs(config.process_timeout_secs);
    let status = match child.wait_timeout(timeout)? {
        Some(status) => status,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(PireError::Message(format!(
                "shell command exceeded a {}-second timeout",
                timeout.as_secs()
            )));
        }
    };
    let stdout = join_reader(stdout_reader, "stdout")?;
    let stderr = join_reader(stderr_reader, "stderr")?;
    Ok(ShellResult {
        success: status.success(),
        status: status.code(),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    })
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
                format!("shell output exceeded {max_bytes} bytes"),
            ));
        }
        Ok(bytes)
    })
}

fn join_reader(
    handle: thread::JoinHandle<io::Result<Vec<u8>>>,
    name: &str,
) -> Result<Vec<u8>, PireError> {
    handle
        .join()
        .map_err(|_| PireError::Message(format!("shell {name} reader panicked")))?
        .map_err(PireError::Io)
}
