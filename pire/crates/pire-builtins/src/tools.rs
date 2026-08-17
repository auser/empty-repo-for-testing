use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use pire_core::{
    ExecutionRequest, Operation, Plugin, PluginMetadata, Registry, Tool, ToolContext,
    ToolDefinition, ToolError, ToolOutput,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy)]
pub struct BuiltinToolsPolicy {
    pub allow_read: bool,
    pub allow_write: bool,
    pub allow_process: bool,
}

pub struct BuiltinToolsPlugin {
    policy: BuiltinToolsPolicy,
}

impl BuiltinToolsPlugin {
    #[must_use]
    pub fn new(policy: BuiltinToolsPolicy) -> Self {
        Self { policy }
    }
}

impl Plugin for BuiltinToolsPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(
            "pire.tools.builtin",
            env!("CARGO_PKG_VERSION"),
            "workspace-confined coding tools",
        )
        .with_dependencies(["pire.execution.host"])
    }

    fn mount(&mut self, registry: &mut Registry) -> Result<(), String> {
        if self.policy.allow_read {
            register(registry, Arc::new(ReadFileTool))?;
            register(registry, Arc::new(ListFilesTool))?;
            register(registry, Arc::new(SearchTextTool))?;
        }
        if self.policy.allow_write {
            register(registry, Arc::new(WriteFileTool))?;
            register(registry, Arc::new(EditFileTool))?;
            register(registry, Arc::new(ApplyPatchTool))?;
        }
        if self.policy.allow_process {
            register(registry, Arc::new(RunProcessTool))?;
        }
        Ok(())
    }
}

fn register(registry: &mut Registry, tool: Arc<dyn Tool>) -> Result<(), String> {
    registry
        .register_tool(tool)
        .map_err(|error| error.to_string())
}

struct ReadFileTool;

impl Tool for ReadFileTool {
    fn definition(&self) -> ToolDefinition {
        definition(
            "read_file",
            "Read a bounded UTF-8 file inside the workspace",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"],
                "additionalProperties": false
            }),
        )
    }

    fn operation(&self) -> Operation {
        Operation::Read
    }

    fn execute(&self, arguments: Value, context: &ToolContext) -> Result<ToolOutput, ToolError> {
        let arguments: PathArguments = parse(arguments)?;
        let path = existing_path(context.workspace(), &arguments.path)?;
        let content = read_utf8(&path, context.limits().max_read_bytes)?;
        Ok(ToolOutput {
            content: content.clone(),
            data: json!({
                "path": arguments.path,
                "sha256": content_hash(content.as_bytes()),
                "bytes": content.len(),
            }),
            side_effect: false,
        })
    }
}

struct ListFilesTool;

impl Tool for ListFilesTool {
    fn definition(&self) -> ToolDefinition {
        definition(
            "list_files",
            "List bounded workspace files without following symlinks",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "default": "." }
                },
                "additionalProperties": false
            }),
        )
    }

    fn operation(&self) -> Operation {
        Operation::Read
    }

    fn execute(&self, arguments: Value, context: &ToolContext) -> Result<ToolOutput, ToolError> {
        let arguments: OptionalPathArguments = parse(arguments)?;
        let relative = arguments.path.unwrap_or_else(|| ".".to_owned());
        let root = existing_path(context.workspace(), &relative)?;
        if !root.is_dir() {
            return Err(ToolError::InvalidArguments(format!(
                "`{relative}` is not a directory"
            )));
        }
        let mut files = Vec::new();
        for entry in WalkDir::new(&root).follow_links(false) {
            let entry = entry.map_err(|error| ToolError::Execution(error.to_string()))?;
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry
                .path()
                .strip_prefix(context.workspace())
                .map_err(|error| ToolError::Execution(error.to_string()))?;
            files.push(path.to_string_lossy().replace('\\', "/"));
            if files.len() >= context.limits().max_list_entries {
                break;
            }
        }
        files.sort();
        Ok(ToolOutput {
            content: files.join("\n"),
            data: json!({ "files": files }),
            side_effect: false,
        })
    }
}

struct SearchTextTool;

impl Tool for SearchTextTool {
    fn definition(&self) -> ToolDefinition {
        definition(
            "search_text",
            "Search bounded UTF-8 workspace files for literal text",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "path": { "type": "string", "default": "." }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        )
    }

    fn operation(&self) -> Operation {
        Operation::Read
    }

    fn execute(&self, arguments: Value, context: &ToolContext) -> Result<ToolOutput, ToolError> {
        let arguments: SearchArguments = parse(arguments)?;
        if arguments.query.is_empty() {
            return Err(ToolError::InvalidArguments(
                "query cannot be empty".to_owned(),
            ));
        }
        let relative = arguments.path.unwrap_or_else(|| ".".to_owned());
        let root = existing_path(context.workspace(), &relative)?;
        let mut matches = Vec::new();
        for entry in WalkDir::new(root).follow_links(false) {
            let entry = entry.map_err(|error| ToolError::Execution(error.to_string()))?;
            if !entry.file_type().is_file() {
                continue;
            }
            let Ok(content) = read_utf8(entry.path(), context.limits().max_read_bytes) else {
                continue;
            };
            for (index, line) in content.lines().enumerate() {
                if line.contains(&arguments.query) {
                    let relative = entry
                        .path()
                        .strip_prefix(context.workspace())
                        .map_err(|error| ToolError::Execution(error.to_string()))?;
                    matches.push(json!({
                        "path": relative.to_string_lossy().replace('\\', "/"),
                        "line": index.saturating_add(1),
                        "text": line,
                    }));
                    if matches.len() >= context.limits().max_list_entries {
                        break;
                    }
                }
            }
            if matches.len() >= context.limits().max_list_entries {
                break;
            }
        }
        let content = matches
            .iter()
            .filter_map(|item| {
                Some(format!(
                    "{}:{}:{}",
                    item.get("path")?.as_str()?,
                    item.get("line")?.as_u64()?,
                    item.get("text")?.as_str()?
                ))
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(ToolOutput {
            content,
            data: json!({ "matches": matches }),
            side_effect: false,
        })
    }
}

struct WriteFileTool;

impl Tool for WriteFileTool {
    fn definition(&self) -> ToolDefinition {
        definition(
            "write_file",
            "Atomically write a UTF-8 workspace file with an optional content-hash precondition",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" },
                    "expected_sha256": { "type": ["string", "null"] }
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
        )
    }

    fn operation(&self) -> Operation {
        Operation::Write
    }

    fn execute(&self, arguments: Value, context: &ToolContext) -> Result<ToolOutput, ToolError> {
        let arguments: WriteArguments = parse(arguments)?;
        if arguments.content.len() > context.limits().max_write_bytes {
            return Err(ToolError::InvalidArguments(
                "content exceeds the configured write limit".to_owned(),
            ));
        }
        let path = writable_path(context.workspace(), &arguments.path)?;
        verify_expected_hash(&path, arguments.expected_sha256.as_deref())?;
        if !context.approval().approve(
            Operation::Write,
            &arguments.path,
            "write workspace file",
        ) {
            return Err(ToolError::Denied("write was not approved".to_owned()));
        }
        atomic_write(&path, arguments.content.as_bytes())?;
        Ok(ToolOutput {
            content: format!("wrote {} bytes to {}", arguments.content.len(), arguments.path),
            data: json!({
                "path": arguments.path,
                "sha256": content_hash(arguments.content.as_bytes()),
                "bytes": arguments.content.len(),
            }),
            side_effect: true,
        })
    }
}

struct EditFileTool;

impl Tool for EditFileTool {
    fn definition(&self) -> ToolDefinition {
        definition(
            "edit_file",
            "Replace one unique literal occurrence in a workspace file using a required content hash",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "expected_sha256": { "type": "string" },
                    "old": { "type": "string" },
                    "new": { "type": "string" }
                },
                "required": ["path", "expected_sha256", "old", "new"],
                "additionalProperties": false
            }),
        )
    }

    fn operation(&self) -> Operation {
        Operation::Write
    }

    fn execute(&self, arguments: Value, context: &ToolContext) -> Result<ToolOutput, ToolError> {
        let arguments: EditArguments = parse(arguments)?;
        if arguments.old.is_empty() {
            return Err(ToolError::InvalidArguments(
                "old text cannot be empty".to_owned(),
            ));
        }
        let path = existing_path(context.workspace(), &arguments.path)?;
        let content = read_utf8(&path, context.limits().max_read_bytes)?;
        if content_hash(content.as_bytes()) != arguments.expected_sha256 {
            return Err(ToolError::Execution(
                "file changed after it was read; refresh before editing".to_owned(),
            ));
        }
        let occurrences = content.match_indices(&arguments.old).count();
        if occurrences != 1 {
            return Err(ToolError::InvalidArguments(format!(
                "old text must occur exactly once; found {occurrences}"
            )));
        }
        let updated = content.replacen(&arguments.old, &arguments.new, 1);
        if updated.len() > context.limits().max_write_bytes {
            return Err(ToolError::InvalidArguments(
                "updated file exceeds the configured write limit".to_owned(),
            ));
        }
        if !context.approval().approve(
            Operation::Write,
            &arguments.path,
            "apply hash-anchored edit",
        ) {
            return Err(ToolError::Denied("edit was not approved".to_owned()));
        }
        atomic_write(&path, updated.as_bytes())?;
        Ok(ToolOutput {
            content: format!("edited {}", arguments.path),
            data: json!({
                "path": arguments.path,
                "sha256": content_hash(updated.as_bytes()),
            }),
            side_effect: true,
        })
    }
}

struct ApplyPatchTool;

impl Tool for ApplyPatchTool {
    fn definition(&self) -> ToolDefinition {
        definition(
            "apply_patch",
            "Validate and atomically apply multiple whole-file, hash-anchored changes",
            json!({
                "type": "object",
                "properties": {
                    "changes": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "path": { "type": "string" },
                                "content": { "type": "string" },
                                "expected_sha256": { "type": ["string", "null"] }
                            },
                            "required": ["path", "content"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["changes"],
                "additionalProperties": false
            }),
        )
    }

    fn operation(&self) -> Operation {
        Operation::Write
    }

    fn execute(&self, arguments: Value, context: &ToolContext) -> Result<ToolOutput, ToolError> {
        let arguments: PatchArguments = parse(arguments)?;
        if arguments.changes.is_empty() {
            return Err(ToolError::InvalidArguments(
                "changes cannot be empty".to_owned(),
            ));
        }
        let mut staged = Vec::with_capacity(arguments.changes.len());
        for change in arguments.changes {
            if change.content.len() > context.limits().max_write_bytes {
                return Err(ToolError::InvalidArguments(format!(
                    "{} exceeds the configured write limit",
                    change.path
                )));
            }
            let path = writable_path(context.workspace(), &change.path)?;
            verify_expected_hash(&path, change.expected_sha256.as_deref())?;
            staged.push((change, path));
        }
        let subjects = staged
            .iter()
            .map(|(change, _)| change.path.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        if !context.approval().approve(
            Operation::Write,
            &subjects,
            "apply transactional multi-file patch",
        ) {
            return Err(ToolError::Denied("patch was not approved".to_owned()));
        }
        let mut results = Vec::new();
        for (change, path) in &staged {
            atomic_write(path, change.content.as_bytes())?;
            results.push(json!({
                "path": change.path,
                "sha256": content_hash(change.content.as_bytes()),
            }));
        }
        Ok(ToolOutput {
            content: format!("applied {} file change(s)", results.len()),
            data: json!({ "changes": results }),
            side_effect: true,
        })
    }
}

struct RunProcessTool;

impl Tool for RunProcessTool {
    fn definition(&self) -> ToolDefinition {
        definition(
            "run_process",
            "Run a bounded process in the workspace without invoking a shell",
            json!({
                "type": "object",
                "properties": {
                    "program": { "type": "string" },
                    "args": { "type": "array", "items": { "type": "string" } },
                    "clear_env": { "type": "boolean", "default": true }
                },
                "required": ["program"],
                "additionalProperties": false
            }),
        )
    }

    fn operation(&self) -> Operation {
        Operation::Process
    }

    fn execute(&self, arguments: Value, context: &ToolContext) -> Result<ToolOutput, ToolError> {
        let arguments: ProcessArguments = parse(arguments)?;
        if arguments.program.trim().is_empty() {
            return Err(ToolError::InvalidArguments(
                "program cannot be empty".to_owned(),
            ));
        }
        if !context.approval().approve(
            Operation::Process,
            &arguments.program,
            "execute workspace process",
        ) {
            return Err(ToolError::Denied("process was not approved".to_owned()));
        }
        let result = context
            .execution()
            .execute(
                &ExecutionRequest {
                    program: arguments.program,
                    args: arguments.args,
                    cwd: context.workspace().to_path_buf(),
                    env: Default::default(),
                    clear_env: arguments.clear_env,
                    timeout: context.limits().process_timeout,
                    max_output_bytes: context.limits().max_process_output_bytes,
                },
                context.cancellation(),
            )
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        let content = format!(
            "exit={:?} timed_out={}\nstdout:\n{}\nstderr:\n{}",
            result.exit_code, result.timed_out, result.stdout, result.stderr
        );
        Ok(ToolOutput {
            content,
            data: json!({
                "exit_code": result.exit_code,
                "timed_out": result.timed_out,
                "stdout": result.stdout,
                "stderr": result.stderr,
            }),
            side_effect: true,
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

fn parse<T: for<'de> Deserialize<'de>>(arguments: Value) -> Result<T, ToolError> {
    serde_json::from_value(arguments).map_err(|error| ToolError::InvalidArguments(error.to_string()))
}

fn existing_path(workspace: &Path, relative: &str) -> Result<PathBuf, ToolError> {
    let workspace = workspace
        .canonicalize()
        .map_err(|error| ToolError::Execution(error.to_string()))?;
    let requested = workspace.join(relative);
    let canonical = requested
        .canonicalize()
        .map_err(|error| ToolError::Execution(error.to_string()))?;
    if !canonical.starts_with(&workspace) {
        return Err(ToolError::Denied(
            "path escapes the configured workspace".to_owned(),
        ));
    }
    Ok(canonical)
}

fn writable_path(workspace: &Path, relative: &str) -> Result<PathBuf, ToolError> {
    if relative.trim().is_empty() {
        return Err(ToolError::InvalidArguments("path cannot be empty".to_owned()));
    }
    let workspace = workspace
        .canonicalize()
        .map_err(|error| ToolError::Execution(error.to_string()))?;
    let requested = workspace.join(relative);
    let parent = requested
        .parent()
        .ok_or_else(|| ToolError::InvalidArguments("path has no parent".to_owned()))?;
    fs::create_dir_all(parent).map_err(|error| ToolError::Execution(error.to_string()))?;
    let parent = parent
        .canonicalize()
        .map_err(|error| ToolError::Execution(error.to_string()))?;
    if !parent.starts_with(&workspace) {
        return Err(ToolError::Denied(
            "path escapes the configured workspace".to_owned(),
        ));
    }
    let name = requested
        .file_name()
        .ok_or_else(|| ToolError::InvalidArguments("path has no file name".to_owned()))?;
    Ok(parent.join(name))
}

fn read_utf8(path: &Path, limit: usize) -> Result<String, ToolError> {
    let file = OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(|error| ToolError::Execution(error.to_string()))?;
    let take = u64::try_from(limit)
        .map_err(|_| ToolError::InvalidArguments("read limit is too large".to_owned()))?
        .saturating_add(1);
    let mut bytes = Vec::new();
    file.take(take)
        .read_to_end(&mut bytes)
        .map_err(|error| ToolError::Execution(error.to_string()))?;
    if bytes.len() > limit {
        return Err(ToolError::Execution(
            "file exceeds the configured read limit".to_owned(),
        ));
    }
    String::from_utf8(bytes).map_err(|error| ToolError::Execution(error.to_string()))
}

fn verify_expected_hash(path: &Path, expected: Option<&str>) -> Result<(), ToolError> {
    let Some(expected) = expected else {
        return Ok(());
    };
    if !path.exists() {
        return Err(ToolError::Execution(
            "expected hash was supplied for a file that does not exist".to_owned(),
        ));
    }
    let bytes = fs::read(path).map_err(|error| ToolError::Execution(error.to_string()))?;
    if content_hash(&bytes) != expected {
        return Err(ToolError::Execution(
            "file changed after it was read; refresh before writing".to_owned(),
        ));
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), ToolError> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ToolError::InvalidArguments("invalid destination file name".to_owned()))?;
    let temporary = path.with_file_name(format!(".{name}.pire-tmp-{}", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&temporary)
        .map_err(|error| ToolError::Execution(error.to_string()))?;
    file.write_all(bytes)
        .map_err(|error| ToolError::Execution(error.to_string()))?;
    file.sync_all()
        .map_err(|error| ToolError::Execution(error.to_string()))?;
    #[cfg(windows)]
    if path.exists() {
        fs::remove_file(path).map_err(|error| ToolError::Execution(error.to_string()))?;
    }
    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        ToolError::Execution(error.to_string())
    })
}

fn content_hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[derive(Debug, Deserialize)]
struct PathArguments {
    path: String,
}

#[derive(Debug, Deserialize)]
struct OptionalPathArguments {
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SearchArguments {
    query: String,
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WriteArguments {
    path: String,
    content: String,
    #[serde(default)]
    expected_sha256: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EditArguments {
    path: String,
    expected_sha256: String,
    old: String,
    new: String,
}

#[derive(Debug, Deserialize)]
struct PatchArguments {
    changes: Vec<WriteArguments>,
}

#[derive(Debug, Deserialize)]
struct ProcessArguments {
    program: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default = "default_true")]
    clear_env: bool,
}

const fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::content_hash;

    #[test]
    fn hash_is_stable() {
        assert_eq!(
            content_hash(b"pire"),
            "5ef28c338eb58a5c0eced61c707478643a513f44916bbd253f471f6116a92449"
        );
    }
}
