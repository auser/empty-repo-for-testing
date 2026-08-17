use std::io::{self, IsTerminal, Write};

use pire_core::{ApprovalPolicy, Operation};

use crate::config::ApprovalMode;

pub struct CliApprovalPolicy {
    mode: ApprovalMode,
    trusted: bool,
    assume_yes: bool,
}

impl CliApprovalPolicy {
    #[must_use]
    pub const fn new(mode: ApprovalMode, trusted: bool, assume_yes: bool) -> Self {
        Self {
            mode,
            trusted,
            assume_yes,
        }
    }
}

impl ApprovalPolicy for CliApprovalPolicy {
    fn approve(&self, operation: Operation, subject: &str, reason: &str) -> bool {
        if operation == Operation::Read {
            return true;
        }
        if !self.trusted {
            return false;
        }
        if self.assume_yes {
            return true;
        }
        match self.mode {
            ApprovalMode::Always => true,
            ApprovalMode::Never => false,
            ApprovalMode::Prompt => prompt(operation, subject, reason),
        }
    }
}

fn prompt(operation: Operation, subject: &str, reason: &str) -> bool {
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        return false;
    }
    let mut stderr = io::stderr().lock();
    if writeln!(
        stderr,
        "Approve {operation:?} operation on `{subject}`? {reason} [y/N]"
    )
    .and_then(|_| stderr.flush())
    .is_err()
    {
        return false;
    }
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .is_ok_and(|_| matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes"))
}
