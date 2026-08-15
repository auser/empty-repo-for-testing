use std::process::ExitCode;

use asupersync::runtime::RuntimeBuilder;
use asupersync::runtime::reactor::create_reactor;
use clap::Parser;

use crate::{
    app::run_app,
    cli::{Cli, Commands, commands::doctor},
    config::AppConfig,
    error::{PireResult, print_error},
    http::Client,
    input::{Invocation, read_piped_stdin},
    telemetry,
};

#[must_use]
pub fn run() -> ExitCode {
    #[cfg(windows)]
    if let Err(error) = enable_ansi_support::enable_ansi_support() {
        eprintln!("warning: unable to enable ANSI terminal support: {error}");
    }

    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let exit_code = clap_exit_code(error.exit_code());
            if let Err(print_error) = error.print() {
                eprintln!("{error}");
                eprintln!("unable to print command-line error: {print_error}");
            }
            return ExitCode::from(exit_code);
        }
    };

    let verbose = cli.verbose;
    match run_inner(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            print_error(&error, verbose);
            ExitCode::from(error.exit_code())
        }
    }
}

fn run_inner(cli: Cli) -> PireResult<()> {
    if cli.version {
        println!("pire {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let config = AppConfig::load(cli.config.as_deref(), &cli.overrides)?;
    config.validate()?;
    telemetry::init(&config.logging, cli.verbose)?;

    if let Some(command) = &cli.command {
        return match command {
            Commands::Doctor(args) => doctor::run(args, &config),
        };
    }

    let stdin = read_piped_stdin(config.input.max_stdin_bytes)?;
    let invocation = Invocation::new(cli.print, cli.args, stdin);
    let client = Client::from_config(&config.http_client_config)?;

    let reactor = create_reactor()?;
    let runtime = RuntimeBuilder::multi_thread()
        .blocking_threads(
            config.runtime.blocking_min_threads,
            config.runtime.blocking_max_threads,
        )
        .with_reactor(reactor)
        .build()?;
    let handle = runtime.handle();

    runtime.block_on(run_app(&config, invocation, client, handle))
}

fn clap_exit_code(code: i32) -> u8 {
    u8::try_from(code).unwrap_or(2)
}
