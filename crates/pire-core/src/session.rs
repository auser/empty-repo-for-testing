use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{AgentEvent, Message};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub id: String,
    pub created_unix_ms: u128,

    #[serde(default)]
    pub updated_unix_ms: u128,

    pub workspace: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SessionData {
    pub metadata: SessionMetadata,
    pub messages: Vec<Message>,
    pub events: Vec<AgentEvent>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "record", rename_all = "snake_case")]
enum SessionRecord {
    Metadata { value: SessionMetadata },
    Message { value: Message },
    Event { value: AgentEvent },
    Checkpoint { reason: String, messages: Vec<Message> },
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("session I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("session JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("session does not contain metadata: {0}")]
    MissingMetadata(String),

    #[error("invalid session identifier: {0}")]
    InvalidId(String),

    #[error("session identifier is ambiguous: {0}")]
    AmbiguousId(String),
}

#[derive(Debug, Clone)]
pub struct SessionStore {
    directory: PathBuf,
}

impl SessionStore {
    pub fn open(directory: impl AsRef<Path>) -> Result<Self, SessionError> {
        fs::create_dir_all(directory.as_ref())?;
        Ok(Self {
            directory: directory.as_ref().to_path_buf(),
        })
    }

    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn create(
        &self,
        workspace: &Path,
        parent: Option<String>,
    ) -> Result<SessionWriter, SessionError> {
        self.create_named(workspace, parent, None)
    }

    pub fn create_named(
        &self,
        workspace: &Path,
        parent: Option<String>,
        name: Option<String>,
    ) -> Result<SessionWriter, SessionError> {
        let now = unix_ms();
        let metadata = SessionMetadata {
            id: Uuid::new_v4().to_string(),
            created_unix_ms: now,
            updated_unix_ms: now,
            workspace: workspace.to_string_lossy().into_owned(),
            name,
            parent,
        };
        let path = self.path_for_exact(&metadata.id)?;
        let file = OpenOptions::new().create_new(true).write(true).open(&path)?;
        let mut writer = SessionWriter {
            metadata: metadata.clone(),
            path,
            writer: BufWriter::new(file),
        };
        writer.append_record(&SessionRecord::Metadata { value: metadata })?;
        Ok(writer)
    }

    pub fn append(&self, id: &str) -> Result<SessionWriter, SessionError> {
        let resolved = self.resolve_id(id)?;
        let data = self.load(&resolved)?;
        let path = self.path_for_exact(&resolved)?;
        let file = OpenOptions::new().append(true).open(&path)?;
        Ok(SessionWriter {
            metadata: data.metadata,
            path,
            writer: BufWriter::new(file),
        })
    }

    pub fn load(&self, id: &str) -> Result<SessionData, SessionError> {
        let resolved = self.resolve_id(id)?;
        let path = self.path_for_exact(&resolved)?;
        let file = File::open(path)?;
        let mut metadata = None;
        let mut messages = Vec::new();
        let mut events = Vec::new();

        for line in BufReader::new(file).lines() {
            let record: SessionRecord = serde_json::from_str(&line?)?;
            match record {
                SessionRecord::Metadata { value } => metadata = Some(value),
                SessionRecord::Message { value } => messages.push(value),
                SessionRecord::Event { value } => events.push(value),
                SessionRecord::Checkpoint {
                    reason: _,
                    messages: value,
                } => messages = value,
            }
        }

        let metadata = metadata.ok_or_else(|| SessionError::MissingMetadata(resolved))?;
        Ok(SessionData {
            metadata,
            messages,
            events,
        })
    }

    pub fn list(&self) -> Result<Vec<SessionMetadata>, SessionError> {
        let mut sessions = Vec::new();
        for entry in fs::read_dir(&self.directory)? {
            let entry = entry?;
            if entry.path().extension().and_then(|value| value.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(id) = entry.path().file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            if let Ok(data) = self.load(id) {
                sessions.push(data.metadata);
            }
        }
        sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_unix_ms));
        Ok(sessions)
    }

    pub fn resolve_id(&self, id: &str) -> Result<String, SessionError> {
        if Uuid::parse_str(id).is_ok() && self.path_for_exact(id)?.is_file() {
            return Ok(id.to_owned());
        }
        let mut matches = Vec::new();
        for entry in fs::read_dir(&self.directory)? {
            let entry = entry?;
            let Some(stem) = entry.path().file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            if stem.starts_with(id) {
                matches.push(stem.to_owned());
            }
        }
        match matches.as_slice() {
            [resolved] => Ok(resolved.clone()),
            [] => Err(SessionError::InvalidId(id.to_owned())),
            _ => Err(SessionError::AmbiguousId(id.to_owned())),
        }
    }

    fn path_for_exact(&self, id: &str) -> Result<PathBuf, SessionError> {
        let parsed = Uuid::parse_str(id).map_err(|_| SessionError::InvalidId(id.to_owned()))?;
        Ok(self.directory.join(format!("{parsed}.jsonl")))
    }
}

pub struct SessionWriter {
    metadata: SessionMetadata,
    path: PathBuf,
    writer: BufWriter<File>,
}

impl SessionWriter {
    #[must_use]
    pub fn metadata(&self) -> &SessionMetadata {
        &self.metadata
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn rename(&mut self, name: Option<String>) -> Result<(), SessionError> {
        self.metadata.name = name;
        self.metadata.updated_unix_ms = unix_ms();
        self.append_record(&SessionRecord::Metadata {
            value: self.metadata.clone(),
        })
    }

    pub fn append_message(&mut self, message: &Message) -> Result<(), SessionError> {
        self.metadata.updated_unix_ms = unix_ms();
        self.append_record(&SessionRecord::Message {
            value: message.clone(),
        })
    }

    pub fn append_event(&mut self, event: &AgentEvent) -> Result<(), SessionError> {
        self.metadata.updated_unix_ms = unix_ms();
        self.append_record(&SessionRecord::Event {
            value: event.clone(),
        })
    }

    pub fn checkpoint(
        &mut self,
        reason: impl Into<String>,
        messages: &[Message],
    ) -> Result<(), SessionError> {
        self.metadata.updated_unix_ms = unix_ms();
        self.append_record(&SessionRecord::Checkpoint {
            reason: reason.into(),
            messages: messages.to_vec(),
        })
    }

    fn append_record(&mut self, record: &SessionRecord) -> Result<(), SessionError> {
        serde_json::to_writer(&mut self.writer, record)?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()?;
        Ok(())
    }
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
