use std::{
    io::{self, Read},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use pire_core::{
    ApprovalAction, ApprovalRequest, Tool, ToolContext, ToolDefinition, ToolError, ToolOutput,
    ToolRegistry,
};
use serde::Deserialize;
use serde_json::{Value, json};
use wait_timeout::ChildExt;

use crate::config::ToolsConfig;

pub fn registry(config: &ToolsConfig) -> Result<ToolRegistry, ToolError> {
    let mut registry = ToolRegistry::new();
    if config.allow_read {
        registry.register(ReadFile)?;
        registry.register(ListFiles)?;
        registry.register(SearchText)?;
    }
    if config.allow_write {
        registry.register(WriteFile)?;
        registry.register(EditFile)?;
    }
    if config.allow_process {
        registry.register(RunProcess)?;
    }
    Ok(registry)
}

struct ReadFile;
struct WriteFile;
struct EditFile;
struct ListFiles;
struct SearchText;
struct RunProcess;

#[derive(Debug, Deserialize)]
struct PathArgs {
    path: String,
}

impl Tool for ReadFile {
    fn definition(&self) -> ToolDefinition {
        definition("read_file", "Read a UTF-8 file inside the workspace", json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"],
            "additionalProperties": false
        }))
    }

    fn execute(&self, context: &mut ToolContext<'_>, arguments: &Value) -> Result<ToolOutput, ToolError> {
        let args: PathArgs = parse(arguments)?;
        approve(context, ApprovalAction::Read, "read_file", format!("read {}", args.path))?;
        let content = context
            .workspace
            .read_text(&args.path, context.limits.max_read_bytes)
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        Ok(ToolOutput::success(content))
    }
}

#[derive(Debug, Deserialize)]
struct WriteArgs {
    path: String,
    content: String,
}

impl Tool for WriteFile {
    fn definition(&self) -> ToolDefinition {
        definition("write_file", "Create or replace a UTF-8 file inside the workspace", json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" }
            },
            "required": ["path", "content"],
            "additionalProperties": false
        }))
    }

    fn execute(&self, context: &mut ToolContext<'_>, arguments: &Value) -> Result<ToolOutput, ToolError> {
        let args: WriteArgs = parse(arguments)?;
        approve(context, ApprovalAction::Write, "write_file", format!("write {}", args.path))?;
        let path = context
            .workspace
            .write_text(&args.path, &args.content, context.limits.max_write_bytes)
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        Ok(ToolOutput::success(format!("wrote {}", path.display())))
    }
}

#[derive(Debug, Deserialize)]
struct EditArgs {
    path: String,
    old: String,
    new: String,
}

impl Tool for EditFile {
    fn definition(&self) -> ToolDefinition {
        definition("edit_file", "Replace one exact text occurrence in a workspace file", json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "old": { "type": "string" },
                "new": { "type": "string" }
            },
            "required": ["path", "old", "new"],
            "additionalProperties": false
        }))
    }

    fn execute(&self, context: &mut ToolContext<'_>, arguments: &Value) -> Result<ToolOutput, ToolError> {
        let args: EditArgs = parse(arguments)?;
        approve(context, ApprovalAction::Write, "edit_file", format!("edit {}", args.path))?;
        if args.old.is_empty() {
            return Err(ToolError::InvalidArguments("old text cannot be empty".to_owned()));
        }
        let content = context
            .workspace
            .read_text(&args.path, context.limits.max_read_bytes)
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        let occurrences = content.matches(&args.old).count();
        if occurrences != 1 {
            return Err(ToolError::Execution(format!(
                "expected exactly one occurrence, found {occurrences}"
            )));
        }
        let updated = content.replacen(&args.old, &args.new, 1);
        context
            .workspace
            .write_text(&args.path, &updated, context.limits.max_write_bytes)
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        Ok(ToolOutput::success(format!("edited {}", args.path)))
    }
}

#[derive(Debug, Deserialize)]
struct ListArgs {
    #[serde(default = "dot")]
    path: String,
}

impl Tool for ListFiles {
    fn definition(&self) -> ToolDefinition {
        definition("list_files", "List files recursively inside the workspace", json!({
            "type": "object",
            "properties": { "path": { "type": "string", "default": "." } },
            "additionalProperties": false
        }))
    }

    fn execute(&self, context: &mut ToolContext<'_>, arguments: &Value) -> Result<ToolOutput, ToolError> {
        let args: ListArgs = parse(arguments)?;
        approve(context, ApprovalAction::Read, "list_files", format!("list {}", args.path))?;
        let files = context
            .workspace
            .list_files(&args.path, context.limits.max_list_entries)
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        Ok(ToolOutput::success(files.join("\n")))
    }
}

#[derive(Debug, Deserialize)]
struct SearchArgs {
    query: String,
    #[serde(default = "dot")]
    path: String,
}

impl Tool for SearchText {
    fn definition(&self) -> ToolDefinition {
        definition("search_text", "Search for literal text in UTF-8 workspace files", json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "path": { "type": "string", "default": "." }
            },
            "required": ["query"],
            "additionalProperties": false
        }))
    }

    fn execute(&self, context: &mut ToolContext<'_>, arguments: &Value) -> Result<ToolOutput, ToolError> {
        let args: SearchArgs = parse(arguments)?;
        approve(context, ApprovalAction::Read, "search_text", format!("search for {:?}", args.query))?;
        let files = context
            .workspace
            .list_files(&args.path, context.limits.max_list_entries)
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        let mut matches = Vec::new();
        for file in files {
            let Ok(content) = context.workspace.read_text(&file, context.limits.max_read_bytes) else {
                continue;
            };
            for (index, line) in content.lines().enumerate() {
                if line.contains(&args.query) {
                    matches.push(format!("{}:{}:{}", file, index + 1, line));
                }
            }
        }
        Ok(ToolOutput::success(matches.join("\n")))
    }
}

#[derive(Debug, Deserialize)]
struct ProcessArgs {
    program: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default = "dot")]
    cwd: String,
}

impl Tool for RunProcess {
    fn definition(&self) -> ToolDefinition {
        definition("run_process", "Run a bounded process inside the trusted workspace", json!({
            "type": "object",
            "properties": {
                "program": { "type": "string" },
                "args": { "type": "array", "items": { "type": "string" } },
                "cwd": { "type": "string", "default": "." }
            },
            "required": ["program"],
            "additionalProperties": false
        }))
    }

    fn execute(&self, context: &mut ToolContext<'_>, arguments: &Value) -> Result<ToolOutput, ToolError> {
        let args: ProcessArgs = parse(arguments)?;
        approve(
            context,
            ApprovalAction::Process,
            "run_process",
            format!("run {} {:?}", args.program, args.args),
        )?;
        let cwd = context
            .workspace
            .resolve_existing(&args.cwd)
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        let mut child = Command::new(&args.program)
            .args(&args.args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        let stdout = child.stdout.take().ok_or_else(|| ToolError::Execution("stdout unavailable".to_owned()))?;
        let stderr = child.stderr.take().ok_or_else(|| ToolError::Execution("stderr unavailable".to_owned()))?;
        let stdout_reader = read_stream(stdout, context.limits.max_process_output_bytes);
        let stderr_reader = read_stream(stderr, context.limits.max_process_output_bytes);
        let timeout = Duration::from_secs(context.limits.process_timeout_seconds);
        let status = child
            .wait_timeout(timeout)
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        let status = match status {
            Some(status) => status,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ToolError::Execution(format!(
                    "process exceeded a {}-second timeout",
                    timeout.as_secs()
                )));
            }
        };
        let stdout = join_reader(stdout_reader)?;
        let stderr = join_reader(stderr_reader)?;
        let result = json!({
            "status": status.code(),
            "success": status.success(),
            "stdout": String::from_utf8_lossy(&stdout),
            "stderr": String::from_utf8_lossy(&stderr),
        });
        Ok(ToolOutput {
            content: result.to_string(),
            is_error: !status.success(),
        })
    }
}

fn definition(name: &str, description: &str, input_schema: Value) -> ToolDefinition {
    ToolDefinition {
        name: name.to_owned(),
        description: description.to_owned(),
        input_schema,
    }
}

fn parse<T>(arguments: &Value) -> Result<T, ToolError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(arguments.clone())
        .map_err(|error| ToolError::InvalidArguments(error.to_string()))
}

fn approve(
    context: &mut ToolContext<'_>,
    action: ApprovalAction,
    tool: &str,
    summary: String,
) -> Result<(), ToolError> {
    let request = ApprovalRequest {
        action,
        tool: tool.to_owned(),
        summary,
    };
    let approved = context
        .approval
        .approve(&request)
        .map_err(|error| ToolError::Execution(error.to_string()))?;
    if approved {
        Ok(())
    } else {
        Err(ToolError::Denied(request.summary))
    }
}

fn dot() -> String {
    ".".to_owned()
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
            return Err(io::Error::new(io::ErrorKind::InvalidData, "process output limit exceeded"));
        }
        Ok(bytes)
    })
}

fn join_reader(handle: thread::JoinHandle<io::Result<Vec<u8>>>) -> Result<Vec<u8>, ToolError> {
    handle
        .join()
        .map_err(|_| ToolError::Execution("process output reader panicked".to_owned()))?
        .map_err(|error| ToolError::Execution(error.to_string()))
}
