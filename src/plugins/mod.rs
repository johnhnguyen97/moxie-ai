//! Plugin system for extensible AI capabilities
//!
//! Moxie's plugin architecture allows you to extend the AI assistant with custom
//! capabilities. Plugins provide "tools" that the AI can use to interact with
//! external systems like files, databases, APIs, and hardware.
//!
//! # Quick Start
//!
//! 1. Implement the `Plugin` trait
//! 2. Register your plugin with the `PluginLoader`
//! 3. Your tools are automatically available to the AI
//!
//! # Example
//!
//! ```ignore
//! use moxie_ai::plugins::prelude::*;
//!
//! pub struct MyPlugin;
//!
//! #[async_trait]
//! impl Plugin for MyPlugin {
//!     fn manifest(&self) -> PluginManifest {
//!         PluginManifest::new("com.example.myplugin", "My Plugin", "Does something cool")
//!             .with_version(1, 0, 0)
//!             .with_author("Your Name")
//!     }
//!
//!     fn tools(&self) -> Vec<ToolDefinition> {
//!         vec![
//!             ToolDefinition::new("my_tool", "Does something useful")
//!                 .with_parameters(json!({
//!                     "type": "object",
//!                     "properties": {
//!                         "input": { "type": "string" }
//!                     },
//!                     "required": ["input"]
//!                 }))
//!         ]
//!     }
//!
//!     async fn execute(&self, tool: &str, params: Value) -> Result<ToolResult, PluginError> {
//!         match tool {
//!             "my_tool" => {
//!                 let input = params["input"].as_str().unwrap_or("");
//!                 Ok(ToolResult::success(format!("Processed: {}", input)))
//!             }
//!             _ => Err(PluginError::ToolNotFound(tool.to_string()))
//!         }
//!     }
//!
//!     fn as_any(&self) -> &dyn std::any::Any { self }
//!     fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
//! }
//! ```
//!
//! # Built-in Plugins
//!
//! - `filesystem` - Read, write, and list files

pub mod api;
pub mod filesystem;
pub mod loader;
pub mod manifest;
pub mod traits;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

// Re-exports for convenience
pub use loader::{PluginLoader, SharedPluginLoader, shared_loader};
pub use manifest::{
    ConfigField, ConfigFieldBuilder, ConfigFieldType, PluginCategory, PluginManifest, Version,
};
pub use traits::{Plugin, PluginContext, PluginState, PluginExt};

/// Prelude for plugin development
pub mod prelude {
    pub use super::{
        Plugin, PluginCategory, PluginContext, PluginError, PluginExt, PluginManifest,
        PluginState, ToolDefinition, ToolResult, Version,
        ConfigField, ConfigFieldBuilder, ConfigFieldType,
    };
    pub use async_trait::async_trait;
    pub use serde_json::{json, Value};
}

/// Errors that can occur during plugin operations
#[derive(Debug, Error)]
pub enum PluginError {
    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Invalid parameters: {0}")]
    InvalidParameters(String),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Plugin not found: {0}")]
    PluginNotFound(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Plugin disabled: {0}")]
    PluginDisabled(String),

    #[error("Initialization failed: {0}")]
    InitFailed(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),
}

/// Definition of a tool that an AI can call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Unique name of the tool (e.g., "read_file", "query_database")
    pub name: String,

    /// Human-readable description for the AI to understand when to use this tool
    pub description: String,

    /// JSON Schema defining the expected parameters
    pub parameters: Value,

    /// Whether this tool requires user confirmation before execution
    #[serde(default)]
    pub requires_confirmation: bool,

    /// Plugin ID that provides this tool
    #[serde(default)]
    pub plugin_id: Option<String>,
}

impl ToolDefinition {
    /// Create a new tool definition
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            requires_confirmation: false,
            plugin_id: None,
        }
    }

    /// Set the parameters schema for this tool
    pub fn with_parameters(mut self, parameters: Value) -> Self {
        self.parameters = parameters;
        self
    }

    /// Mark this tool as requiring confirmation
    pub fn with_confirmation(mut self) -> Self {
        self.requires_confirmation = true;
        self
    }

    /// Set the plugin ID for this tool
    pub fn from_plugin(mut self, plugin_id: impl Into<String>) -> Self {
        self.plugin_id = Some(plugin_id.into());
        self
    }
}

/// Result of executing a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Whether the tool execution was successful
    pub success: bool,

    /// The output/result of the tool execution
    pub output: Value,

    /// Optional error message if execution failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Execution metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ToolResultMetadata>,
}

/// Metadata about tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultMetadata {
    /// Execution time in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,

    /// Plugin that executed the tool
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_id: Option<String>,
}

impl ToolResult {
    /// Create a successful result
    pub fn success(output: impl Into<Value>) -> Self {
        Self {
            success: true,
            output: output.into(),
            error: None,
            metadata: None,
        }
    }

    /// Create a failed result
    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            output: Value::Null,
            error: Some(error.into()),
            metadata: None,
        }
    }

    /// Add metadata to the result
    pub fn with_metadata(mut self, metadata: ToolResultMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Add duration metadata
    pub fn with_duration(mut self, duration_ms: u64) -> Self {
        let metadata = self.metadata.get_or_insert(ToolResultMetadata {
            duration_ms: None,
            plugin_id: None,
        });
        metadata.duration_ms = Some(duration_ms);
        self
    }
}

// ============================================================================
// Legacy PluginRegistry (backwards compatibility)
// ============================================================================

/// Legacy plugin registry - use PluginLoader for new code
///
/// This is maintained for backwards compatibility with existing code.
/// New code should use `PluginLoader` instead.
pub struct PluginRegistry {
    plugins: HashMap<String, Arc<dyn LegacyPlugin>>,
}

/// Legacy plugin trait (backwards compatibility)
#[async_trait]
pub trait LegacyPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn tools(&self) -> Vec<ToolDefinition>;
    async fn execute(&self, tool: &str, params: Value) -> Result<ToolResult, PluginError>;

    fn has_tool(&self, tool: &str) -> bool {
        self.tools().iter().any(|t| t.name == tool)
    }
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    pub fn register<P: LegacyPlugin + 'static>(&mut self, plugin: P) {
        let name = plugin.name().to_string();
        self.plugins.insert(name, Arc::new(plugin));
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn LegacyPlugin>> {
        self.plugins.get(name).cloned()
    }

    pub fn all(&self) -> Vec<Arc<dyn LegacyPlugin>> {
        self.plugins.values().cloned().collect()
    }

    pub fn all_tools(&self) -> Vec<ToolDefinition> {
        self.plugins.values().flat_map(|p| p.tools()).collect()
    }

    pub fn find_plugin_for_tool(&self, tool: &str) -> Option<Arc<dyn LegacyPlugin>> {
        self.plugins.values().find(|p| p.has_tool(tool)).cloned()
    }

    pub async fn execute(&self, tool: &str, params: Value) -> Result<ToolResult, PluginError> {
        let plugin = self
            .find_plugin_for_tool(tool)
            .ok_or_else(|| PluginError::ToolNotFound(tool.to_string()))?;
        plugin.execute(tool, params).await
    }

    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestPlugin;

    #[async_trait]
    impl LegacyPlugin for TestPlugin {
        fn name(&self) -> &str {
            "test"
        }

        fn description(&self) -> &str {
            "A test plugin"
        }

        fn tools(&self) -> Vec<ToolDefinition> {
            vec![ToolDefinition::new("test_tool", "A test tool")]
        }

        async fn execute(&self, tool: &str, _params: Value) -> Result<ToolResult, PluginError> {
            if tool == "test_tool" {
                Ok(ToolResult::success("test result"))
            } else {
                Err(PluginError::ToolNotFound(tool.to_string()))
            }
        }
    }

    #[test]
    fn test_tool_definition() {
        let tool = ToolDefinition::new("my_tool", "Does something");
        assert_eq!(tool.name, "my_tool");
        assert_eq!(tool.description, "Does something");
    }

    #[test]
    fn test_tool_result() {
        let success = ToolResult::success("output");
        assert!(success.success);
        assert!(success.error.is_none());

        let failure = ToolResult::failure("something went wrong");
        assert!(!failure.success);
        assert!(failure.error.is_some());
    }

    #[test]
    fn test_registry() {
        let mut registry = PluginRegistry::new();
        registry.register(TestPlugin);

        assert_eq!(registry.len(), 1);
        assert!(registry.get("test").is_some());
        assert!(registry.find_plugin_for_tool("test_tool").is_some());
    }

    #[tokio::test]
    async fn test_execute_tool() {
        let mut registry = PluginRegistry::new();
        registry.register(TestPlugin);

        let result = registry.execute("test_tool", Value::Null).await.unwrap();

        assert!(result.success);
    }
}
