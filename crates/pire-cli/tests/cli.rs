use std::{error::Error, fs, process::Command};

#[test]
fn version_works() -> Result<(), Box<dyn Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_pire"))
        .arg("--version")
        .output()?;
    assert!(output.status.success());
    Ok(())
}

#[test]
fn doctor_works_without_a_config_file() -> Result<(), Box<dyn Error>> {
    let workspace = temporary_workspace()?;
    let output = Command::new(env!("CARGO_BIN_EXE_pire"))
        .arg("--workspace")
        .arg(&workspace)
        .arg("doctor")
        .output()?;
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    Ok(())
}

#[test]
fn offline_print_mode_works() -> Result<(), Box<dyn Error>> {
    let workspace = temporary_workspace()?;
    let output = Command::new(env!("CARGO_BIN_EXE_pire"))
        .arg("--workspace")
        .arg(&workspace)
        .arg("--no-session")
        .arg("--print")
        .arg("hello")
        .output()?;
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8(output.stdout)?.contains("Offline provider"));
    Ok(())
}

fn temporary_workspace() -> Result<std::path::PathBuf, std::io::Error> {
    let path = std::env::temp_dir().join(format!(
        "pire-cli-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&path)?;
    Ok(path)
}
