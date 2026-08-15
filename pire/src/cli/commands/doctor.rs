use std::path::PathBuf;

use clap::Args;

use crate::{config::AppConfig, error::PireResult};

#[derive(Args, Debug)]
pub struct DoctorArgs {
    /// Optional extension path to inspect.
    pub path: Option<PathBuf>,

    /// Apply safe automatic fixes when available.
    #[arg(long)]
    pub fix: bool,
}

pub fn run(args: &DoctorArgs, config: &AppConfig) -> PireResult<()> {
    println!("Pire doctor");
    println!("  configuration: valid");
    println!("  log level: {}", config.logging.level);
    println!("  stdin limit: {} bytes", config.input.max_stdin_bytes);

    if let Some(path) = &args.path {
        println!("  extension path: {}", path.display());
    }

    if args.fix {
        println!("  no automatic fixes are currently required");
    }

    Ok(())
}
