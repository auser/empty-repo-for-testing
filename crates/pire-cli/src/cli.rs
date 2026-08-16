use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::config::CliOverrides;

#[derive(Debug, Parser)]
#[command(name = "pire", version, about = "A clean Rust coding-agent harness")]
pub struct Cli {
    #[arg(long, short = 'c')]
    pub config: Option<PathBuf>,

    #[arg(long, short = 'w', default_value = ".")]
    pub workspace: PathBuf,

    #[arg(long, short = 'p')]
    pub print: bool,

    // Use an explicit Clap ID because the flattened logging overrides also
    // contain a Rust field named `json` for the `--json-logs` option.
    #[arg(id = "output_json", long = "json")]
    pub json: bool,

    #[arg(long)]
    pub yes: bool,

    #[arg(long)]
    pub trust_project: bool,

    #[arg(long)]
    pub session: Option<String>,

    #[arg(long)]
    pub no_session: bool,

    #[command(flatten)]
    pub overrides: CliOverrides,

    #[command(subcommand)]
    pub command: Option<Commands>,

    #[arg(trailing_var_arg = true, value_name = "MESSAGE")]
    pub args: Vec<String>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Doctor,
    Config(ConfigArgs),
    Models,
    Sessions(SessionsArgs),
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
