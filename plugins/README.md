# Moxie AI Plugin Development Guide

Create custom plugins to extend Moxie's capabilities with new tools for AI-powered automation.

## Quick Start

### 1. Create Your Plugin

```rust
use moxie_ai::plugins::prelude::*;

pub struct MyPlugin;

#[async_trait]
impl Plugin for MyPlugin {
    fn manifest(&self) -> PluginManifest {
        PluginManifest::new(
            "com.example.myplugin",  // Unique ID
            "My Plugin",              // Display name
            "Does something useful",  // Description
        )
        .with_version(1, 0, 0)
        .with_author("Your Name")
        .with_category(PluginCategory::Custom)
    }

    fn tools(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition::new("my_tool", "Processes input and returns output")
                .with_parameters(json!({
                    "type": "object",
                    "properties": {
                        "input": { "type": "string" }
                    },
                    "required": ["input"]
                }))
        ]
    }

    async fn execute(&self, tool: &str, params: Value) -> Result<ToolResult, PluginError> {
        match tool {
            "my_tool" => {
                let input = params["input"].as_str().unwrap_or("");
                Ok(ToolResult::success(json!({ "result": input })))
            }
            _ => Err(PluginError::ToolNotFound(tool.to_string()))
        }
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}
```

### 2. Register Your Plugin

```rust
// In main.rs or plugin initialization
let mut loader = PluginLoader::new();
loader.register(MyPlugin::new())?;
loader.init_all().await?;
```

### 3. Configure in Client TOML

```toml
[plugins]
enabled = ["com.example.myplugin"]

[plugins.myplugin]
# Your plugin configuration
```

## Plugin Architecture

### Plugin Trait

Every plugin must implement the `Plugin` trait:

| Method | Required | Description |
|--------|----------|-------------|
| `manifest()` | ✅ | Returns plugin metadata |
| `tools()` | ✅ | Returns available tools |
| `execute()` | ✅ | Executes a tool |
| `on_init()` | ❌ | Called on initialization |
| `on_shutdown()` | ❌ | Called on shutdown |
| `on_enable()` | ❌ | Called when enabled |
| `on_disable()` | ❌ | Called when disabled |
| `before_execute()` | ❌ | Called before tool execution |
| `after_execute()` | ❌ | Called after tool execution |

### Plugin Manifest

```rust
PluginManifest::new("id", "Name", "Description")
    .with_version(1, 0, 0)
    .with_author("Author Name")
    .with_category(PluginCategory::Database)
    .with_keywords(vec!["sql", "database"])
    .with_config_field(
        ConfigFieldBuilder::new("connection_string", ConfigFieldType::Secret)
            .label("Connection String")
            .required()
            .build()
    )
```

### Plugin Categories

| Category | Use Case |
|----------|----------|
| `Filesystem` | File operations |
| `Database` | SQL, NoSQL databases |
| `Office` | Excel, Word, PowerPoint |
| `Communication` | Email, Slack, Teams |
| `Network` | Ping, DNS, monitoring |
| `Hardware` | Scanners, PLCs, sensors |
| `Knowledge` | RAG, document search |
| `Cloud` | AWS, Azure, GCP |
| `Custom` | Everything else |

### Configuration Field Types

| Type | Description |
|------|-------------|
| `String` | Text input |
| `Number` | Numeric value |
| `Boolean` | True/false toggle |
| `StringArray` | List of strings |
| `Path` | File/directory path |
| `PathArray` | List of paths |
| `Secret` | Sensitive data (hidden) |
| `Select(vec)` | Dropdown with options |

## Tool Definition

```rust
ToolDefinition::new("tool_name", "What this tool does")
    .with_parameters(json!({
        "type": "object",
        "properties": {
            "required_param": {
                "type": "string",
                "description": "A required parameter"
            },
            "optional_param": {
                "type": "number",
                "description": "An optional parameter"
            }
        },
        "required": ["required_param"]
    }))
    .with_confirmation()  // Requires user confirmation
    .from_plugin("plugin.id")
```

## Tool Results

```rust
// Success
ToolResult::success(json!({
    "data": "your result",
    "count": 42
}))

// Failure
ToolResult::failure("Error message")

// With metadata
ToolResult::success(data)
    .with_duration(150)  // Execution time in ms
```

## Lifecycle Hooks

```rust
async fn on_init(&mut self, ctx: &PluginContext) -> Result<(), PluginError> {
    // ctx.config - Plugin configuration from TOML
    // ctx.data_dir - Directory for plugin data storage
    // ctx.debug - Whether debug mode is enabled

    self.config = MyConfig::from_value(&ctx.config)?;
    self.connection = connect_to_service().await?;

    Ok(())
}

async fn on_shutdown(&mut self) -> Result<(), PluginError> {
    self.connection.close().await?;
    Ok(())
}
```

## Error Handling

```rust
use moxie_ai::plugins::PluginError;

// Available error types
PluginError::ToolNotFound(name)       // Tool doesn't exist
PluginError::InvalidParameters(msg)   // Bad parameters
PluginError::ExecutionFailed(msg)     // Tool execution failed
PluginError::PluginNotFound(id)       // Plugin not registered
PluginError::PluginDisabled(id)       // Plugin is disabled
PluginError::InitFailed(msg)          // Initialization error
PluginError::ConfigError(msg)         // Configuration error
PluginError::IoError(err)             // I/O operations
PluginError::JsonError(err)           // JSON parsing
```

## Best Practices

### Security

1. **Validate all inputs** - Never trust user input
2. **Sanitize paths** - Prevent directory traversal
3. **Use `requires_confirmation`** - For destructive operations
4. **Don't log secrets** - Mask sensitive data

### Performance

1. **Cache connections** - Reuse database/API connections
2. **Implement timeouts** - Don't hang on slow operations
3. **Stream large data** - Don't load huge files into memory

### Configuration

1. **Provide defaults** - Make setup easy
2. **Validate early** - Check config in `on_init()`
3. **Document options** - Clear descriptions in schema

## Example Plugins

See the built-in plugins for reference:

- [filesystem](../src/plugins/filesystem/mod.rs) - File operations

## Plugin Store (Future)

Plugins will be discoverable via the Moxie Plugin Store:

```toml
# Install from store
[plugins]
install = ["moxie.office", "moxie.database"]

# Or use custom plugins
enabled = ["com.mycompany.custom"]
```

## Template

Use [template/mod.rs.template](template/mod.rs.template) as a starting point for new plugins.
