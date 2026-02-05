//! Client-specific configuration loaded from TOML files
//!
//! Each deployed Moxie agent has a client configuration that defines:
//! - Which plugins are enabled
//! - Plugin-specific settings
//! - LLM provider configuration
//! - Security settings

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Root client configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    /// Client information
    pub client: ClientInfo,

    /// LLM provider settings
    #[serde(default)]
    pub llm: LlmConfig,

    /// Plugin configuration
    #[serde(default)]
    pub plugins: PluginsConfig,

    /// Knowledge base configuration
    #[serde(default)]
    pub knowledge: KnowledgeConfig,

    /// Security settings
    #[serde(default)]
    pub security: SecurityConfig,

    /// Telemetry settings (for RMM dashboard)
    #[serde(default)]
    pub telemetry: TelemetryConfig,
}

impl ClientConfig {
    /// Load configuration from a TOML file
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let config: ClientConfig = toml::from_str(&content)?;
        Ok(config)
    }

    /// Load configuration from a TOML string
    pub fn from_str(content: &str) -> Result<Self, ConfigError> {
        let config: ClientConfig = toml::from_str(content)?;
        Ok(config)
    }
}

/// Client identification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    /// Client name
    pub name: String,

    /// Industry vertical (for templates)
    #[serde(default)]
    pub industry: Option<String>,

    /// Unique client ID
    #[serde(default)]
    pub id: Option<String>,
}

/// LLM provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Provider name: "ollama", "openai", "anthropic"
    #[serde(default = "default_provider")]
    pub provider: String,

    /// Model to use
    #[serde(default = "default_model")]
    pub model: String,

    /// API key environment variable name (for cloud providers)
    #[serde(default)]
    pub api_key_env: Option<String>,

    /// Custom API endpoint
    #[serde(default)]
    pub endpoint: Option<String>,
}

fn default_provider() -> String {
    "ollama".to_string()
}

fn default_model() -> String {
    "llama3.2".to_string()
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            model: default_model(),
            api_key_env: None,
            endpoint: None,
        }
    }
}

/// Plugin configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginsConfig {
    /// List of enabled plugins
    #[serde(default)]
    pub enabled: Vec<String>,

    /// Office plugin settings
    #[serde(default)]
    pub office: Option<OfficePluginConfig>,

    /// Filesystem plugin settings
    #[serde(default)]
    pub filesystem: Option<FilesystemPluginConfig>,

    /// Database plugin settings
    #[serde(default)]
    pub database: Option<DatabasePluginConfig>,

    /// Custom plugin settings (key-value pairs)
    #[serde(default, flatten)]
    pub custom: HashMap<String, toml::Value>,
}

/// Office plugin configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficePluginConfig {
    #[serde(default = "default_true")]
    pub excel_enabled: bool,

    #[serde(default = "default_true")]
    pub word_enabled: bool,

    #[serde(default)]
    pub powerpoint_enabled: bool,

    #[serde(default)]
    pub outlook_enabled: bool,
}

fn default_true() -> bool {
    true
}

impl Default for OfficePluginConfig {
    fn default() -> Self {
        Self {
            excel_enabled: true,
            word_enabled: true,
            powerpoint_enabled: false,
            outlook_enabled: false,
        }
    }
}

/// Filesystem plugin configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemPluginConfig {
    /// Allowed file paths
    #[serde(default)]
    pub allowed_paths: Vec<PathBuf>,

    /// Cloud storage providers to enable
    #[serde(default)]
    pub cloud_providers: Vec<String>,

    /// Whether write operations are allowed
    #[serde(default)]
    pub allow_write: bool,
}

impl Default for FilesystemPluginConfig {
    fn default() -> Self {
        Self {
            allowed_paths: vec![],
            cloud_providers: vec![],
            allow_write: false,
        }
    }
}

/// Database plugin configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabasePluginConfig {
    /// Database connections
    #[serde(default)]
    pub connections: Vec<DatabaseConnection>,

    /// Allowed operations: "read", "write"
    #[serde(default)]
    pub allowed_operations: Vec<String>,
}

impl Default for DatabasePluginConfig {
    fn default() -> Self {
        Self {
            connections: vec![],
            allowed_operations: vec!["read".to_string()],
        }
    }
}

/// Database connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConnection {
    /// Connection name
    pub name: String,

    /// Database type: "sqlite", "postgres", "mysql", "sqlserver"
    #[serde(rename = "type")]
    pub db_type: String,

    /// Environment variable containing the connection string
    pub connection_string_env: String,
}

/// Knowledge base configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KnowledgeConfig {
    /// Whether the knowledge base is enabled
    #[serde(default)]
    pub enabled: bool,

    /// Document sources
    #[serde(default)]
    pub sources: Vec<KnowledgeSource>,
}

/// Knowledge source configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSource {
    /// Path to the source
    pub path: PathBuf,

    /// Source type: "directory", "file", "url"
    #[serde(rename = "type")]
    pub source_type: String,

    /// File patterns to include (for directories)
    #[serde(default)]
    pub patterns: Vec<String>,
}

/// Security settings
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Tools that require user confirmation before execution
    #[serde(default)]
    pub require_confirmation_for: Vec<String>,

    /// Path for audit logging
    #[serde(default)]
    pub audit_log_path: Option<PathBuf>,

    /// Whether to log all tool calls
    #[serde(default)]
    pub log_tool_calls: bool,

    /// Maximum tokens per request (rate limiting)
    #[serde(default)]
    pub max_tokens_per_request: Option<u32>,
}

/// Telemetry configuration for RMM dashboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    /// Whether telemetry is enabled
    #[serde(default)]
    pub enabled: bool,

    /// Dashboard URL
    #[serde(default)]
    pub dashboard_url: Option<String>,

    /// API key environment variable
    #[serde(default)]
    pub api_key_env: Option<String>,

    /// Send system metrics (CPU, memory, uptime)
    #[serde(default = "default_true")]
    pub send_metrics: bool,

    /// Send usage stats (conversation counts, token usage)
    #[serde(default = "default_true")]
    pub send_usage: bool,

    /// Send sanitized error reports
    #[serde(default = "default_true")]
    pub send_errors: bool,

    /// NEVER send actual conversation content
    #[serde(default)]
    pub send_conversations: bool,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            dashboard_url: None,
            api_key_env: None,
            send_metrics: true,
            send_usage: true,
            send_errors: true,
            send_conversations: false, // Always false by default
        }
    }
}

/// Configuration errors
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("Validation error: {0}")]
    Validation(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_CONFIG: &str = r#"
[client]
name = "ACME Corporation"
industry = "manufacturing"

[llm]
provider = "ollama"
model = "llama3.2"

[plugins]
enabled = ["filesystem", "database"]

[plugins.filesystem]
allowed_paths = ["C:\\Data", "D:\\Reports"]
allow_write = false

[plugins.database]
allowed_operations = ["read"]

[[plugins.database.connections]]
name = "production"
type = "postgres"
connection_string_env = "PROD_DB"

[security]
require_confirmation_for = ["write_file", "sql_update"]
log_tool_calls = true

[telemetry]
enabled = true
dashboard_url = "https://dashboard.moxie.ai"
api_key_env = "MOXIE_DASHBOARD_KEY"
send_conversations = false
"#;

    #[test]
    fn test_parse_config() {
        let config = ClientConfig::from_str(SAMPLE_CONFIG).unwrap();

        assert_eq!(config.client.name, "ACME Corporation");
        assert_eq!(config.client.industry, Some("manufacturing".to_string()));
        assert_eq!(config.llm.provider, "ollama");
        assert_eq!(config.plugins.enabled, vec!["filesystem", "database"]);

        let fs_config = config.plugins.filesystem.unwrap();
        assert!(!fs_config.allow_write);

        let db_config = config.plugins.database.unwrap();
        assert_eq!(db_config.connections.len(), 1);
        assert_eq!(db_config.connections[0].name, "production");

        assert!(config.telemetry.enabled);
        assert!(!config.telemetry.send_conversations);
    }

    #[test]
    fn test_minimal_config() {
        let minimal = r#"
[client]
name = "Test Client"
"#;

        let config = ClientConfig::from_str(minimal).unwrap();
        assert_eq!(config.client.name, "Test Client");
        assert_eq!(config.llm.provider, "ollama"); // Default
        assert!(config.plugins.enabled.is_empty());
    }
}
