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
    pub workspace: String,

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

    pub fn create(
        &self,
        workspace: &Path,
        parent: Option<String>,
    ) -> Result<SessionWriter, SessionError> {
        let metadata = SessionMetadata {
            id: Uuid::new_v4().to_string(),
            created_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            workspace: workspace.to_string_lossy().into_owned(),
            parent,
        };
        let path = self.path_for(&metadata.id)?;
        let file = OpenOptions::new().create_new(true).write(true).open(path)?;
        let mut writer = SessionWriter {
            metadata: metadata.clone(),
            writer: BufWriter::new(file),
        };
        writer.append_record(&SessionRecord::Metadata { value: metadata })?;
        Ok(writer)
    }

    pub fn append(&self, id: &str) -> Result<SessionWriter, SessionError> {
        let data = self.load(id)?;
        let path = self.path_for(id)?;
        let file = OpenOptions::new().append(true).open(path)?;
        Ok(SessionWriter {
            metadata: data.metadata,
            writer: BufWriter::new(file),
        })
    }

    pub fn load(&self, id: &str) -> Result<SessionData, SessionError> {
        let path = self.path_for(id)?;
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
            }
        }

        let metadata = metadata.ok_or_else(|| SessionError::MissingMetadata(id.to_owned()))?;
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
            let file = File::open(entry.path())?;
            let mut lines = BufReader::new(file).lines();
            let Some(line) = lines.next() else {
                continue;
            };
            if let SessionRecord::Metadata { value } = serde_json::from_str(&line?)? {
                sessions.push(value);
            }
        }
        sessions.sort_by_key(|session| std::cmp::Reverse(session.created_unix_ms));
        Ok(sessions)
    }

    fn path_for(&self, id: &str) -> Result<PathBuf, SessionError> {
        let parsed = Uuid::parse_str(id).map_err(|_| SessionError::InvalidId(id.to_owned()))?;
        Ok(self.directory.join(format!("{parsed}.jsonl")))
    }
}

pub struct SessionWriter {
    metadata: SessionMetadata,
    writer: BufWriter<File>,
}

impl SessionWriter {
    #[must_use]
    pub fn metadata(&self) -> &SessionMetadata {
        &self.metadata
    }

    pub fn append_message(&mut self, message: &Message) -> Result<(), SessionError> {
        self.append_record(&SessionRecord::Message {
            value: message.clone(),
        })
    }

    pub fn append_event(&mut self, event: &AgentEvent) -> Result<(), SessionError> {
        self.append_record(&SessionRecord::Event {
            value: event.clone(),
        })
    }

    fn append_record(&mut self, record: &SessionRecord) -> Result<(), SessionError> {
        serde_json::to_writer(&mut self.writer, record)?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()?;
        Ok(())
    }
}
