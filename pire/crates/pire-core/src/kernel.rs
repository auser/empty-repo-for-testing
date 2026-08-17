use std::{collections::BTreeMap, sync::Arc};

use thiserror::Error;

use crate::{
    CapabilityDescriptor, CapabilityKind, ExecutionBackend, LearningStore, Provider,
    ResourceResolver, Router, SessionStore, SlashCommand, Tool,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginMetadata {
    pub id: String,
    pub version: String,
    pub description: String,
    pub dependencies: Vec<String>,
}

impl PluginMetadata {
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        version: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            version: version.into(),
            description: description.into(),
            dependencies: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_dependencies(mut self, dependencies: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.dependencies = dependencies.into_iter().map(Into::into).collect();
        self
    }
}

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("plugin `{plugin}` failed to mount: {message}")]
    Mount { plugin: String, message: String },
    #[error("plugin dependency graph is unresolved: {0}")]
    DependencyGraph(String),
    #[error("plugin `{0}` was registered more than once")]
    DuplicatePlugin(String),
}

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("{kind:?} capability `{id}` was registered more than once")]
    DuplicateCapability { kind: CapabilityKind, id: String },
}

pub trait Plugin: Send {
    fn metadata(&self) -> PluginMetadata;
    fn mount(&mut self, registry: &mut Registry) -> Result<(), String>;
}

#[derive(Default)]
pub struct Registry {
    plugins: BTreeMap<String, PluginMetadata>,
    providers: BTreeMap<String, Arc<dyn Provider>>,
    tools: BTreeMap<String, Arc<dyn Tool>>,
    commands: BTreeMap<String, Arc<dyn SlashCommand>>,
    routers: BTreeMap<String, Arc<dyn Router>>,
    learning: BTreeMap<String, Arc<dyn LearningStore>>,
    sessions: BTreeMap<String, Arc<dyn SessionStore>>,
    resources: BTreeMap<String, Arc<dyn ResourceResolver>>,
    execution: BTreeMap<String, Arc<dyn ExecutionBackend>>,
    capabilities: Vec<CapabilityDescriptor>,
    mounting_plugin: Option<String>,
}

impl Registry {
    fn current_plugin(&self) -> String {
        self.mounting_plugin
            .clone()
            .unwrap_or_else(|| "host".to_owned())
    }

    fn descriptor(
        &self,
        kind: CapabilityKind,
        id: &str,
        description: &str,
    ) -> CapabilityDescriptor {
        CapabilityDescriptor {
            kind,
            id: id.to_owned(),
            plugin_id: self.current_plugin(),
            description: description.to_owned(),
        }
    }

    pub fn register_provider(
        &mut self,
        provider: Arc<dyn Provider>,
    ) -> Result<(), RegistryError> {
        let id = provider.descriptor().provider_id.clone();
        if self.providers.contains_key(&id) {
            return Err(RegistryError::DuplicateCapability {
                kind: CapabilityKind::Provider,
                id,
            });
        }
        self.capabilities.push(self.descriptor(
            CapabilityKind::Provider,
            &id,
            &format!("model {}", provider.descriptor().id),
        ));
        self.providers.insert(id, provider);
        Ok(())
    }

    pub fn register_tool(&mut self, tool: Arc<dyn Tool>) -> Result<(), RegistryError> {
        let definition = tool.definition();
        let id = definition.name;
        if self.tools.contains_key(&id) {
            return Err(RegistryError::DuplicateCapability {
                kind: CapabilityKind::Tool,
                id,
            });
        }
        self.capabilities.push(self.descriptor(
            CapabilityKind::Tool,
            &id,
            &definition.description,
        ));
        self.tools.insert(id, tool);
        Ok(())
    }

    pub fn register_command(
        &mut self,
        command: Arc<dyn SlashCommand>,
    ) -> Result<(), RegistryError> {
        let id = command.name().trim_start_matches('/').to_owned();
        if self.commands.contains_key(&id) {
            return Err(RegistryError::DuplicateCapability {
                kind: CapabilityKind::SlashCommand,
                id,
            });
        }
        self.capabilities.push(self.descriptor(
            CapabilityKind::SlashCommand,
            &id,
            command.description(),
        ));
        self.commands.insert(id, command);
        Ok(())
    }

    pub fn register_router(&mut self, router: Arc<dyn Router>) -> Result<(), RegistryError> {
        let id = router.id().to_owned();
        if self.routers.contains_key(&id) {
            return Err(RegistryError::DuplicateCapability {
                kind: CapabilityKind::Router,
                id,
            });
        }
        self.capabilities.push(self.descriptor(
            CapabilityKind::Router,
            &id,
            "model and provider router",
        ));
        self.routers.insert(id, router);
        Ok(())
    }

    pub fn register_learning(
        &mut self,
        learning: Arc<dyn LearningStore>,
    ) -> Result<(), RegistryError> {
        let id = learning.id().to_owned();
        if self.learning.contains_key(&id) {
            return Err(RegistryError::DuplicateCapability {
                kind: CapabilityKind::Learning,
                id,
            });
        }
        self.capabilities.push(self.descriptor(
            CapabilityKind::Learning,
            &id,
            "aggregate routing learning store",
        ));
        self.learning.insert(id, learning);
        Ok(())
    }

    pub fn register_session(
        &mut self,
        session: Arc<dyn SessionStore>,
    ) -> Result<(), RegistryError> {
        let id = session.id().to_owned();
        if self.sessions.contains_key(&id) {
            return Err(RegistryError::DuplicateCapability {
                kind: CapabilityKind::Session,
                id,
            });
        }
        self.capabilities.push(self.descriptor(
            CapabilityKind::Session,
            &id,
            "append-only session store",
        ));
        self.sessions.insert(id, session);
        Ok(())
    }

    pub fn register_resource(
        &mut self,
        resource: Arc<dyn ResourceResolver>,
    ) -> Result<(), RegistryError> {
        let id = resource.id().to_owned();
        if self.resources.contains_key(&id) {
            return Err(RegistryError::DuplicateCapability {
                kind: CapabilityKind::Resource,
                id,
            });
        }
        self.capabilities.push(self.descriptor(
            CapabilityKind::Resource,
            &id,
            "resource URI resolver",
        ));
        self.resources.insert(id, resource);
        Ok(())
    }

    pub fn register_execution(
        &mut self,
        execution: Arc<dyn ExecutionBackend>,
    ) -> Result<(), RegistryError> {
        let id = execution.id().to_owned();
        if self.execution.contains_key(&id) {
            return Err(RegistryError::DuplicateCapability {
                kind: CapabilityKind::Execution,
                id,
            });
        }
        self.capabilities.push(self.descriptor(
            CapabilityKind::Execution,
            &id,
            "bounded execution backend",
        ));
        self.execution.insert(id, execution);
        Ok(())
    }

    #[must_use]
    pub fn provider(&self, id: &str) -> Option<Arc<dyn Provider>> {
        self.providers.get(id).cloned()
    }

    #[must_use]
    pub fn providers(&self) -> Vec<Arc<dyn Provider>> {
        self.providers.values().cloned().collect()
    }

    #[must_use]
    pub fn tool(&self, id: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(id).cloned()
    }

    #[must_use]
    pub fn tools(&self) -> Vec<Arc<dyn Tool>> {
        self.tools.values().cloned().collect()
    }

    #[must_use]
    pub fn command(&self, id: &str) -> Option<Arc<dyn SlashCommand>> {
        self.commands.get(id.trim_start_matches('/')).cloned()
    }

    #[must_use]
    pub fn commands(&self) -> Vec<Arc<dyn SlashCommand>> {
        self.commands.values().cloned().collect()
    }

    #[must_use]
    pub fn router(&self, id: &str) -> Option<Arc<dyn Router>> {
        self.routers.get(id).cloned()
    }

    #[must_use]
    pub fn learning(&self, id: &str) -> Option<Arc<dyn LearningStore>> {
        self.learning.get(id).cloned()
    }

    #[must_use]
    pub fn session(&self, id: &str) -> Option<Arc<dyn SessionStore>> {
        self.sessions.get(id).cloned()
    }

    #[must_use]
    pub fn execution(&self, id: &str) -> Option<Arc<dyn ExecutionBackend>> {
        self.execution.get(id).cloned()
    }

    #[must_use]
    pub fn resolve_resource(&self, scheme: &str) -> Option<Arc<dyn ResourceResolver>> {
        self.resources
            .values()
            .find(|resolver| resolver.schemes().contains(&scheme))
            .cloned()
    }

    #[must_use]
    pub fn plugins(&self) -> Vec<PluginMetadata> {
        self.plugins.values().cloned().collect()
    }

    #[must_use]
    pub fn capabilities(&self) -> &[CapabilityDescriptor] {
        &self.capabilities
    }
}

#[derive(Default)]
pub struct Kernel {
    plugins: BTreeMap<String, Box<dyn Plugin>>,
}

impl Kernel {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, plugin: Box<dyn Plugin>) -> Result<(), PluginError> {
        let id = plugin.metadata().id;
        if self.plugins.insert(id.clone(), plugin).is_some() {
            return Err(PluginError::DuplicatePlugin(id));
        }
        Ok(())
    }

    pub fn mount(mut self) -> Result<Registry, PluginError> {
        let mut registry = Registry::default();
        let mut mounted = BTreeMap::<String, PluginMetadata>::new();

        while !self.plugins.is_empty() {
            let ready = self
                .plugins
                .iter()
                .find_map(|(id, plugin)| {
                    let metadata = plugin.metadata();
                    metadata
                        .dependencies
                        .iter()
                        .all(|dependency| mounted.contains_key(dependency))
                        .then(|| id.clone())
                });

            let Some(id) = ready else {
                let unresolved = self
                    .plugins
                    .values()
                    .map(|plugin| {
                        let metadata = plugin.metadata();
                        format!("{} -> {:?}", metadata.id, metadata.dependencies)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(PluginError::DependencyGraph(unresolved));
            };

            let mut plugin = self.plugins.remove(&id).ok_or_else(|| {
                PluginError::DependencyGraph(format!("plugin `{id}` disappeared during mount"))
            })?;
            let metadata = plugin.metadata();
            registry.mounting_plugin = Some(metadata.id.clone());
            plugin.mount(&mut registry).map_err(|message| PluginError::Mount {
                plugin: metadata.id.clone(),
                message,
            })?;
            registry.mounting_plugin = None;
            mounted.insert(metadata.id.clone(), metadata.clone());
            registry.plugins.insert(metadata.id.clone(), metadata);
        }

        Ok(registry)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::{Kernel, Plugin, PluginMetadata, Registry};

    struct TestPlugin {
        metadata: PluginMetadata,
        order: Arc<Mutex<Vec<String>>>,
    }

    impl Plugin for TestPlugin {
        fn metadata(&self) -> PluginMetadata {
            self.metadata.clone()
        }

        fn mount(&mut self, _registry: &mut Registry) -> Result<(), String> {
            self.order
                .lock()
                .map_err(|error| error.to_string())?
                .push(self.metadata.id.clone());
            Ok(())
        }
    }

    #[test]
    fn mounts_plugins_in_dependency_order() -> Result<(), Box<dyn std::error::Error>> {
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut kernel = Kernel::new();
        kernel.add(Box::new(TestPlugin {
            metadata: PluginMetadata::new("child", "1", "child")
                .with_dependencies(["parent"]),
            order: Arc::clone(&order),
        }))?;
        kernel.add(Box::new(TestPlugin {
            metadata: PluginMetadata::new("parent", "1", "parent"),
            order: Arc::clone(&order),
        }))?;
        let _registry = kernel.mount()?;
        let mounted = order.lock().map_err(|error| error.to_string())?.clone();
        assert_eq!(mounted, vec!["parent", "child"]);
        Ok(())
    }
}
