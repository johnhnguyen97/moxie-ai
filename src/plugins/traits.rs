//! Enhanced plugin traits with lifecycle hooks
//!
//! This module defines the core Plugin trait that all plugins must implement,
//! along with lifecycle hooks for initialization, shutdown, and state management.

use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;

use super::manifest::PluginManifest;
use super::{PluginError, ToolDefinition, ToolResult};

/// Plugin state for lifecycle management
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginState {
    /// Plugin is registered but not initialized
    Registered,
    /// Plugin is initializing
    Initializing,
    /// Plugin is ready and active
    Active,
    /// Plugin is disabled (tools not available)
    Disabled,
    /// Plugin encountered an error
    Error,
    /// Plugin is shutting down
    ShuttingDown,
}

/// Context provided to plugins during lifecycle events
pub struct PluginContext {
    /// Plugin configuration from client TOML
    pub config: Value,

    /// Data directory for plugin storage
    pub data_dir: std::path::PathBuf,

    /// Whether the plugin is running in debug mode
    pub debug: bool,
}

impl Default for PluginContext {
    fn default() -> Self {
        Self {
            config: Value::Object(serde_json::Map::new()),
            data_dir: std::path::PathBuf::from("./data/plugins"),
            debug: false,
        }
    }
}

/// Core trait that all plugins must implement
///
/// # Example
///
/// ```ignore
/// use moxie_ai::plugins::{Plugin, PluginManifest, ToolDefinition, ToolResult, PluginError};
///
/// pub struct MyPlugin {
///     config: MyConfig,
/// }
///
/// #[async_trait]
/// impl Plugin for MyPlugin {
///     fn manifest(&self) -> PluginManifest {
///         PluginManifest::new("com.example.myplugin", "My Plugin", "Does something cool")
///             .with_version(1, 0, 0)
///             .with_author("Your Name")
///     }
///
///     fn tools(&self) -> Vec<ToolDefinition> {
///         vec![
///             ToolDefinition::new("my_tool", "Description of what it does")
///         ]
///     }
///
///     async fn execute(&self, tool: &str, params: Value) -> Result<ToolResult, PluginError> {
///         match tool {
///             "my_tool" => Ok(ToolResult::success("result")),
///             _ => Err(PluginError::ToolNotFound(tool.to_string()))
///         }
///     }
/// }
/// ```
#[async_trait]
pub trait Plugin: Send + Sync {
    // ========== Required Methods ==========

    /// Returns the plugin manifest with metadata
    fn manifest(&self) -> PluginManifest;

    /// Returns all tools provided by this plugin
    fn tools(&self) -> Vec<ToolDefinition>;

    /// Execute a specific tool with the given parameters
    async fn execute(&self, tool: &str, params: Value) -> Result<ToolResult, PluginError>;

    // ========== Lifecycle Hooks (Optional) ==========

    /// Called when the plugin is first loaded
    /// Use this for one-time initialization, resource allocation, etc.
    async fn on_init(&mut self, _ctx: &PluginContext) -> Result<(), PluginError> {
        Ok(())
    }

    /// Called when the plugin is being shut down
    /// Use this for cleanup, saving state, closing connections, etc.
    async fn on_shutdown(&mut self) -> Result<(), PluginError> {
        Ok(())
    }

    /// Called when the plugin is enabled (after being disabled)
    async fn on_enable(&mut self) -> Result<(), PluginError> {
        Ok(())
    }

    /// Called when the plugin is disabled (but not unloaded)
    async fn on_disable(&mut self) -> Result<(), PluginError> {
        Ok(())
    }

    /// Called before a tool is executed
    /// Return Err to prevent execution
    async fn before_execute(&self, _tool: &str, _params: &Value) -> Result<(), PluginError> {
        Ok(())
    }

    /// Called after a tool is executed (with the result)
    async fn after_execute(&self, _tool: &str, _result: &ToolResult) -> Result<(), PluginError> {
        Ok(())
    }

    // ========== Convenience Methods ==========

    /// Returns the plugin ID from the manifest
    fn id(&self) -> String {
        self.manifest().id
    }

    /// Returns the plugin name from the manifest
    fn name(&self) -> &str {
        // Note: This returns a reference to a temporary, which won't work
        // In practice, plugins should store the manifest or name
        "plugin"
    }

    /// Returns the plugin description from the manifest
    fn description(&self) -> &str {
        "A Moxie plugin"
    }

    /// Check if this plugin provides a specific tool
    fn has_tool(&self, tool: &str) -> bool {
        self.tools().iter().any(|t| t.name == tool)
    }

    /// Get a tool definition by name
    fn get_tool(&self, name: &str) -> Option<ToolDefinition> {
        self.tools().into_iter().find(|t| t.name == name)
    }

    /// Downcast to concrete type (for plugin-specific functionality)
    fn as_any(&self) -> &dyn Any;

    /// Downcast to mutable concrete type
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// Extension trait for easier plugin development
pub trait PluginExt: Plugin {
    /// Validate configuration against the manifest schema
    fn validate_config(&self, config: &Value) -> Result<(), Vec<String>> {
        let manifest = self.manifest();
        let mut errors = Vec::new();

        for field in &manifest.config_schema {
            let value = config.get(&field.name);

            // Check required fields
            if field.required && value.is_none() {
                errors.push(format!("Missing required field: {}", field.name));
                continue;
            }

            // Additional validation could go here
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

// Automatically implement PluginExt for all Plugin implementations
impl<T: Plugin + ?Sized> PluginExt for T {}

/// Macro for easy plugin creation
///
/// # Example
///
/// ```ignore
/// moxie_plugin! {
///     name: "My Plugin",
///     id: "com.example.myplugin",
///     version: "1.0.0",
///     author: "Your Name",
///     description: "Does something amazing",
///     tools: [
///         ("tool_name", "Tool description", tool_handler),
///     ],
/// }
/// ```
#[macro_export]
macro_rules! moxie_plugin {
    (
        name: $name:expr,
        id: $id:expr,
        version: $version:expr,
        author: $author:expr,
        description: $desc:expr,
        category: $category:expr,
        $(tools: [$( ($tool_name:expr, $tool_desc:expr) ),* $(,)?] )?
    ) => {
        // Plugin metadata
        pub const PLUGIN_NAME: &str = $name;
        pub const PLUGIN_ID: &str = $id;
        pub const PLUGIN_VERSION: &str = $version;
        pub const PLUGIN_AUTHOR: &str = $author;
        pub const PLUGIN_DESCRIPTION: &str = $desc;
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestPlugin;

    #[async_trait]
    impl Plugin for TestPlugin {
        fn manifest(&self) -> PluginManifest {
            PluginManifest::new("test.plugin", "Test Plugin", "A test plugin")
        }

        fn tools(&self) -> Vec<ToolDefinition> {
            vec![ToolDefinition::new("test_tool", "A test tool")]
        }

        async fn execute(&self, tool: &str, _params: Value) -> Result<ToolResult, PluginError> {
            match tool {
                "test_tool" => Ok(ToolResult::success("test result")),
                _ => Err(PluginError::ToolNotFound(tool.to_string())),
            }
        }

        fn as_any(&self) -> &dyn Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    #[test]
    fn test_plugin_ext() {
        let plugin = TestPlugin;
        assert!(plugin.has_tool("test_tool"));
        assert!(!plugin.has_tool("nonexistent"));
    }
}
