//! Plugin manifest and metadata
//!
//! Defines the metadata structure for plugins, used for:
//! - Plugin discovery and identification
//! - Version compatibility checking
//! - Plugin store listings
//! - Configuration schema definitions

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Semantic version for plugins
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self { major, minor, patch }
    }

    /// Check if this version is compatible with a required version
    /// Uses semver: major must match, minor/patch can be >= required
    pub fn is_compatible_with(&self, required: &Version) -> bool {
        if self.major != required.major {
            return false;
        }
        if self.minor < required.minor {
            return false;
        }
        if self.minor == required.minor && self.patch < required.patch {
            return false;
        }
        true
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl Default for Version {
    fn default() -> Self {
        Self::new(0, 1, 0)
    }
}

/// Plugin category for organization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginCategory {
    /// File system and storage
    Filesystem,
    /// Database integrations
    Database,
    /// Office applications (Excel, Word, etc.)
    Office,
    /// Communication (email, Slack, Teams)
    Communication,
    /// Network tools and diagnostics
    Network,
    /// Hardware integrations
    Hardware,
    /// Knowledge base and RAG
    Knowledge,
    /// Cloud services
    Cloud,
    /// Custom/Other
    Custom,
}

impl Default for PluginCategory {
    fn default() -> Self {
        Self::Custom
    }
}

/// Platform requirements for a plugin
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlatformRequirements {
    /// Supported operating systems (empty = all)
    #[serde(default)]
    pub os: Vec<String>,

    /// Required external dependencies
    #[serde(default)]
    pub dependencies: Vec<String>,

    /// Minimum Moxie version required
    #[serde(default)]
    pub min_moxie_version: Option<Version>,
}

/// Configuration field type for plugin settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigFieldType {
    String,
    Number,
    Boolean,
    StringArray,
    Path,
    PathArray,
    Secret,      // Stored securely, not logged
    Select(Vec<String>),  // Dropdown options
}

/// A configuration field definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigField {
    /// Field name (used in TOML config)
    pub name: String,

    /// Human-readable label
    pub label: String,

    /// Description/help text
    pub description: String,

    /// Field type
    pub field_type: ConfigFieldType,

    /// Whether this field is required
    #[serde(default)]
    pub required: bool,

    /// Default value (as JSON)
    #[serde(default)]
    pub default: Option<serde_json::Value>,

    /// Validation pattern (regex for strings)
    #[serde(default)]
    pub validation: Option<String>,
}

/// Plugin manifest - complete metadata for a plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Unique plugin identifier (e.g., "moxie.filesystem", "com.acme.custom")
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Plugin version
    pub version: Version,

    /// Short description
    pub description: String,

    /// Long description (supports markdown)
    #[serde(default)]
    pub long_description: Option<String>,

    /// Plugin category
    #[serde(default)]
    pub category: PluginCategory,

    /// Author name
    pub author: String,

    /// Author email (optional)
    #[serde(default)]
    pub email: Option<String>,

    /// Homepage/repository URL
    #[serde(default)]
    pub homepage: Option<String>,

    /// License identifier (e.g., "MIT", "Apache-2.0")
    #[serde(default)]
    pub license: Option<String>,

    /// Keywords for search
    #[serde(default)]
    pub keywords: Vec<String>,

    /// Platform requirements
    #[serde(default)]
    pub platform: PlatformRequirements,

    /// Configuration schema
    #[serde(default)]
    pub config_schema: Vec<ConfigField>,

    /// Plugin dependencies (other plugin IDs)
    #[serde(default)]
    pub dependencies: HashMap<String, Version>,

    /// Whether this plugin requires confirmation for dangerous operations
    #[serde(default)]
    pub requires_confirmation: bool,

    /// Icon URL or base64 data (for UI)
    #[serde(default)]
    pub icon: Option<String>,
}

impl PluginManifest {
    /// Create a new plugin manifest with required fields
    pub fn new(id: impl Into<String>, name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            version: Version::default(),
            description: description.into(),
            long_description: None,
            category: PluginCategory::default(),
            author: String::new(),
            email: None,
            homepage: None,
            license: None,
            keywords: vec![],
            platform: PlatformRequirements::default(),
            config_schema: vec![],
            dependencies: HashMap::new(),
            requires_confirmation: false,
            icon: None,
        }
    }

    /// Builder pattern methods
    pub fn with_version(mut self, major: u32, minor: u32, patch: u32) -> Self {
        self.version = Version::new(major, minor, patch);
        self
    }

    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = author.into();
        self
    }

    pub fn with_category(mut self, category: PluginCategory) -> Self {
        self.category = category;
        self
    }

    pub fn with_keywords(mut self, keywords: Vec<&str>) -> Self {
        self.keywords = keywords.into_iter().map(String::from).collect();
        self
    }

    pub fn with_config_field(mut self, field: ConfigField) -> Self {
        self.config_schema.push(field);
        self
    }

    pub fn requires_confirmation(mut self) -> Self {
        self.requires_confirmation = true;
        self
    }

    /// Validate the manifest
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() {
            return Err("Plugin ID cannot be empty".into());
        }
        if self.name.is_empty() {
            return Err("Plugin name cannot be empty".into());
        }
        if self.description.is_empty() {
            return Err("Plugin description cannot be empty".into());
        }
        // ID should be a valid identifier (alphanumeric, dots, hyphens)
        if !self.id.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_') {
            return Err("Plugin ID contains invalid characters".into());
        }
        Ok(())
    }
}

/// Builder for creating ConfigField entries
pub struct ConfigFieldBuilder {
    field: ConfigField,
}

impl ConfigFieldBuilder {
    pub fn new(name: impl Into<String>, field_type: ConfigFieldType) -> Self {
        Self {
            field: ConfigField {
                name: name.into(),
                label: String::new(),
                description: String::new(),
                field_type,
                required: false,
                default: None,
                validation: None,
            },
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.field.label = label.into();
        self
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.field.description = desc.into();
        self
    }

    pub fn required(mut self) -> Self {
        self.field.required = true;
        self
    }

    pub fn default_value(mut self, value: serde_json::Value) -> Self {
        self.field.default = Some(value);
        self
    }

    pub fn validation(mut self, pattern: impl Into<String>) -> Self {
        self.field.validation = Some(pattern.into());
        self
    }

    pub fn build(self) -> ConfigField {
        self.field
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_compatibility() {
        let v1 = Version::new(1, 2, 3);
        let v2 = Version::new(1, 2, 0);
        let v3 = Version::new(1, 3, 0);
        let v4 = Version::new(2, 0, 0);

        assert!(v1.is_compatible_with(&v2)); // 1.2.3 >= 1.2.0
        assert!(!v2.is_compatible_with(&v1)); // 1.2.0 < 1.2.3
        assert!(!v1.is_compatible_with(&v3)); // 1.2.3 < 1.3.0
        assert!(!v1.is_compatible_with(&v4)); // Different major
    }

    #[test]
    fn test_manifest_builder() {
        let manifest = PluginManifest::new(
            "moxie.test",
            "Test Plugin",
            "A test plugin",
        )
        .with_version(1, 0, 0)
        .with_author("John Nguyen")
        .with_category(PluginCategory::Custom)
        .with_keywords(vec!["test", "example"]);

        assert_eq!(manifest.id, "moxie.test");
        assert_eq!(manifest.version.major, 1);
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn test_config_field_builder() {
        let field = ConfigFieldBuilder::new("allowed_paths", ConfigFieldType::PathArray)
            .label("Allowed Paths")
            .description("Directories the plugin can access")
            .required()
            .build();

        assert_eq!(field.name, "allowed_paths");
        assert!(field.required);
    }
}
