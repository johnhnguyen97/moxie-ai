//! Plugin loader and registry management
//!
//! Handles plugin discovery, loading, lifecycle management, and the plugin store.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::manifest::{PluginManifest, Version};
use super::traits::{Plugin, PluginContext, PluginState};
use super::{PluginError, ToolDefinition, ToolResult};
use serde_json::Value;

/// Information about a loaded plugin
pub struct LoadedPlugin {
    /// The plugin instance
    pub plugin: Box<dyn Plugin>,

    /// Current state
    pub state: PluginState,

    /// Configuration from client TOML
    pub config: Value,

    /// Load order (for dependency resolution)
    pub load_order: usize,
}

/// Enhanced plugin registry with lifecycle management
pub struct PluginLoader {
    /// Loaded plugins by ID
    plugins: HashMap<String, LoadedPlugin>,

    /// Plugin load order counter
    load_counter: usize,

    /// Plugins directory for external plugins
    plugins_dir: Option<PathBuf>,

    /// Global plugin context
    context: PluginContext,
}

impl PluginLoader {
    /// Create a new plugin loader
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            load_counter: 0,
            plugins_dir: None,
            context: PluginContext::default(),
        }
    }

    /// Set the plugins directory for external plugin discovery
    pub fn with_plugins_dir(mut self, dir: PathBuf) -> Self {
        self.plugins_dir = Some(dir);
        self
    }

    /// Set the plugin context (data directory, debug mode, etc.)
    pub fn with_context(mut self, context: PluginContext) -> Self {
        self.context = context;
        self
    }

    /// Register a built-in plugin
    pub fn register<P: Plugin + 'static>(&mut self, plugin: P) -> Result<(), PluginError> {
        let manifest = plugin.manifest();
        let id = manifest.id.clone();

        // Validate manifest
        manifest.validate().map_err(|e| PluginError::InvalidParameters(e))?;

        // Check for duplicates
        if self.plugins.contains_key(&id) {
            return Err(PluginError::ExecutionFailed(format!(
                "Plugin '{}' is already registered",
                id
            )));
        }

        // Check dependencies
        for (dep_id, required_version) in &manifest.dependencies {
            if let Some(dep) = self.plugins.get(dep_id) {
                let dep_version = &dep.plugin.manifest().version;
                if !dep_version.is_compatible_with(required_version) {
                    return Err(PluginError::ExecutionFailed(format!(
                        "Plugin '{}' requires {} version {}, but {} is loaded",
                        id, dep_id, required_version, dep_version
                    )));
                }
            } else {
                return Err(PluginError::ExecutionFailed(format!(
                    "Plugin '{}' requires '{}' which is not loaded",
                    id, dep_id
                )));
            }
        }

        self.load_counter += 1;

        self.plugins.insert(
            id.clone(),
            LoadedPlugin {
                plugin: Box::new(plugin),
                state: PluginState::Registered,
                config: Value::Object(serde_json::Map::new()),
                load_order: self.load_counter,
            },
        );

        tracing::info!("Registered plugin: {} v{}", id, manifest.version);

        Ok(())
    }

    /// Register a plugin with configuration
    pub fn register_with_config<P: Plugin + 'static>(
        &mut self,
        plugin: P,
        config: Value,
    ) -> Result<(), PluginError> {
        let manifest = plugin.manifest();
        let id = manifest.id.clone();

        self.register(plugin)?;

        if let Some(loaded) = self.plugins.get_mut(&id) {
            loaded.config = config;
        }

        Ok(())
    }

    /// Initialize all registered plugins
    pub async fn init_all(&mut self) -> Result<(), PluginError> {
        // Sort by load order for consistent initialization
        let mut ids: Vec<_> = self.plugins.keys().cloned().collect();
        ids.sort_by_key(|id| self.plugins.get(id).map(|p| p.load_order).unwrap_or(0));

        for id in ids {
            self.init_plugin(&id).await?;
        }

        Ok(())
    }

    /// Initialize a specific plugin
    pub async fn init_plugin(&mut self, id: &str) -> Result<(), PluginError> {
        let loaded = self
            .plugins
            .get_mut(id)
            .ok_or_else(|| PluginError::PluginNotFound(id.to_string()))?;

        if loaded.state != PluginState::Registered {
            return Ok(()); // Already initialized or in another state
        }

        loaded.state = PluginState::Initializing;

        // Create context with plugin-specific config
        let ctx = PluginContext {
            config: loaded.config.clone(),
            data_dir: self.context.data_dir.join(id),
            debug: self.context.debug,
        };

        // Create data directory if it doesn't exist
        if let Err(e) = std::fs::create_dir_all(&ctx.data_dir) {
            tracing::warn!("Failed to create plugin data dir: {}", e);
        }

        // Call plugin's init hook
        if let Err(e) = loaded.plugin.on_init(&ctx).await {
            loaded.state = PluginState::Error;
            return Err(e);
        }

        loaded.state = PluginState::Active;
        tracing::info!("Initialized plugin: {}", id);

        Ok(())
    }

    /// Shutdown all plugins
    pub async fn shutdown_all(&mut self) -> Result<(), PluginError> {
        // Shutdown in reverse load order
        let mut ids: Vec<_> = self.plugins.keys().cloned().collect();
        ids.sort_by_key(|id| {
            self.plugins
                .get(id)
                .map(|p| std::cmp::Reverse(p.load_order))
                .unwrap_or(std::cmp::Reverse(0))
        });

        for id in ids {
            if let Err(e) = self.shutdown_plugin(&id).await {
                tracing::error!("Error shutting down plugin {}: {}", id, e);
            }
        }

        Ok(())
    }

    /// Shutdown a specific plugin
    pub async fn shutdown_plugin(&mut self, id: &str) -> Result<(), PluginError> {
        let loaded = self
            .plugins
            .get_mut(id)
            .ok_or_else(|| PluginError::PluginNotFound(id.to_string()))?;

        if loaded.state != PluginState::Active && loaded.state != PluginState::Disabled {
            return Ok(());
        }

        loaded.state = PluginState::ShuttingDown;

        if let Err(e) = loaded.plugin.on_shutdown().await {
            tracing::error!("Error in plugin shutdown: {}", e);
        }

        loaded.state = PluginState::Registered;
        tracing::info!("Shutdown plugin: {}", id);

        Ok(())
    }

    /// Enable a disabled plugin
    pub async fn enable_plugin(&mut self, id: &str) -> Result<(), PluginError> {
        let loaded = self
            .plugins
            .get_mut(id)
            .ok_or_else(|| PluginError::PluginNotFound(id.to_string()))?;

        if loaded.state != PluginState::Disabled {
            return Ok(());
        }

        loaded.plugin.on_enable().await?;
        loaded.state = PluginState::Active;

        tracing::info!("Enabled plugin: {}", id);
        Ok(())
    }

    /// Disable an active plugin (without unloading)
    pub async fn disable_plugin(&mut self, id: &str) -> Result<(), PluginError> {
        let loaded = self
            .plugins
            .get_mut(id)
            .ok_or_else(|| PluginError::PluginNotFound(id.to_string()))?;

        if loaded.state != PluginState::Active {
            return Ok(());
        }

        loaded.plugin.on_disable().await?;
        loaded.state = PluginState::Disabled;

        tracing::info!("Disabled plugin: {}", id);
        Ok(())
    }

    /// Get a plugin by ID
    pub fn get(&self, id: &str) -> Option<&dyn Plugin> {
        self.plugins.get(id).map(|p| p.plugin.as_ref())
    }

    /// Get plugin state
    pub fn get_state(&self, id: &str) -> Option<PluginState> {
        self.plugins.get(id).map(|p| p.state)
    }

    /// List all registered plugins (returns owned manifests)
    pub fn list(&self) -> Vec<PluginManifest> {
        self.plugins
            .values()
            .map(|p| p.plugin.manifest())
            .collect()
    }

    /// List all active plugins
    pub fn list_active(&self) -> Vec<String> {
        self.plugins
            .iter()
            .filter(|(_, p)| p.state == PluginState::Active)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Get all tools from all active plugins
    pub fn all_tools(&self) -> Vec<ToolDefinition> {
        self.plugins
            .values()
            .filter(|p| p.state == PluginState::Active)
            .flat_map(|p| p.plugin.tools())
            .collect()
    }

    /// Find which plugin provides a tool
    pub fn find_plugin_for_tool(&self, tool: &str) -> Option<&str> {
        self.plugins
            .iter()
            .filter(|(_, p)| p.state == PluginState::Active)
            .find(|(_, p)| p.plugin.has_tool(tool))
            .map(|(id, _)| id.as_str())
    }

    /// Execute a tool
    pub async fn execute(&self, tool: &str, params: Value) -> Result<ToolResult, PluginError> {
        let (id, loaded) = self
            .plugins
            .iter()
            .filter(|(_, p)| p.state == PluginState::Active)
            .find(|(_, p)| p.plugin.has_tool(tool))
            .ok_or_else(|| PluginError::ToolNotFound(tool.to_string()))?;

        // Check if confirmation is required
        let manifest = loaded.plugin.manifest();
        if manifest.requires_confirmation {
            // In a real implementation, this would prompt the user
            tracing::warn!(
                "Tool '{}' from plugin '{}' requires confirmation",
                tool,
                id
            );
        }

        // Call before_execute hook
        loaded.plugin.before_execute(tool, &params).await?;

        // Execute the tool
        let result = loaded.plugin.execute(tool, params).await?;

        // Call after_execute hook
        loaded.plugin.after_execute(tool, &result).await?;

        Ok(result)
    }

    /// Get the number of registered plugins
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// Check if no plugins are registered
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }
}

impl Default for PluginLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe plugin loader for use with Axum state
pub type SharedPluginLoader = Arc<RwLock<PluginLoader>>;

/// Create a shared plugin loader
pub fn shared_loader() -> SharedPluginLoader {
    Arc::new(RwLock::new(PluginLoader::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::manifest::PluginCategory;

    struct TestPlugin {
        name: String,
    }

    impl TestPlugin {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
            }
        }
    }

    #[async_trait::async_trait]
    impl Plugin for TestPlugin {
        fn manifest(&self) -> PluginManifest {
            PluginManifest::new(
                format!("test.{}", self.name),
                &self.name,
                "A test plugin",
            )
            .with_category(PluginCategory::Custom)
        }

        fn tools(&self) -> Vec<ToolDefinition> {
            vec![ToolDefinition::new(
                format!("{}_tool", self.name),
                "A test tool",
            )]
        }

        async fn execute(&self, tool: &str, _params: Value) -> Result<ToolResult, PluginError> {
            Ok(ToolResult::success(format!("Executed {} on {}", tool, self.name)))
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    }

    #[test]
    fn test_register_plugin() {
        let mut loader = PluginLoader::new();
        loader.register(TestPlugin::new("foo")).unwrap();

        assert_eq!(loader.len(), 1);
        assert!(loader.get("test.foo").is_some());
    }

    #[test]
    fn test_duplicate_registration() {
        let mut loader = PluginLoader::new();
        loader.register(TestPlugin::new("foo")).unwrap();

        let result = loader.register(TestPlugin::new("foo"));
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_tool() {
        let mut loader = PluginLoader::new();
        loader.register(TestPlugin::new("foo")).unwrap();
        loader.init_all().await.unwrap();

        let result = loader
            .execute("foo_tool", Value::Null)
            .await
            .unwrap();

        assert!(result.success);
    }

    #[test]
    fn test_all_tools() {
        let mut loader = PluginLoader::new();
        loader.register(TestPlugin::new("foo")).unwrap();
        loader.register(TestPlugin::new("bar")).unwrap();

        // Plugins are registered but not active yet
        let tools = loader.all_tools();
        assert_eq!(tools.len(), 0); // Not active
    }
}
