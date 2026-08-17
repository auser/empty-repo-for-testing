mod app;
mod approval;
mod cli;
mod config;
mod error;
mod interactive;
mod output;
mod resources;
mod trust;

use std::{
    error::Error as StdError,
    hash::{Hash, Hasher},
    io::{self, IsTerminal, Read},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
};

use clap::Parser;
use pire_builtins::{
    BuiltinToolsPlugin, BuiltinToolsPolicy, CommandProviderPlugin, CommandsPlugin,
    CostAwareRouterPlugin, FileResourcePlugin, HostExecutionPlugin, JsonLearningPlugin,
    JsonlSessionPlugin, OfflineProviderPlugin, ProviderProfile, SidecarPlugin,
};
#[cfg(feature = "http")]
use pire_builtins::OpenAiCompatibleProviderPlugin;
use pire_core::{Kernel, ModelDescriptor, Plugin, Registry, SessionStore};
use tracing_subscriber::EnvFilter;

use crate::{
    app::App,
    cli::{Cli, Commands, ConfigCommand, PluginCommand, SessionsCommand, TrustCommand},
    config::{AppConfig, ModelConfig, ProviderKind, user_config_path, user_data_directory},
    error::{PireError, Result},
    interactive::LineEditor,
    trust::TrustStore,
};

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let code = u8::try_from(error.exit_code()).unwrap_or(2);
            let _ = error.print();
            return ExitCode::from(code);
        }
    };
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            let mut source = error.source();
            while let Some(cause) = source {
                eprintln!("  caused by: {cause}");
                source = cause.source();
            }
            ExitCode::from(error.exit_code())
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let workspace = canonical_workspace(&cli.workspace)?;
    let data_directory = user_data_directory().unwrap_or_else(|| workspace.join(".pire-data"));
    let trust_path = data_directory.join("trust.json");
    let mut trust_store = TrustStore::open(trust_path).map_err(PireError::Message)?;

    if let Some(Commands::Trust(args)) = &cli.command {
        return handle_trust_command(&mut trust_store, &workspace, &args.command);
    }

    let trusted = cli.trust_project || trust_store.is_trusted(&workspace);
    let config = AppConfig::load(
        &workspace,
        trusted,
        cli.config.as_deref(),
        &cli.overrides,
    )?;
    config.validate()?;
    init_tracing(&config)?;

    if let Some(Commands::Config(args)) = &cli.command {
        return handle_config_command(&config, &workspace, &args.command);
    }
    if let Some(Commands::Plugin(args)) = &cli.command {
        return handle_plugin_command(&args.command);
    }

    let session_directory = config
        .sessions
        .directory
        .clone()
        .unwrap_or_else(|| data_directory.join("sessions").join(workspace_key(&workspace)));
    let learning_path = config
        .learning
        .path
        .clone()
        .unwrap_or_else(|| data_directory.join("routing-learning.json"));

    let mut kernel = Kernel::new();
    add(&mut kernel, HostExecutionPlugin)?;
    add(&mut kernel, CostAwareRouterPlugin)?;
    add(
        &mut kernel,
        JsonLearningPlugin::new(learning_path, config.learning.enabled),
    )?;
    if config.sessions.enabled {
        add(&mut kernel, JsonlSessionPlugin::new(session_directory))?;
    }
    add(&mut kernel, CommandsPlugin)?;
    add(&mut kernel, FileResourcePlugin::new(workspace.clone()))?;
    add(
        &mut kernel,
        BuiltinToolsPlugin::new(BuiltinToolsPolicy {
            allow_read: config.tools.allow_read,
            allow_write: config.tools.allow_write,
            allow_process: config.tools.allow_process,
        }),
    )?;
    for model in config.models.iter().filter(|model| model.enabled) {
        add_model_plugin(&mut kernel, model)?;
    }
    for plugin in resources::prompt_plugins(&workspace, trusted)? {
        kernel.add(plugin)?;
    }
    for plugin in resources::skill_plugins(&workspace, trusted)? {
        kernel.add(plugin)?;
    }
    for plugin in resources::sidecar_plugins(&workspace, trusted, &config.plugins)? {
        kernel.add(plugin)?;
    }

    let registry = Arc::new(kernel.mount()?);

    if let Some(command) = &cli.command {
        return handle_management_command(command, &config, registry.as_ref());
    }

    let session_store = if config.sessions.enabled {
        registry.session("default")
    } else {
        None
    };
    let session = prepare_session(
        session_store.as_ref(),
        cli.session.as_deref(),
        cli.no_session,
    )?;
    let system_prompt = resources::system_prompt(&workspace, trusted, &config.resources)?;
    let history_path = data_directory.join("history.txt");
    let history_limit = config.ui.history_limit;
    let max_input_bytes = config.resources.max_total_bytes;
    let mut application = App::new(
        config,
        workspace.clone(),
        trusted,
        cli.yes,
        cli.json_events,
        Arc::clone(&registry),
        system_prompt,
        session_store,
        session,
        trust_store,
    );

    let stdin = read_piped_stdin(max_input_bytes)?;
    let has_request = !cli.args.is_empty() || stdin.is_some();
    if cli.print || has_request {
        let input = resources::expand_input(
            registry.as_ref(),
            &cli.args,
            stdin,
            max_input_bytes,
        )?;
        if input.trim().is_empty() {
            return Err(PireError::Message("request input is empty".to_owned()));
        }
        let _ = application.submit(input)?;
        return Ok(());
    }

    println!("Pire {} — type /help for commands", env!("CARGO_PKG_VERSION"));
    let mut editor = LineEditor::new(history_path, history_limit);
    loop {
        let prompt = application.prompt();
        let commands = application.commands();
        let Some(line) = editor.read_line(&prompt, &commands, &workspace)? else {
            break;
        };
        match application.handle_line(&line) {
            Ok(true) => {}
            Ok(false) => break,
            Err(error) => eprintln!("error: {error}"),
        }
    }
    Ok(())
}

fn add(kernel: &mut Kernel, plugin: impl Plugin + 'static) -> Result<()> {
    kernel.add(Box::new(plugin))?;
    Ok(())
}

fn add_model_plugin(kernel: &mut Kernel, model: &ModelConfig) -> Result<()> {
    let profile = ProviderProfile {
        id: model.id.clone(),
        provider_name: model.provider_name.clone(),
        model: model.model.clone(),
        base_url: model.base_url.clone(),
        api_key_env: model.api_key_env.clone(),
        command: model.command.clone(),
        local: model.local,
        priority: model.priority,
        tags: model.tags.clone(),
        input_cost_per_million: model.input_cost_per_million,
        output_cost_per_million: model.output_cost_per_million,
        context_window: model.context_window,
        capabilities: model.capabilities.clone(),
        timeout_secs: model.timeout_secs,
        max_response_bytes: model.max_response_bytes,
    };
    match model.kind {
        ProviderKind::Offline => add(kernel, OfflineProviderPlugin::new(profile)),
        ProviderKind::Command => add(kernel, CommandProviderPlugin::new(profile)),
        ProviderKind::OpenAiCompatible => {
            #[cfg(feature = "http")]
            {
                add(kernel, OpenAiCompatibleProviderPlugin::new(profile))
            }
            #[cfg(not(feature = "http"))]
            {
                let _ = profile;
                Err(PireError::Message(
                    "HTTP provider support was disabled at compile time".to_owned(),
                ))
            }
        }
    }
}

fn handle_management_command(
    command: &Commands,
    config: &AppConfig,
    registry: &Registry,
) -> Result<()> {
    match command {
        Commands::Doctor => {
            println!("configuration: valid");
            println!("models: {}", registry.providers().len());
            println!("plugins: {}", registry.plugins().len());
            println!("tools: {}", registry.tools().len());
            println!("commands: {}", registry.commands().len());
            println!("process tool enabled: {}", config.tools.allow_process);
        }
        Commands::Models => {
            let mut models = registry
                .providers()
                .into_iter()
                .map(|provider| provider.descriptor().clone())
                .collect::<Vec<ModelDescriptor>>();
            models.sort_by(|left, right| left.id.cmp(&right.id));
            for model in models {
                println!(
                    "{} provider={} local={} tools={}",
                    model.id, model.provider_name, model.local, model.capabilities.tools
                );
            }
        }
        Commands::Plugins => {
            for plugin in registry.plugins() {
                println!("{} {} — {}", plugin.id, plugin.version, plugin.description);
            }
            for capability in registry.capabilities() {
                println!(
                    "  {:?} {} <- {}",
                    capability.kind, capability.id, capability.plugin_id
                );
            }
        }
        Commands::Sessions(args) => {
            let store = registry
                .session("default")
                .ok_or_else(|| PireError::Message("sessions are disabled".to_owned()))?;
            match &args.command {
                SessionsCommand::List => {
                    for session in store.list().map_err(PireError::Message)? {
                        println!("{} {}", session.id, session.name.as_deref().unwrap_or(""));
                    }
                }
                SessionsCommand::Show { id } => {
                    for record in store.load(id).map_err(PireError::Message)? {
                        println!(
                            "{}",
                            serde_json::to_string(&record)
                                .map_err(|error| PireError::Message(error.to_string()))?
                        );
                    }
                }
                SessionsCommand::Fork { id, name } => {
                    let session = store
                        .fork(id, name.as_deref())
                        .map_err(PireError::Message)?;
                    println!("{}", session.id);
                }
            }
        }
        Commands::Config(_) | Commands::Trust(_) | Commands::Plugin(_) => {}
    }
    Ok(())
}

fn handle_config_command(config: &AppConfig, workspace: &Path, command: &ConfigCommand) -> Result<()> {
    match command {
        ConfigCommand::Show => println!(
            "{}",
            serde_json::to_string_pretty(config)
                .map_err(|error| PireError::Message(error.to_string()))?
        ),
        ConfigCommand::Paths => {
            println!(
                "user: {}",
                user_config_path()
                    .map_or_else(|| "unavailable".to_owned(), |path| path.display().to_string())
            );
            println!("project: {}", workspace.join(".pire/config.toml").display());
        }
    }
    Ok(())
}

fn handle_trust_command(
    store: &mut TrustStore,
    workspace: &Path,
    command: &TrustCommand,
) -> Result<()> {
    match command {
        TrustCommand::Status => println!("trusted: {}", store.is_trusted(workspace)),
        TrustCommand::Grant => {
            store.grant(workspace).map_err(PireError::Message)?;
            println!("trusted: true");
        }
        TrustCommand::Revoke => {
            store.revoke(workspace).map_err(PireError::Message)?;
            println!("trusted: false");
        }
    }
    Ok(())
}

fn handle_plugin_command(command: &PluginCommand) -> Result<()> {
    match command {
        PluginCommand::Validate { manifest } => {
            let bytes = std::fs::read(manifest)?;
            let plugin = SidecarPlugin::from_json(&bytes).map_err(PireError::Message)?;
            let metadata = plugin.metadata();
            println!("{} {} — valid", metadata.id, metadata.version);
        }
    }
    Ok(())
}

fn prepare_session(
    store: Option<&Arc<dyn SessionStore>>,
    requested: Option<&str>,
    disabled: bool,
) -> Result<Option<pire_core::SessionSummary>> {
    if disabled {
        return Ok(None);
    }
    let Some(store) = store else {
        return Ok(None);
    };
    if let Some(id) = requested {
        let session = store
            .list()
            .map_err(PireError::Message)?
            .into_iter()
            .find(|session| session.id == id || session.id.starts_with(id))
            .ok_or_else(|| PireError::Message(format!("session `{id}` was not found")))?;
        return Ok(Some(session));
    }
    store.create(None, None).map(Some).map_err(PireError::Message)
}

fn canonical_workspace(path: &Path) -> Result<PathBuf> {
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }
    path.canonicalize().map_err(PireError::Io)
}

fn read_piped_stdin(max_bytes: usize) -> io::Result<Option<String>> {
    if io::stdin().is_terminal() {
        return Ok(None);
    }
    let limit = u64::try_from(max_bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "stdin limit is too large"))?
        .saturating_add(1);
    let mut bytes = Vec::new();
    io::stdin().lock().take(limit).read_to_end(&mut bytes)?;
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

fn workspace_key(path: &Path) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn init_tracing(config: &AppConfig) -> Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(config.logging.level.as_str()));
    if config.logging.json {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .json()
            .try_init()
            .map_err(|error| PireError::Message(error.to_string()))
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .try_init()
            .map_err(|error| PireError::Message(error.to_string()))
    }
}
