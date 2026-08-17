use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::config::CliOverrides;

#[derive(Debug, Parser)]
#[command(name = "pire", version, about = "A small plugin-first Rust coding-agent harness")]
pub struct Cli {
    /// Explicit configuration file.
    #[arg(long, short = 'c', value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Workspace root.
    #[arg(long, short = 'w', default_value = ".", value_name = "DIRECTORY")]
    pub workspace: PathBuf,

    /// Process one request, print the answer, and exit.
    #[arg(long, short = 'p')]
    pub print: bool,

    /// Emit typed agent events as JSON Lines.
    #[arg(long = "json", id = "json_events")]
    pub json_events: bool,

    /// Approve write and process operations non-interactively.
    #[arg(long)]
    pub yes: bool,

    /// Trust this project for the current invocation.
    #[arg(long)]
    pub trust_project: bool,

    /// Resume this session ID.
    #[arg(long)]
    pub session: Option<String>,

    /// Disable persistent sessions for this invocation.
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

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Diagnose configuration, trust, plugins, providers, and storage.
    Doctor,
    /// Inspect resolved configuration.
    Config(ConfigArgs),
    /// List configured models.
    Models,
    /// List mounted plugins and capabilities.
    Plugins,
    /// Manage append-only sessions.
    Sessions(SessionsArgs),
    /// Manage project trust.
    Trust(TrustArgs),
    /// Validate an external sidecar plugin manifest.
    Plugin(PluginArgs),
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
    Fork {
        id: String,
        #[arg(long)]
        name: Option<String>,
    },
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

#[derive(Debug, Args)]
pub struct PluginArgs {
    #[command(subcommand)]
    pub command: PluginCommand,
}

#[derive(Debug, Subcommand)]
pub enum PluginCommand {
    Validate { manifest: PathBuf },
}
