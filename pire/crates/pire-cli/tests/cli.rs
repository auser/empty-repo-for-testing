use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static COUNTER: AtomicU64 = AtomicU64::new(1);

#[test]
fn version_works() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let output = fixture.run(["--version"])?;
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).starts_with("pire "));
    Ok(())
}

#[test]
fn doctor_mounts_plugin_capabilities() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let output = fixture.run([
        "--workspace",
        fixture.workspace_text(),
        "doctor",
    ])?;
    assert!(output.status.success(), "{}", stderr(&output));
    let stdout = stdout(&output);
    assert!(stdout.contains("configuration: valid"));
    assert!(stdout.contains("plugins:"));
    assert!(stdout.contains("tools:"));
    Ok(())
}

#[test]
fn offline_one_shot_works_without_credentials() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let output = fixture.run([
        "--workspace",
        fixture.workspace_text(),
        "--no-session",
        "--print",
        "hello",
    ])?;
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("Offline provider"));
    Ok(())
}

#[test]
fn cli_overrides_environment_and_explicit_file() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let explicit = fixture.root.join("explicit.toml");
    fs::write(
        &explicit,
        r#"
[router]
strategy = "cost"
"#,
    )?;
    let output = fixture.run_with_env(
        [
            "--workspace",
            fixture.workspace_text(),
            "--config",
            path_text(&explicit),
            "--route",
            "local-first",
            "config",
            "show",
        ],
        [("PIRE_ROUTER__STRATEGY", "quality")],
    )?;
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("\"strategy\": \"local-first\""));
    Ok(())
}

#[test]
fn project_configuration_is_ignored_until_trusted() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    fs::create_dir_all(fixture.workspace.join(".pire"))?;
    fs::write(
        fixture.workspace.join(".pire/config.toml"),
        r#"
[router]
strategy = "cost"
"#,
    )?;

    let untrusted = fixture.run([
        "--workspace",
        fixture.workspace_text(),
        "config",
        "show",
    ])?;
    assert!(untrusted.status.success(), "{}", stderr(&untrusted));
    assert!(stdout(&untrusted).contains("\"strategy\": \"balanced\""));

    let trusted = fixture.run([
        "--workspace",
        fixture.workspace_text(),
        "--trust-project",
        "config",
        "show",
    ])?;
    assert!(trusted.status.success(), "{}", stderr(&trusted));
    assert!(stdout(&trusted).contains("\"strategy\": \"cost\""));
    Ok(())
}

#[test]
fn project_configuration_cannot_define_models() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    fs::create_dir_all(fixture.workspace.join(".pire"))?;
    fs::write(
        fixture.workspace.join(".pire/config.toml"),
        r#"
[[models]]
id = "malicious"
kind = "open-ai-compatible"
provider_name = "malicious"
model = "malicious"
base_url = "https://example.invalid/v1"
"#,
    )?;
    let output = fixture.run([
        "--workspace",
        fixture.workspace_text(),
        "--trust-project",
        "config",
        "show",
    ])?;
    assert!(!output.status.success());
    assert!(stderr(&output).contains("configuration error"));
    Ok(())
}

#[test]
fn validates_external_plugin_manifest() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let manifest = fixture.root.join("plugin.json");
    fs::write(
        &manifest,
        r#"{
  "protocol": "pire-plugin-v1",
  "id": "example.echo",
  "version": "1.0.0",
  "description": "example",
  "command": ["example-sidecar"],
  "providers": [],
  "tools": [],
  "commands": []
}"#,
    )?;
    let output = fixture.run(["plugin", "validate", path_text(&manifest)])?;
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("example.echo 1.0.0 — valid"));
    Ok(())
}

struct Fixture {
    root: PathBuf,
    workspace: PathBuf,
}

impl Fixture {
    fn new() -> Result<Self, Box<dyn Error>> {
        let root = std::env::temp_dir().join(format!(
            "pire-cli-test-{}-{}-{}",
            std::process::id(),
            now_nanos(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let workspace = root.join("workspace");
        fs::create_dir_all(&workspace)?;
        Ok(Self { root, workspace })
    }

    fn workspace_text(&self) -> &str {
        self.workspace.to_str().unwrap_or("")
    }

    fn run<const N: usize>(&self, args: [&str; N]) -> Result<Output, Box<dyn Error>> {
        self.run_with_env(args, [])
    }

    fn run_with_env<const N: usize, const E: usize>(
        &self,
        args: [&str; N],
        environment: [(&str, &str); E],
    ) -> Result<Output, Box<dyn Error>> {
        let mut command = Command::new(env!("CARGO_BIN_EXE_pire"));
        command.args(args);
        command.env("HOME", &self.root);
        command.env("XDG_CONFIG_HOME", self.root.join("config"));
        command.env("XDG_DATA_HOME", self.root.join("data"));
        command.env("APPDATA", self.root.join("appdata"));
        command.env("LOCALAPPDATA", self.root.join("localappdata"));
        for (key, value) in environment {
            command.env(key, value);
        }
        Ok(command.output()?)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn path_text(path: &Path) -> &str {
    path.to_str().unwrap_or("")
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}
