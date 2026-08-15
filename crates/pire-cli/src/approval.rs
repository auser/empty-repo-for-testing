use std::io::{self, IsTerminal, Write};

use pire_core::{ApprovalAction, ApprovalError, ApprovalPolicy, ApprovalRequest};

use crate::config::ApprovalMode;

pub struct CliApprovalPolicy {
    mode: ApprovalMode,
    trusted: bool,
    assume_yes: bool,
    interactive: bool,
}

impl CliApprovalPolicy {
    #[must_use]
    pub fn new(mode: ApprovalMode, trusted: bool, assume_yes: bool) -> Self {
        Self {
            mode,
            trusted,
            assume_yes,
            interactive: io::stdin().is_terminal() && io::stderr().is_terminal(),
        }
    }
}

impl ApprovalPolicy for CliApprovalPolicy {
    fn approve(&mut self, request: &ApprovalRequest) -> Result<bool, ApprovalError> {
        if request.action == ApprovalAction::Read {
            return Ok(true);
        }
        if !self.trusted {
            return Ok(false);
        }
        if self.assume_yes || self.mode == ApprovalMode::Always {
            return Ok(true);
        }
        if self.mode == ApprovalMode::Never || !self.interactive {
            return Ok(false);
        }

        eprint!(
            "Approve {} operation by {}: {} [y/N] ",
            match request.action {
                ApprovalAction::Read => "read",
                ApprovalAction::Write => "write",
                ApprovalAction::Process => "process",
            },
            request.tool,
            request.summary
        );
        io::stderr()
            .flush()
            .map_err(|error| ApprovalError::new(error.to_string()))?;
        let mut response = String::new();
        io::stdin()
            .read_line(&mut response)
            .map_err(|error| ApprovalError::new(error.to_string()))?;
        Ok(matches!(response.trim().to_ascii_lowercase().as_str(), "y" | "yes"))
    }
}
