mod approval;
mod builtins;
mod cli;
mod config;
mod error;
mod output;
mod resources;
mod trust;

use std::{
    collections::hash_map::DefaultHasher,
    env,
    hash::{Hash, Hasher},
    io::{self, IsTerminal, Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    time::Duration,
};

use approval::CliApprovalPolicy;
use clap::Parser;
use cli::{Cli, Commands, ConfigCommand, SessionsCommand, TrustCommand};
use config::{
    AppConfig, ProviderKind, user_config_path, user_data_directory,
};
use error::PireError;
use output::CliObserver;
use pire_core::{
    Agent, AgentConfig as CoreAgentConfig, AgentRunRequest, Message, SessionStore, SessionWriter,
    ToolLimits, Workspace,
};
use pire_providers::{ProviderSpec, build_provider};
use tracing_subscriber::EnvFilter;
use trust::TrustStore;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(error.exit_code())
        }
    }
}

fn run() -> Result<(), PireError> {
    let cli = Cli::parse();
    let workspace = Workspace::open(&cli.workspace)?;
    let config = AppConfig::load(workspace.root(), cli.config.as_deref(), &cli.overrides)?;
    config.validate()?;
    init_tracing(&config)?;

    let data_directory = user_data_directory().unwrap_or_else(|| workspace.root().join(".pire/state"));
    let mut trust_store = TrustStore::load(data_directory.join("trusted-projects.json"))?;
    if cli.trust_project {
        trust_store.grant(workspace.root())?;
    }
    let trusted = !config.security.require_project_trust || trust_store.contains(workspace.root());
    let session_store = SessionStore::open(session_directory(&config, &data_directory, workspace.root()))?;

    if let Some(command) = &cli.command {
        return handle_command(
            command,
            &cli,
            &config,
            &workspace,
            &session_store,
            &mut trust_store,
            trusted,
        );
    }

    let provider = build_provider(provider_spec(&config, workspace.root())?)?;
    let tools = builtins::registry(&config.tools)?;
    let agent = Agent::new(
        provider,
        tools,
        CoreAgentConfig {
            max_steps: config.agent.max_steps,
            max_tool_calls: config.agent.max_tool_calls,
        },
        ToolLimits {
            max_read_bytes: config.tools.max_read_bytes,
            max_write_bytes: config.tools.max_write_bytes,
            max_process_output_bytes: config.tools.max_process_output_bytes,
            process_timeout_seconds: config.tools.process_timeout_secs,
            max_list_entries: config.tools.max_list_entries,
        },
    );
    let system_prompt = resources::system_prompt(&workspace, &config.resources)?;
    let (mut history, session) = prepare_session(&cli, &config, &session_store, workspace.root())?;
    let mut observer = CliObserver::new(cli.json, session);
    let mut approval = CliApprovalPolicy::new(config.security.approval_mode, trusted, cli.yes);
    let stdin = read_piped_stdin(config.resources.max_total_bytes)?;
    let input = resources::input(
        &workspace,
        &cli.args,
        stdin,
        config.resources.max_total_bytes,
    )?;

    if input.is_empty() && !cli.print {
        interactive(
            &agent,
            &workspace,
            &mut approval,
            &mut observer,
            &system_prompt,
            &config.provider.model,
            &mut history,
        )
    } else {
        if input.is_empty() {
            return Err(PireError::Message(
                "print mode requires a message, @file reference, or piped input".to_owned(),
            ));
        }
        let request = AgentRunRequest {
            model: config.provider.model.clone(),
            system_prompt,
            input,
            history,
        };
        let _result = agent.run(request, &workspace, &mut approval, &mut observer)?;
        Ok(())
    }
}

fn interactive(
    agent: &Agent,
    workspace: &Workspace,
    approval: &mut CliApprovalPolicy,
    observer: &mut CliObserver,
    system_prompt: &str,
    model: &str,
    history: &mut Vec<Message>,
) -> Result<(), PireError> {
    println!("Pire interactive mode. Type /exit to quit.");
    loop {
        print!("pire> ");
        io::stdout().flush()?;
        let mut line = String::new();
        if io::stdin().read_line(&mut line)? == 0 {
            break;
        }
        let input = line.trim();
        if input.is_empty() {
            continue;
        }
        if matches!(input, "/exit" | "/quit") {
            break;
        }
        let result = agent.run(
            AgentRunRequest {
                model: model.to_owned(),
                system_prompt: system_prompt.to_owned(),
                input: input.to_owned(),
                history: history.clone(),
            },
            workspace,
            approval,
            observer,
        )?;
        *history = result.messages;
    }
    Ok(())
}

fn handle_command(
    command: &Commands,
    cli: &Cli,
    config: &AppConfig,
    workspace: &Workspace,
    sessions: &SessionStore,
    trust_store: &mut TrustStore,
    trusted: bool,
) -> Result<(), PireError> {
    match command {
        Commands::Doctor => {
            println!("Pire doctor");
            println!("  workspace: {}", workspace.root().display());
            println!("  configuration: valid");
            println!("  provider: {}", config.provider.kind);
            println!("  model: {}", config.provider.model);
            println!("  project trusted: {trusted}");
            println!("  sessions enabled: {}", config.sessions.enabled);
            println!("  local endpoint support: yes");
            println!("  local command support: yes");
        }
        Commands::Config(args) => match args.command {
            ConfigCommand::Show => println!("{}", serde_json::to_string_pretty(config)?),
            ConfigCommand::Paths => {
                println!(
                    "user: {}",
                    user_config_path()
                        .map_or_else(|| "unavailable".to_owned(), |path| path.display().to_string())
                );
                println!("workspace: {}", workspace.root().join(".pire/config.toml").display());
                if let Some(path) = &cli.config {
                    println!("explicit: {}", path.display());
                }
            }
        },
        Commands::Models => {
            let provider = build_provider(provider_spec(config, workspace.root())?)?;
            let models = provider.list_models()?;
            if models.is_empty() {
                println!("No models were reported by {}", provider.name());
            } else {
                for model in models {
                    println!("{model}");
                }
            }
        }
        Commands::Sessions(args) => match &args.command {
            SessionsCommand::List => {
                for session in sessions.list()? {
                    println!("{}\t{}\t{}", session.id, session.created_unix_ms, session.workspace);
                }
            }
            SessionsCommand::Show { id } => {
                let session = sessions.load(id)?;
                println!("{}", serde_json::to_string_pretty(&session.metadata)?);
                for message in session.messages {
                    println!("{:?}: {}", message.role, message.content);
                }
            }
            SessionsCommand::Fork { id } => {
                let session = sessions.load(id)?;
                let mut writer = sessions.create(workspace.root(), Some(id.clone()))?;
                for message in &session.messages {
                    writer.append_message(message)?;
                }
                println!("{}", writer.metadata().id);
            }
        },
        Commands::Trust(args) => match args.command {
            TrustCommand::Status => println!("{trusted}"),
            TrustCommand::Grant => {
                trust_store.grant(workspace.root())?;
                println!("trusted {}", workspace.root().display());
            }
            TrustCommand::Revoke => {
                trust_store.revoke(workspace.root())?;
                println!("revoked {}", workspace.root().display());
            }
        },
    }
    Ok(())
}

fn provider_spec(config: &AppConfig, workspace: &Path) -> Result<ProviderSpec, PireError> {
    let provider = &config.provider;
    let timeout = Duration::from_secs(provider.request_timeout_secs);
    match provider.kind {
        ProviderKind::Offline => Ok(ProviderSpec::Offline),
        ProviderKind::OpenAi => {
            let variable = provider.api_key_env.as_deref().unwrap_or("OPENAI_API_KEY");
            let api_key = env::var(variable).map_err(|_| {
                PireError::Message(format!("required provider credential is missing: {variable}"))
            })?;
            Ok(ProviderSpec::OpenAi {
                model: provider.model.clone(),
                api_key,
                timeout,
                max_response_bytes: provider.max_response_bytes,
            })
        }
        ProviderKind::OpenAiCompatible => {
            let api_key = match provider.api_key_env.as_deref() {
                Some(variable) => Some(env::var(variable).map_err(|_| {
                    PireError::Message(format!("provider credential is missing: {variable}"))
                })?),
                None => None,
            };
            Ok(ProviderSpec::OpenAiCompatible {
                name: "open-ai-compatible".to_owned(),
                base_url: provider.base_url.clone(),
                api_key,
                timeout,
                max_response_bytes: provider.max_response_bytes,
            })
        }
        ProviderKind::Command => Ok(ProviderSpec::Command {
            command: provider
                .command
                .clone()
                .ok_or_else(|| PireError::Message("provider.command is required".to_owned()))?,
            timeout,
            max_output_bytes: provider.max_response_bytes,
            current_dir: workspace.to_path_buf(),
        }),
    }
}

fn prepare_session(
    cli: &Cli,
    config: &AppConfig,
    store: &SessionStore,
    workspace: &Path,
) -> Result<(Vec<Message>, Option<SessionWriter>), PireError> {
    if cli.no_session || !config.sessions.enabled {
        return Ok((Vec::new(), None));
    }
    if let Some(id) = &cli.session {
        let data = store.load(id)?;
        let writer = store.append(id)?;
        return Ok((data.messages, Some(writer)));
    }
    let writer = store.create(workspace, None)?;
    tracing::debug!(session_id = writer.metadata().id, "session created");
    Ok((Vec::new(), Some(writer)))
}

fn session_directory(config: &AppConfig, data_directory: &Path, workspace: &Path) -> PathBuf {
    if let Some(directory) = &config.sessions.directory {
        return directory.clone();
    }
    let mut hasher = DefaultHasher::new();
    workspace.hash(&mut hasher);
    data_directory.join("sessions").join(format!("{:016x}", hasher.finish()))
}

fn read_piped_stdin(max_bytes: usize) -> Result<Option<String>, io::Error> {
    let stdin = io::stdin();
    if stdin.is_terminal() {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    stdin
        .lock()
        .take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("stdin exceeds {max_bytes} bytes"),
        ));
    }
    if bytes.is_empty() {
        return Ok(None);
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn init_tracing(config: &AppConfig) -> Result<(), PireError> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(config.logging.level.as_str()));
    if config.logging.json {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .json()
            .try_init()
            .map_err(|error| PireError::Message(format!("tracing initialization failed: {error}")))
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .try_init()
            .map_err(|error| PireError::Message(format!("tracing initialization failed: {error}")))
    }
}
