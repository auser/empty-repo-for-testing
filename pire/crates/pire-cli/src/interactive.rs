use std::{
    fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
};

use crossterm::{
    cursor::MoveToColumn,
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind,
        KeyModifiers,
    },
    execute,
    style::Print,
    terminal::{self, Clear, ClearType},
};
use walkdir::WalkDir;

pub struct LineEditor {
    history: Vec<String>,
    history_path: PathBuf,
    history_limit: usize,
}

impl LineEditor {
    pub fn new(history_path: PathBuf, history_limit: usize) -> Self {
        let history = fs::read_to_string(&history_path)
            .map(|content| {
                content
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        Self {
            history,
            history_path,
            history_limit,
        }
    }

    pub fn read_line(
        &mut self,
        prompt: &str,
        commands: &[(String, String)],
        workspace: &Path,
    ) -> io::Result<Option<String>> {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            print!("{prompt}");
            io::stdout().flush()?;
            let mut line = String::new();
            if io::stdin().read_line(&mut line)? == 0 {
                return Ok(None);
            }
            return Ok(Some(line.trim_end_matches(['\r', '\n']).to_owned()));
        }

        let _guard = RawModeGuard::enter()?;
        let mut stdout = io::stdout();
        let mut buffer = Vec::<char>::new();
        let mut cursor = 0usize;
        let mut history_index = self.history.len();
        render(&mut stdout, prompt, &buffer, cursor)?;

        loop {
            match event::read()? {
                Event::Paste(text) => {
                    for character in text.chars() {
                        buffer.insert(cursor, character);
                        cursor = cursor.saturating_add(1);
                    }
                }
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Enter
                        if key.modifiers.contains(KeyModifiers::ALT)
                            || key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        buffer.insert(cursor, '\n');
                        cursor = cursor.saturating_add(1);
                    }
                    KeyCode::Enter => {
                        execute!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
                        println!("{prompt}{}", display_buffer(&buffer));
                        let line = buffer.iter().collect::<String>();
                        self.record(&line)?;
                        return Ok(Some(line));
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if buffer.is_empty() {
                            execute!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
                            println!();
                            return Ok(None);
                        }
                        buffer.clear();
                        cursor = 0;
                    }
                    KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if buffer.is_empty() {
                            execute!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
                            println!();
                            return Ok(None);
                        }
                    }
                    KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        cursor = 0;
                    }
                    KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        cursor = buffer.len();
                    }
                    KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        buffer.drain(..cursor);
                        cursor = 0;
                    }
                    KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        buffer.truncate(cursor);
                    }
                    KeyCode::Char(character) => {
                        buffer.insert(cursor, character);
                        cursor = cursor.saturating_add(1);
                    }
                    KeyCode::Backspace => {
                        if cursor > 0 {
                            cursor = cursor.saturating_sub(1);
                            buffer.remove(cursor);
                        }
                    }
                    KeyCode::Delete => {
                        if cursor < buffer.len() {
                            buffer.remove(cursor);
                        }
                    }
                    KeyCode::Left => cursor = cursor.saturating_sub(1),
                    KeyCode::Right => cursor = cursor.saturating_add(1).min(buffer.len()),
                    KeyCode::Home => cursor = 0,
                    KeyCode::End => cursor = buffer.len(),
                    KeyCode::Up => {
                        if history_index > 0 {
                            history_index = history_index.saturating_sub(1);
                            buffer = self.history[history_index].chars().collect();
                            cursor = buffer.len();
                        }
                    }
                    KeyCode::Down => {
                        if history_index.saturating_add(1) < self.history.len() {
                            history_index = history_index.saturating_add(1);
                            buffer = self.history[history_index].chars().collect();
                        } else {
                            history_index = self.history.len();
                            buffer.clear();
                        }
                        cursor = buffer.len();
                    }
                    KeyCode::Tab => {
                        let completions = completions(&buffer, commands, workspace);
                        match completions.as_slice() {
                            [only] => {
                                buffer = only.chars().collect();
                                cursor = buffer.len();
                            }
                            [] => {}
                            many => {
                                execute!(
                                    stdout,
                                    MoveToColumn(0),
                                    Clear(ClearType::CurrentLine),
                                    Print("\r\n")
                                )?;
                                for completion in many.iter().take(20) {
                                    println!("  {completion}");
                                }
                            }
                        }
                    }
                    _ => {}
                },
                Event::Resize(_, _) => {}
                _ => {}
            }
            render(&mut stdout, prompt, &buffer, cursor)?;
        }
    }

    fn record(&mut self, line: &str) -> io::Result<()> {
        if line.trim().is_empty() {
            return Ok(());
        }
        if self.history.last().is_none_or(|last| last != line) {
            self.history.push(line.to_owned());
        }
        if self.history.len() > self.history_limit {
            let remove = self.history.len().saturating_sub(self.history_limit);
            self.history.drain(..remove);
        }
        if let Some(parent) = self.history_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.history_path, self.history.join("\n"))
    }
}

struct RawModeGuard;

impl RawModeGuard {
    fn enter() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        execute!(io::stdout(), EnableBracketedPaste)?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), DisableBracketedPaste);
        let _ = terminal::disable_raw_mode();
    }
}

fn render(
    stdout: &mut io::Stdout,
    prompt: &str,
    buffer: &[char],
    cursor: usize,
) -> io::Result<()> {
    let displayed = display_buffer(buffer);
    execute!(
        stdout,
        MoveToColumn(0),
        Clear(ClearType::CurrentLine),
        Print(prompt),
        Print(&displayed)
    )?;
    let prefix = display_buffer(&buffer[..cursor]);
    let column = prompt.chars().count().saturating_add(prefix.chars().count());
    execute!(stdout, MoveToColumn(u16::try_from(column).unwrap_or(u16::MAX)))?;
    stdout.flush()
}

fn display_buffer(buffer: &[char]) -> String {
    buffer
        .iter()
        .map(|character| if *character == '\n' { '↵' } else { *character })
        .collect()
}

fn completions(
    buffer: &[char],
    commands: &[(String, String)],
    workspace: &Path,
) -> Vec<String> {
    let input = buffer.iter().collect::<String>();
    if let Some(command) = input.strip_prefix('/') {
        let prefix = command.split_whitespace().next().unwrap_or("");
        return commands
            .iter()
            .filter(|(name, _)| name.starts_with(prefix))
            .map(|(name, _)| format!("/{name} "))
            .collect();
    }
    let Some(token) = input.split_whitespace().last() else {
        return Vec::new();
    };
    let Some(prefix) = token.strip_prefix('@') else {
        return Vec::new();
    };
    let before = input.strip_suffix(token).unwrap_or("");
    let mut matches = Vec::new();
    for entry in WalkDir::new(workspace)
        .max_depth(8)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(workspace) else {
            continue;
        };
        let relative = relative.to_string_lossy().replace('\\', "/");
        if relative.starts_with(prefix) {
            matches.push(format!("{before}@{relative}"));
        }
        if matches.len() >= 20 {
            break;
        }
    }
    matches
}
