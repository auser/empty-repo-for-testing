use std::{error::Error, process::Command};

use tempfile::tempdir;

#[test]
fn version_short_circuits_with_success() -> Result<(), Box<dyn Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_pire"))
        .arg("--version")
        .output()?;

    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout)?.starts_with("pire "));
    Ok(())
}

#[test]
fn doctor_short_circuits_with_success() -> Result<(), Box<dyn Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_pire"))
        .arg("doctor")
        .output()?;

    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout)?.contains("Pire doctor"));
    Ok(())
}

#[test]
fn missing_explicit_config_uses_usage_exit_code() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let missing = directory.path().join("missing.toml");
    let output = Command::new(env!("CARGO_BIN_EXE_pire"))
        .arg("--config")
        .arg(missing)
        .arg("doctor")
        .output()?;

    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8(output.stderr)?.contains("configuration error"));
    Ok(())
}

#[test]
fn cli_overrides_environment_and_file_configuration() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let config_path = directory.path().join("app.toml");
    std::fs::write(
        &config_path,
        r#"
[logging]
level = "debug"
"#,
    )?;

    let output = Command::new(env!("CARGO_BIN_EXE_pire"))
        .arg("--config")
        .arg(config_path)
        .env("PIRE_LOGGING__LEVEL", "warn")
        .arg("--log-level")
        .arg("error")
        .arg("doctor")
        .output()?;

    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout)?.contains("log level: error"));
    Ok(())
}

#[test]
fn unknown_configuration_fields_are_rejected() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let config_path = directory.path().join("app.toml");
    std::fs::write(
        &config_path,
        r#"
[logging]
level = "info"
unknown = true
"#,
    )?;

    let output = Command::new(env!("CARGO_BIN_EXE_pire"))
        .arg("--config")
        .arg(config_path)
        .arg("doctor")
        .output()?;

    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8(output.stderr)?.contains("configuration error"));
    Ok(())
}
