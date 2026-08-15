use std::io::{self, IsTerminal, Read};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvocationMode {
    Interactive,
    OneShot,
}

#[derive(Debug)]
pub struct Invocation {
    print: bool,
    args: Vec<String>,
    stdin: Option<String>,
}

impl Invocation {
    #[must_use]
    pub fn new(print: bool, args: Vec<String>, stdin: Option<String>) -> Self {
        Self {
            print: print || stdin.is_some(),
            args,
            stdin,
        }
    }

    #[must_use]
    pub fn mode(&self) -> InvocationMode {
        if self.args.is_empty() && self.stdin.is_none() {
            InvocationMode::Interactive
        } else {
            InvocationMode::OneShot
        }
    }

    #[must_use]
    pub const fn print(&self) -> bool {
        self.print
    }

    #[must_use]
    pub fn args(&self) -> &[String] {
        &self.args
    }

    #[must_use]
    pub fn stdin(&self) -> Option<&str> {
        self.stdin.as_deref()
    }

    #[must_use]
    pub fn input_bytes(&self) -> usize {
        let args = self.args.iter().map(String::len).sum::<usize>();
        let stdin = self.stdin.as_ref().map_or(0, String::len);
        args.saturating_add(stdin)
    }
}

pub fn read_piped_stdin(max_bytes: usize) -> io::Result<Option<String>> {
    let stdin = io::stdin();
    if stdin.is_terminal() {
        return Ok(None);
    }

    read_optional_text(stdin.lock(), max_bytes)
}

fn read_optional_text(reader: impl Read, max_bytes: usize) -> io::Result<Option<String>> {
    let limit = u64::try_from(max_bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "stdin limit is too large"))?
        .saturating_add(1);
    let mut limited = reader.take(limit);
    let mut bytes = Vec::new();
    limited.read_to_end(&mut bytes)?;

    if bytes.len() > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("stdin exceeds the maximum size of {max_bytes} bytes"),
        ));
    }

    if bytes.is_empty() {
        return Ok(None);
    }

    String::from_utf8(bytes).map(Some).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("stdin is not valid UTF-8: {error}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, ErrorKind};

    use super::read_optional_text;

    #[test]
    fn preserves_whitespace() {
        let input = Cursor::new(b"  hello\n".to_vec());
        let text = read_optional_text(input, 32)
            .expect("input should be readable")
            .expect("input should be present");
        assert_eq!(text, "  hello\n");
    }

    #[test]
    fn rejects_input_over_the_limit() {
        let input = Cursor::new(b"12345".to_vec());
        let error = read_optional_text(input, 4).expect_err("input should exceed the limit");
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
