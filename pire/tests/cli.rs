use std::error::Error;

use clap::Parser;
use pire::{
    cli::{Cli, Commands},
    config::LogLevel,
};

#[test]
fn parses_trailing_message_arguments() -> Result<(), Box<dyn Error>> {
    let cli = Cli::try_parse_from(["pire", "review", "this", "--not-a-pire-flag"])?;

    assert_eq!(
        cli.args,
        vec![
            "review".to_owned(),
            "this".to_owned(),
            "--not-a-pire-flag".to_owned(),
        ]
    );
    assert!(cli.command.is_none());
    Ok(())
}

#[test]
fn parses_sparse_configuration_overrides() -> Result<(), Box<dyn Error>> {
    let cli = Cli::try_parse_from([
        "pire",
        "--log-level",
        "debug",
        "--json-logs=false",
        "--request-timeout",
        "90",
        "message",
    ])?;

    assert_eq!(cli.overrides.logging.level, Some(LogLevel::Debug));
    assert_eq!(cli.overrides.logging.json, Some(false));
    assert_eq!(
        cli.overrides.http_client_config.request_timeout_secs,
        Some(90)
    );
    Ok(())
}

#[test]
fn parses_json_logging_as_a_bare_flag() -> Result<(), Box<dyn Error>> {
    let cli = Cli::try_parse_from(["pire", "--json-logs", "message"])?;
    assert_eq!(cli.overrides.logging.json, Some(true));
    Ok(())
}

#[test]
fn parses_doctor_as_a_subcommand() -> Result<(), Box<dyn Error>> {
    let cli = Cli::try_parse_from(["pire", "doctor"])?;
    assert!(matches!(cli.command, Some(Commands::Doctor(_))));
    Ok(())
}
