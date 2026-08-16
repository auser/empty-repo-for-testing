use std::{
    collections::VecDeque,
    fs::{self, OpenOptions},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

use crossterm::{
    cursor::{MoveToColumn, MoveToNextLine, MoveUp},
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers,
    },
    execute, queue,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode, size},
};
use pire_core::Workspace;

use super::SlashCommand;
use crate::resources;

const PROMPT: &str = "› ";

pub enum EditorResult {
    Submit(String),
    Quit,
}

pub struct LineEditor {
    history: VecDeque<String>,
    history_cursor: Option<usize>,
    draft: String,
    history_path: PathBuf,
    history_limit: usize,
    completion_limit: usize,
    color: bool,
}

impl LineEditor {
    pub fn open(
        history_path: PathBuf,
        history_limit: usize,
        completion_limit: usize,
        color: bool,
    ) -> io::Result<Self> {
        let history = load_history(&history_path, history_limit)?;
        Ok(Self {
            history,
            history_cursor: None,
            draft: String::new(),
            history_path,
            history_limit,
            completion_limit,
            color,
        })
    }

    pub fn read_line(
        &mut self,
        footer: &str,
        commands: &[SlashCommand],
        workspace: &Workspace,
    ) -> io::Result<EditorResult> {
        let mut terminal = TerminalGuard::enter()?;
        let mut buffer = Vec::<char>::new();
        let mut cursor = 0usize;
        let mut completion_index = 0usize;
        let mut stdout = io::stdout();

        loop {
            let completions = completions(
                &buffer,
                cursor,
                commands,
                workspace,
                self.completion_limit,
            );
            if completion_index >= completions.len() {
                completion_index = 0;
            }
            render(
                &mut stdout,
                &buffer,
                cursor,
                footer,
                &completions,
                completion_index,
                self.color,
            )?;

            match event::read()? {
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    if let Some(result) = self.handle_key(
                        key,
                        &mut buffer,
                        &mut cursor,
                        &completions,
                        &mut completion_index,
                    )? {
                        clear_editor(&mut stdout, &buffer, self.color)?;
                        terminal.leave()?;
                        if let EditorResult::Submit(value) = &result {
                            self.remember(value)?;
                        }
                        return Ok(result);
                    }
                }
                Event::Paste(value) => {
                    insert_text(&mut buffer, &mut cursor, &value.replace("\r\n", "\n"));
                    completion_index = 0;
                    self.reset_history_navigation();
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }

    fn handle_key(
        &mut self,
        key: KeyEvent,
        buffer: &mut Vec<char>,
        cursor: &mut usize,
        completions: &[Completion],
        completion_index: &mut usize,
    ) -> io::Result<Option<EditorResult>> {
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Enter if alt || control => {
                buffer.insert(*cursor, '\n');
                *cursor = cursor.saturating_add(1);
            }
            KeyCode::Enter => {
                let value = buffer.iter().collect::<String>();
                if !value.trim().is_empty() {
                    return Ok(Some(EditorResult::Submit(value)));
                }
            }
            KeyCode::Char('c') if control => {
                if buffer.is_empty() {
                    return Ok(Some(EditorResult::Quit));
                }
                buffer.clear();
                *cursor = 0;
                self.reset_history_navigation();
            }
            KeyCode::Char('d') if control && buffer.is_empty() => {
                return Ok(Some(EditorResult::Quit));
            }
            KeyCode::Char('l') if control => {
                return Ok(Some(EditorResult::Submit("/model".to_owned())));
            }
            KeyCode::Char('a') if control => *cursor = 0,
            KeyCode::Char('e') if control => *cursor = buffer.len(),
            KeyCode::Char('u') if control => {
                buffer.drain(..*cursor);
                *cursor = 0;
            }
            KeyCode::Char('k') if control => {
                buffer.truncate(*cursor);
            }
            KeyCode::Char(character) => {
                buffer.insert(*cursor, character);
                *cursor = cursor.saturating_add(1);
                *completion_index = 0;
                self.reset_history_navigation();
            }
            KeyCode::Backspace if *cursor > 0 => {
                *cursor = cursor.saturating_sub(1);
                buffer.remove(*cursor);
                *completion_index = 0;
                self.reset_history_navigation();
            }
            KeyCode::Delete if *cursor < buffer.len() => {
                buffer.remove(*cursor);
                *completion_index = 0;
                self.reset_history_navigation();
            }
            KeyCode::Left if *cursor > 0 => *cursor = cursor.saturating_sub(1),
            KeyCode::Right if *cursor < buffer.len() => *cursor = cursor.saturating_add(1),
            KeyCode::Home => *cursor = 0,
            KeyCode::End => *cursor = buffer.len(),
            KeyCode::Tab => {
                if let Some(completion) = completions.get(*completion_index) {
                    apply_completion(buffer, cursor, &completion.value);
                    *completion_index = completion_index.saturating_add(1);
                }
            }
            KeyCode::BackTab => {
                if !completions.is_empty() {
                    *completion_index = completion_index
                        .checked_sub(1)
                        .unwrap_or(completions.len().saturating_sub(1));
                }
            }
            KeyCode::Up if !completions.is_empty() && starts_completion(buffer) => {
                *completion_index = completion_index
                    .checked_sub(1)
                    .unwrap_or(completions.len().saturating_sub(1));
            }
            KeyCode::Down if !completions.is_empty() && starts_completion(buffer) => {
                *completion_index = completion_index.saturating_add(1) % completions.len();
            }
            KeyCode::Up => self.history_previous(buffer, cursor),
            KeyCode::Down => self.history_next(buffer, cursor),
            KeyCode::Esc => {
                if buffer.is_empty() {
                    return Ok(Some(EditorResult::Quit));
                }
                buffer.clear();
                *cursor = 0;
                self.reset_history_navigation();
            }
            _ => {}
        }
        Ok(None)
    }

    fn history_previous(&mut self, buffer: &mut Vec<char>, cursor: &mut usize) {
        if self.history.is_empty() {
            return;
        }
        let next = match self.history_cursor {
            None => {
                self.draft = buffer.iter().collect();
                self.history.len().saturating_sub(1)
            }
            Some(current) => current.saturating_sub(1),
        };
        self.history_cursor = Some(next);
        replace_buffer(buffer, cursor, &self.history[next]);
    }

    fn history_next(&mut self, buffer: &mut Vec<char>, cursor: &mut usize) {
        let Some(current) = self.history_cursor else {
            return;
        };
        if current.saturating_add(1) < self.history.len() {
            let next = current.saturating_add(1);
            self.history_cursor = Some(next);
            replace_buffer(buffer, cursor, &self.history[next]);
        } else {
            self.history_cursor = None;
            replace_buffer(buffer, cursor, &self.draft);
        }
    }

    fn reset_history_navigation(&mut self) {
        self.history_cursor = None;
        self.draft.clear();
    }

    fn remember(&mut self, value: &str) -> io::Result<()> {
        let value = value.trim_end().to_owned();
        if self.history.back() == Some(&value) {
            return Ok(());
        }
        self.history.push_back(value.clone());
        while self.history.len() > self.history_limit {
            self.history.pop_front();
        }
        if let Some(parent) = self.history_path.parent() {
            fs::create_dir_all(parent)?;
        }
        if self.history.len() == self.history_limit {
            rewrite_history(&self.history_path, &self.history)?;
        } else {
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.history_path)?;
            serde_json::to_writer(&mut file, &value).map_err(io::Error::other)?;
            file.write_all(b"\n")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct Completion {
    value: String,
    description: String,
}

fn completions(
    buffer: &[char],
    cursor: usize,
    commands: &[SlashCommand],
    workspace: &Workspace,
    limit: usize,
) -> Vec<Completion> {
    let prefix = buffer[..cursor].iter().collect::<String>();
    let token = prefix
        .rsplit_once(char::is_whitespace)
        .map_or(prefix.as_str(), |(_, token)| token);
    if token.starts_with('/') {
        return commands
            .iter()
            .filter(|command| command.name.starts_with(token))
            .take(limit)
            .map(|command| Completion {
                value: command.name.clone(),
                description: command.description.clone(),
            })
            .collect();
    }
    if let Some(query) = token.strip_prefix('@') {
        return resources::file_completions(workspace, query, limit)
            .into_iter()
            .map(|value| Completion {
                description: "workspace file".to_owned(),
                value,
            })
            .collect();
    }
    Vec::new()
}

fn apply_completion(buffer: &mut Vec<char>, cursor: &mut usize, value: &str) {
    let start = buffer[..*cursor]
        .iter()
        .rposition(|character| character.is_whitespace())
        .map_or(0, |index| index.saturating_add(1));
    buffer.splice(start..*cursor, value.chars());
    *cursor = start.saturating_add(value.chars().count());
}

fn starts_completion(buffer: &[char]) -> bool {
    buffer.first().is_some_and(|character| matches!(character, '/' | '@'))
}

fn insert_text(buffer: &mut Vec<char>, cursor: &mut usize, value: &str) {
    for character in value.chars() {
        buffer.insert(*cursor, character);
        *cursor = cursor.saturating_add(1);
    }
}

fn replace_buffer(buffer: &mut Vec<char>, cursor: &mut usize, value: &str) {
    buffer.clear();
    buffer.extend(value.chars());
    *cursor = buffer.len();
}

fn render(
    stdout: &mut impl Write,
    buffer: &[char],
    cursor: usize,
    footer: &str,
    completions: &[Completion],
    completion_index: usize,
    color: bool,
) -> io::Result<()> {
    let width = size().map_or(80usize, |(columns, _)| usize::from(columns).max(20));
    let available = width.saturating_sub(PROMPT.chars().count()).max(8);
    let (visible, cursor_column) = visible_window(buffer, cursor, available);
    queue!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
    if color {
        queue!(stdout, SetForegroundColor(Color::Cyan))?;
    }
    queue!(stdout, Print(PROMPT))?;
    if color {
        queue!(stdout, ResetColor)?;
    }
    queue!(stdout, Print(visible), MoveToNextLine(1), Clear(ClearType::CurrentLine))?;

    let hint = if completions.is_empty() {
        footer.to_owned()
    } else {
        completions
            .iter()
            .enumerate()
            .take(4)
            .map(|(index, completion)| {
                if index == completion_index {
                    format!("[{}] {}", completion.value, completion.description)
                } else {
                    format!("{} {}", completion.value, completion.description)
                }
            })
            .collect::<Vec<_>>()
            .join("  ·  ")
    };
    let hint = truncate(&hint, width.saturating_sub(1));
    if color {
        queue!(stdout, SetForegroundColor(Color::DarkGrey))?;
    }
    queue!(stdout, Print(hint))?;
    if color {
        queue!(stdout, ResetColor)?;
    }
    let column = PROMPT
        .chars()
        .count()
        .saturating_add(cursor_column)
        .min(usize::from(u16::MAX));
    queue!(stdout, MoveUp(1), MoveToColumn(column as u16))?;
    stdout.flush()
}

fn clear_editor(stdout: &mut impl Write, buffer: &[char], color: bool) -> io::Result<()> {
    queue!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
    if color {
        queue!(stdout, SetForegroundColor(Color::Cyan))?;
    }
    queue!(stdout, Print(PROMPT))?;
    if color {
        queue!(stdout, ResetColor)?;
    }
    queue!(
        stdout,
        Print(buffer.iter().collect::<String>().replace('\n', "↵")),
        MoveToNextLine(1),
        Clear(ClearType::CurrentLine),
        Print("\r\n")
    )?;
    stdout.flush()
}

fn visible_window(buffer: &[char], cursor: usize, width: usize) -> (String, usize) {
    let rendered = buffer
        .iter()
        .map(|character| if *character == '\n' { '↵' } else { *character })
        .collect::<Vec<_>>();
    if rendered.len() <= width {
        return (rendered.iter().collect(), cursor);
    }
    let half = width / 2;
    let mut start = cursor.saturating_sub(half);
    let mut end = start.saturating_add(width);
    if end > rendered.len() {
        end = rendered.len();
        start = end.saturating_sub(width);
    }
    let mut visible = rendered[start..end].iter().collect::<String>();
    if start > 0 {
        visible.replace_range(..visible.chars().next().map_or(0, char::len_utf8), "…");
    }
    if end < rendered.len() {
        let last = visible.char_indices().last().map_or(0, |(index, _)| index);
        visible.replace_range(last.., "…");
    }
    (visible, cursor.saturating_sub(start))
}

fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_owned();
    }
    value
        .chars()
        .take(width.saturating_sub(1))
        .chain(std::iter::once('…'))
        .collect()
}

fn load_history(path: &Path, limit: usize) -> io::Result<VecDeque<String>> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(VecDeque::new()),
        Err(error) => return Err(error),
    };
    let mut history = VecDeque::new();
    for line in BufReader::new(file).lines() {
        let line = line?;
        if let Ok(value) = serde_json::from_str::<String>(&line) {
            history.push_back(value);
            while history.len() > limit {
                history.pop_front();
            }
        }
    }
    Ok(history)
}

fn rewrite_history(path: &Path, history: &VecDeque<String>) -> io::Result<()> {
    let temporary = path.with_extension("jsonl.tmp");
    let mut file = fs::File::create(&temporary)?;
    for value in history {
        serde_json::to_writer(&mut file, value).map_err(io::Error::other)?;
        file.write_all(b"\n")?;
    }
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(temporary, path)
}

struct TerminalGuard {
    active: bool,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnableBracketedPaste)?;
        Ok(Self { active: true })
    }

    fn leave(&mut self) -> io::Result<()> {
        if self.active {
            execute!(io::stdout(), DisableBracketedPaste)?;
            disable_raw_mode()?;
            self.active = false;
        }
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = execute!(io::stdout(), DisableBracketedPaste);
            let _ = disable_raw_mode();
        }
    }
}
