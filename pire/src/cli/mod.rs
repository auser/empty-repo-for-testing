use std::path::PathBuf;

use clap::Parser;

use crate::config::CliOverrides;

pub mod commands;
pub use commands::Commands;

#[derive(Parser, Debug)]
#[command(name = "pire")]
#[command(
    version,
    about,
    long_about = None,
    disable_version_flag = true
)]
pub struct Cli {
    /// Show version information.
    #[arg(short = 'v', long)]
    pub version: bool,

    /// Non-interactive mode (process input, print the response, then exit).
    #[arg(short = 'p', long)]
    pub print: bool,

    /// Include error source chains in diagnostics.
    #[arg(long)]
    pub verbose: bool,

    /// Load configuration from this TOML file.
    #[arg(long, short = 'c', value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Values that explicitly override file and environment configuration.
    #[command(flatten)]
    pub overrides: CliOverrides,

    /// Optional subcommand.
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Messages and `@file` references consumed by Pire's input parser.
    #[arg(trailing_var_arg = true, value_name = "MESSAGE")]
    pub args: Vec<String>,
}
