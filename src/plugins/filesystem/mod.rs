//! Filesystem plugin for local file operations
//!
//! Provides tools for reading, writing, and listing files on the local filesystem.
//! Access is restricted to configured allowed paths for security.
//!
//! # Tools
//!
//! - `read_file` - Read contents of a file
//! - `write_file` - Write content to a file (if enabled)
//! - `list_directory` - List files in a directory
//!
//! # Configuration
//!
//! ```toml
//! [plugins.filesystem]
//! allowed_paths = ["C:\\Data", "D:\\Reports"]
//! allow_write = false
//! max_file_size = 10485760  # 10 MB
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::any::Any;
use std::path::{Path, PathBuf};
use tokio::fs;

use crate::plugins::manifest::{
    ConfigField, ConfigFieldBuilder, ConfigFieldType, PluginCategory, PluginManifest,
};
use crate::plugins::traits::{Plugin, PluginContext};
use crate::plugins::{LegacyPlugin, PluginError, ToolDefinition, ToolResult};

/// Configuration for the filesystem plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemConfig {
    /// Paths that the plugin is allowed to access
    pub allowed_paths: Vec<PathBuf>,

    /// Whether to allow write operations
    #[serde(default)]
    pub allow_write: bool,

    /// Maximum file size to read (in bytes)
    #[serde(default = "default_max_file_size")]
    pub max_file_size: u64,
}

fn default_max_file_size() -> u64 {
    10 * 1024 * 1024 // 10 MB
}

impl Default for FilesystemConfig {
    fn default() -> Self {
        Self {
            allowed_paths: vec![],
            allow_write: false,
            max_file_size: default_max_file_size(),
        }
    }
}

impl FilesystemConfig {
    /// Parse configuration from a JSON Value
    pub fn from_value(value: &Value) -> Result<Self, PluginError> {
        if value.is_null() {
            return Ok(Self::default());
        }

        serde_json::from_value(value.clone())
            .map_err(|e| PluginError::ConfigError(e.to_string()))
    }
}

/// Filesystem plugin for local file access
pub struct FilesystemPlugin {
    config: FilesystemConfig,
}

impl FilesystemPlugin {
    /// Plugin ID constant
    pub const ID: &'static str = "moxie.filesystem";

    /// Create a new filesystem plugin with the given configuration
    pub fn new(config: FilesystemConfig) -> Self {
        Self { config }
    }

    /// Create with default configuration
    pub fn default_plugin() -> Self {
        Self::new(FilesystemConfig::default())
    }

    /// Check if a path is within the allowed paths
    fn is_path_allowed(&self, path: &Path) -> bool {
        // If no paths are configured, allow nothing
        if self.config.allowed_paths.is_empty() {
            return false;
        }

        // Canonicalize the path to prevent directory traversal attacks
        let canonical = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                // If we can't canonicalize, check parent directory for new files
                if let Some(parent) = path.parent() {
                    match parent.canonicalize() {
                        Ok(p) => p,
                        Err(_) => return false,
                    }
                } else {
                    return false;
                }
            }
        };

        self.config.allowed_paths.iter().any(|allowed| {
            if let Ok(allowed_canonical) = allowed.canonicalize() {
                canonical.starts_with(&allowed_canonical)
            } else {
                false
            }
        })
    }

    /// Read a file from the filesystem
    async fn read_file(&self, path: &str) -> Result<ToolResult, PluginError> {
        let path = Path::new(path);

        if !self.is_path_allowed(path) {
            return Ok(ToolResult::failure(format!(
                "Access denied: path '{}' is not in allowed paths",
                path.display()
            )));
        }

        // Check file exists
        if !path.exists() {
            return Ok(ToolResult::failure(format!(
                "File not found: {}",
                path.display()
            )));
        }

        // Check file size
        let metadata = fs::metadata(path).await?;
        if metadata.len() > self.config.max_file_size {
            return Ok(ToolResult::failure(format!(
                "File too large: {} bytes (max: {} bytes)",
                metadata.len(),
                self.config.max_file_size
            )));
        }

        let content = fs::read_to_string(path).await?;

        Ok(ToolResult::success(json!({
            "path": path.to_string_lossy(),
            "content": content,
            "size": metadata.len()
        })))
    }

    /// Write content to a file
    async fn write_file(&self, path: &str, content: &str) -> Result<ToolResult, PluginError> {
        if !self.config.allow_write {
            return Ok(ToolResult::failure(
                "Write operations are disabled for this plugin",
            ));
        }

        let path = Path::new(path);

        if !self.is_path_allowed(path) {
            return Ok(ToolResult::failure(format!(
                "Access denied: path '{}' is not in allowed paths",
                path.display()
            )));
        }

        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).await?;
            }
        }

        fs::write(path, content).await?;

        Ok(ToolResult::success(json!({
            "path": path.to_string_lossy(),
            "bytes_written": content.len()
        })))
    }

    /// List files in a directory
    async fn list_directory(&self, path: &str) -> Result<ToolResult, PluginError> {
        let path = Path::new(path);

        if !self.is_path_allowed(path) {
            return Ok(ToolResult::failure(format!(
                "Access denied: path '{}' is not in allowed paths",
                path.display()
            )));
        }

        if !path.exists() {
            return Ok(ToolResult::failure(format!(
                "Directory not found: {}",
                path.display()
            )));
        }

        let mut entries = Vec::new();
        let mut read_dir = fs::read_dir(path).await?;

        while let Some(entry) = read_dir.next_entry().await? {
            let file_type = entry.file_type().await?;
            let metadata = entry.metadata().await?;

            entries.push(json!({
                "name": entry.file_name().to_string_lossy(),
                "path": entry.path().to_string_lossy(),
                "is_file": file_type.is_file(),
                "is_dir": file_type.is_dir(),
                "size": metadata.len()
            }));
        }

        Ok(ToolResult::success(json!({
            "path": path.to_string_lossy(),
            "count": entries.len(),
            "entries": entries
        })))
    }

    /// Build tools list based on configuration
    fn build_tools(&self) -> Vec<ToolDefinition> {
        let mut tools = vec![
            ToolDefinition::new("read_file", "Read the contents of a file")
                .with_parameters(json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "The path to the file to read"
                        }
                    },
                    "required": ["path"]
                }))
                .from_plugin(Self::ID),
            ToolDefinition::new("list_directory", "List files and directories in a path")
                .with_parameters(json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "The directory path to list"
                        }
                    },
                    "required": ["path"]
                }))
                .from_plugin(Self::ID),
        ];

        if self.config.allow_write {
            tools.push(
                ToolDefinition::new("write_file", "Write content to a file")
                    .with_parameters(json!({
                        "type": "object",
                        "properties": {
                            "path": {
                                "type": "string",
                                "description": "The path to write to"
                            },
                            "content": {
                                "type": "string",
                                "description": "The content to write"
                            }
                        },
                        "required": ["path", "content"]
                    }))
                    .with_confirmation()
                    .from_plugin(Self::ID),
            );
        }

        tools
    }
}

// ============================================================================
// New Plugin trait implementation
// ============================================================================

#[async_trait]
impl Plugin for FilesystemPlugin {
    fn manifest(&self) -> PluginManifest {
        PluginManifest::new(
            Self::ID,
            "Filesystem",
            "Read, write, and list files on the local filesystem",
        )
        .with_version(1, 0, 0)
        .with_author("Moxie AI")
        .with_category(PluginCategory::Filesystem)
        .with_keywords(vec!["files", "filesystem", "read", "write", "directory"])
        .with_config_field(
            ConfigFieldBuilder::new("allowed_paths", ConfigFieldType::PathArray)
                .label("Allowed Paths")
                .description("Directories the plugin can access")
                .required()
                .build(),
        )
        .with_config_field(
            ConfigFieldBuilder::new("allow_write", ConfigFieldType::Boolean)
                .label("Allow Write")
                .description("Enable file write operations")
                .default_value(json!(false))
                .build(),
        )
        .with_config_field(
            ConfigFieldBuilder::new("max_file_size", ConfigFieldType::Number)
                .label("Max File Size")
                .description("Maximum file size to read (in bytes)")
                .default_value(json!(10485760))
                .build(),
        )
    }

    fn tools(&self) -> Vec<ToolDefinition> {
        self.build_tools()
    }

    async fn execute(&self, tool: &str, params: Value) -> Result<ToolResult, PluginError> {
        match tool {
            "read_file" => {
                let path = params["path"]
                    .as_str()
                    .ok_or_else(|| PluginError::InvalidParameters("path is required".into()))?;
                self.read_file(path).await
            }
            "write_file" => {
                let path = params["path"]
                    .as_str()
                    .ok_or_else(|| PluginError::InvalidParameters("path is required".into()))?;
                let content = params["content"]
                    .as_str()
                    .ok_or_else(|| PluginError::InvalidParameters("content is required".into()))?;
                self.write_file(path, content).await
            }
            "list_directory" => {
                let path = params["path"]
                    .as_str()
                    .ok_or_else(|| PluginError::InvalidParameters("path is required".into()))?;
                self.list_directory(path).await
            }
            _ => Err(PluginError::ToolNotFound(tool.to_string())),
        }
    }

    async fn on_init(&mut self, ctx: &PluginContext) -> Result<(), PluginError> {
        // Update config from context if provided
        if !ctx.config.is_null() {
            self.config = FilesystemConfig::from_value(&ctx.config)?;
        }

        tracing::info!(
            "Filesystem plugin initialized with {} allowed paths",
            self.config.allowed_paths.len()
        );

        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// ============================================================================
// Legacy Plugin trait implementation (backwards compatibility)
// ============================================================================

#[async_trait]
impl LegacyPlugin for FilesystemPlugin {
    fn name(&self) -> &str {
        "filesystem"
    }

    fn description(&self) -> &str {
        "Provides access to read and write files on the local filesystem"
    }

    fn tools(&self) -> Vec<ToolDefinition> {
        self.build_tools()
    }

    async fn execute(&self, tool: &str, params: Value) -> Result<ToolResult, PluginError> {
        // Delegate to the new Plugin implementation
        Plugin::execute(self, tool, params).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::traits::Plugin as NewPlugin;
    use std::env;

    fn test_config() -> FilesystemConfig {
        FilesystemConfig {
            allowed_paths: vec![env::temp_dir()],
            allow_write: true,
            max_file_size: 1024 * 1024,
        }
    }

    #[test]
    fn test_manifest() {
        let plugin = FilesystemPlugin::new(test_config());
        let manifest = NewPlugin::manifest(&plugin);

        assert_eq!(manifest.id, "moxie.filesystem");
        assert_eq!(manifest.category, PluginCategory::Filesystem);
        assert!(!manifest.config_schema.is_empty());
    }

    #[tokio::test]
    async fn test_read_write_file() {
        let plugin = FilesystemPlugin::new(test_config());
        let test_path = env::temp_dir().join("moxie_test_file.txt");

        // Write - use the internal method directly
        let write_result = plugin
            .write_file(
                &test_path.to_string_lossy(),
                "Hello, Moxie!",
            )
            .await
            .unwrap();
        assert!(write_result.success);

        // Read
        let read_result = plugin
            .read_file(&test_path.to_string_lossy())
            .await
            .unwrap();
        assert!(read_result.success);
        assert_eq!(read_result.output["content"], "Hello, Moxie!");

        // Cleanup
        fs::remove_file(test_path).await.ok();
    }

    #[tokio::test]
    async fn test_list_directory() {
        let plugin = FilesystemPlugin::new(test_config());

        let result = plugin
            .list_directory(&env::temp_dir().to_string_lossy())
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.output["entries"].is_array());
    }

    #[tokio::test]
    async fn test_path_not_allowed() {
        let plugin = FilesystemPlugin::new(FilesystemConfig {
            allowed_paths: vec![PathBuf::from("/allowed/path")],
            allow_write: true,
            max_file_size: 1024,
        });

        let result = plugin
            .read_file("/not/allowed/file.txt")
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.unwrap().contains("Access denied"));
    }
}
