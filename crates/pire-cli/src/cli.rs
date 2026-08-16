use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::config::CliOverrides;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "lower")]
pub enum RunMode {
    Interactive,
    Print,
    Json,
}

#[derive(Debug, Parser)]
#[command(name = "pire", version, about = "A small, extensible Rust coding-agent harness")]
pub struct Cli {
    /// Explicit configuration file. Loaded after user and project config.
    #[arg(long, short = 'c')]
    pub config: Option<PathBuf>,

    /// Workspace the agent may inspect and modify.
    #[arg(long, short = 'w', default_value = ".")]
    pub workspace: PathBuf,

    /// Run mode. Interactive is selected automatically when no one-shot input exists.
    #[arg(long, value_enum)]
    pub mode: Option<RunMode>,

    /// Print one response and exit.
    #[arg(long, short = 'p')]
    pub print: bool,

    /// Emit agent events as JSON lines.
    #[arg(id = "output_json", long = "json")]
    pub json: bool,

    /// Approve trusted write/process operations without prompting.
    #[arg(long, short = 'y')]
    pub yes: bool,

    /// Trust this project for the current run and remember the decision.
    #[arg(long = "approve", visible_alias = "trust-project")]
    pub trust_project: bool,

    /// Ignore project-local executable resources for this run.
    #[arg(long = "no-approve")]
    pub no_trust_project: bool,

    /// Continue the most recent session for this workspace.
    #[arg(long, short = 'C')]
    pub continue_session: bool,

    /// Resume a specific session ID or prefix.
    #[arg(long)]
    pub session: Option<String>,

    /// Fork a specific session ID or prefix.
    #[arg(long)]
    pub fork: Option<String>,

    /// Human-readable session name.
    #[arg(long, short = 'n')]
    pub name: Option<String>,

    /// Do not save session state.
    #[arg(long)]
    pub no_session: bool,

    #[command(flatten)]
    pub overrides: CliOverrides,

    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Message text and @file references.
    #[arg(trailing_var_arg = true, value_name = "MESSAGE")]
    pub args: Vec<String>,
}

impl Cli {
    #[must_use]
    pub fn run_mode(&self, has_input: bool) -> RunMode {
        if let Some(mode) = self.mode {
            return mode;
        }
        if self.json {
            return RunMode::Json;
        }
        if self.print || has_input {
            return RunMode::Print;
        }
        RunMode::Interactive
    }
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Diagnose configuration, providers, trust, and storage.
    Doctor,
    /// Inspect resolved configuration.
    Config(ConfigArgs),
    /// List configured and discoverable models.
    Models {
        /// Optional case-insensitive filter.
        search: Option<String>,
    },
    /// Manage sessions.
    Sessions(SessionsArgs),
    /// Manage project trust.
    Trust(TrustArgs),
}

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    Show,
    Paths,
}

#[derive(Debug, Args)]
pub struct SessionsArgs {
    #[command(subcommand)]
    pub command: SessionsCommand,
}

#[derive(Debug, Subcommand)]
pub enum SessionsCommand {
    List,
    Show { id: String },
    Fork { id: String },
}

#[derive(Debug, Args)]
pub struct TrustArgs {
    #[command(subcommand)]
    pub command: TrustCommand,
}

#[derive(Debug, Subcommand)]
pub enum TrustCommand {
    Status,
    Grant,
    Revoke,
}
