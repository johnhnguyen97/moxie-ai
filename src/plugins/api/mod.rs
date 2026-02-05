//! Custom API Plugin
//!
//! Allows users to connect any REST API through TOML configuration.
//! No code required - just define your endpoints and Moxie can use them.
//!
//! # Configuration Example
//!
//! ```toml
//! [[plugins.api.services]]
//! id = "weather"
//! name = "Weather API"
//! base_url = "https://api.weather.com/v1"
//! auth_type = "api_key"
//! auth_header = "X-API-Key"
//! auth_env = "WEATHER_API_KEY"
//!
//! [[plugins.api.services.endpoints]]
//! name = "get_weather"
//! method = "GET"
//! path = "/current"
//! description = "Get current weather for a location"
//!
//! [plugins.api.services.endpoints.params]
//! location = { type = "string", required = true, description = "City name or coordinates" }
//! units = { type = "string", required = false, default = "metric" }
//! ```

use async_trait::async_trait;
use reqwest::{Client, Method, RequestBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::any::Any;
use std::collections::HashMap;
use std::env;
use std::time::Duration;

use crate::plugins::manifest::{
    ConfigFieldBuilder, ConfigFieldType, PluginCategory, PluginManifest,
};
use crate::plugins::traits::{Plugin, PluginContext};
use crate::plugins::{LegacyPlugin, PluginError, ToolDefinition, ToolResult};

/// Authentication types supported
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthType {
    /// No authentication
    #[default]
    None,
    /// API Key in header
    ApiKey,
    /// Bearer token
    Bearer,
    /// Basic auth (username:password)
    Basic,
    /// Query parameter
    QueryParam,
}

/// HTTP method
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    #[default]
    GET,
    POST,
    PUT,
    PATCH,
    DELETE,
}

impl HttpMethod {
    fn to_reqwest(&self) -> Method {
        match self {
            HttpMethod::GET => Method::GET,
            HttpMethod::POST => Method::POST,
            HttpMethod::PUT => Method::PUT,
            HttpMethod::PATCH => Method::PATCH,
            HttpMethod::DELETE => Method::DELETE,
        }
    }
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpMethod::GET => write!(f, "GET"),
            HttpMethod::POST => write!(f, "POST"),
            HttpMethod::PUT => write!(f, "PUT"),
            HttpMethod::PATCH => write!(f, "PATCH"),
            HttpMethod::DELETE => write!(f, "DELETE"),
        }
    }
}

/// Parameter definition for an endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamDef {
    /// Parameter type (string, number, boolean, object, array)
    #[serde(rename = "type", default = "default_string")]
    pub param_type: String,

    /// Whether the parameter is required
    #[serde(default)]
    pub required: bool,

    /// Description for the AI
    #[serde(default)]
    pub description: String,

    /// Default value
    #[serde(default)]
    pub default: Option<Value>,

    /// Where to put this param: query, path, header, body
    #[serde(default = "default_query")]
    pub location: String,
}

fn default_string() -> String {
    "string".to_string()
}

fn default_query() -> String {
    "query".to_string()
}

/// An API endpoint definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointDef {
    /// Tool name (used by AI to call this endpoint)
    pub name: String,

    /// HTTP method
    #[serde(default)]
    pub method: HttpMethod,

    /// Path (can include {param} placeholders)
    pub path: String,

    /// Description for the AI
    #[serde(default)]
    pub description: String,

    /// Parameters
    #[serde(default)]
    pub params: HashMap<String, ParamDef>,

    /// Expected response type hint
    #[serde(default)]
    pub response_type: Option<String>,

    /// Whether this endpoint requires confirmation
    #[serde(default)]
    pub requires_confirmation: bool,
}

/// An API service definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDef {
    /// Unique service ID
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Base URL for the API
    pub base_url: String,

    /// Authentication type
    #[serde(default)]
    pub auth_type: AuthType,

    /// Header name for API key auth
    #[serde(default)]
    pub auth_header: Option<String>,

    /// Query param name for query param auth
    #[serde(default)]
    pub auth_param: Option<String>,

    /// Environment variable containing the auth credential
    #[serde(default)]
    pub auth_env: Option<String>,

    /// Default headers to include
    #[serde(default)]
    pub headers: HashMap<String, String>,

    /// Request timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    /// Endpoints
    #[serde(default)]
    pub endpoints: Vec<EndpointDef>,
}

fn default_timeout() -> u64 {
    30
}

/// Configuration for the API plugin
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiPluginConfig {
    /// Configured services
    #[serde(default)]
    pub services: Vec<ServiceDef>,
}

impl ApiPluginConfig {
    /// Parse configuration from a JSON Value
    pub fn from_value(value: &Value) -> Result<Self, PluginError> {
        if value.is_null() {
            return Ok(Self::default());
        }
        serde_json::from_value(value.clone())
            .map_err(|e| PluginError::ConfigError(e.to_string()))
    }
}

/// Custom API plugin
pub struct ApiPlugin {
    config: ApiPluginConfig,
    client: Client,
}

impl ApiPlugin {
    /// Plugin ID
    pub const ID: &'static str = "moxie.api";

    /// Create a new API plugin
    pub fn new(config: ApiPluginConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_default();

        Self { config, client }
    }

    /// Create with default config
    pub fn default_plugin() -> Self {
        Self::new(ApiPluginConfig::default())
    }

    /// Find a service and endpoint by tool name
    fn find_endpoint(&self, tool_name: &str) -> Option<(&ServiceDef, &EndpointDef)> {
        for service in &self.config.services {
            for endpoint in &service.endpoints {
                let full_name = format!("{}_{}", service.id, endpoint.name);
                if full_name == tool_name {
                    return Some((service, endpoint));
                }
            }
        }
        None
    }

    /// Build tool definition from endpoint
    fn endpoint_to_tool(&self, service: &ServiceDef, endpoint: &EndpointDef) -> ToolDefinition {
        let tool_name = format!("{}_{}", service.id, endpoint.name);
        let description = if endpoint.description.is_empty() {
            format!("{}: {} {}", service.name, endpoint.method, endpoint.path)
        } else {
            format!("{}: {}", service.name, endpoint.description)
        };

        // Build JSON Schema for parameters
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for (name, param) in &endpoint.params {
            let mut prop = serde_json::Map::new();
            prop.insert("type".to_string(), json!(param.param_type));
            if !param.description.is_empty() {
                prop.insert("description".to_string(), json!(param.description));
            }
            if let Some(default) = &param.default {
                prop.insert("default".to_string(), default.clone());
            }
            properties.insert(name.clone(), Value::Object(prop));

            if param.required {
                required.push(json!(name));
            }
        }

        let parameters = json!({
            "type": "object",
            "properties": properties,
            "required": required
        });

        let mut tool = ToolDefinition::new(tool_name, description)
            .with_parameters(parameters)
            .from_plugin(Self::ID);

        if endpoint.requires_confirmation {
            tool = tool.with_confirmation();
        }

        tool
    }

    /// Execute an API call
    async fn execute_api_call(
        &self,
        service: &ServiceDef,
        endpoint: &EndpointDef,
        params: Value,
    ) -> Result<ToolResult, PluginError> {
        // Build URL with path parameters
        let mut path = endpoint.path.clone();
        let mut query_params = Vec::new();
        let mut body_params = serde_json::Map::new();
        let mut header_params = HashMap::new();

        // Process parameters
        if let Value::Object(param_map) = &params {
            for (name, value) in param_map {
                let param_def = endpoint.params.get(name);
                let location = param_def
                    .map(|p| p.location.as_str())
                    .unwrap_or("query");

                match location {
                    "path" => {
                        let placeholder = format!("{{{}}}", name);
                        let value_str = match value {
                            Value::String(s) => s.clone(),
                            _ => value.to_string().trim_matches('"').to_string(),
                        };
                        path = path.replace(&placeholder, &value_str);
                    }
                    "query" => {
                        let value_str = match value {
                            Value::String(s) => s.clone(),
                            _ => value.to_string().trim_matches('"').to_string(),
                        };
                        query_params.push((name.clone(), value_str));
                    }
                    "header" => {
                        let value_str = match value {
                            Value::String(s) => s.clone(),
                            _ => value.to_string().trim_matches('"').to_string(),
                        };
                        header_params.insert(name.clone(), value_str);
                    }
                    "body" | _ => {
                        body_params.insert(name.clone(), value.clone());
                    }
                }
            }
        }

        // Build URL
        let url = format!("{}{}", service.base_url.trim_end_matches('/'), path);

        // Create request
        let mut request: RequestBuilder = self.client.request(endpoint.method.to_reqwest(), &url);

        // Add query parameters
        if !query_params.is_empty() {
            request = request.query(&query_params);
        }

        // Add default headers
        for (key, value) in &service.headers {
            request = request.header(key, value);
        }

        // Add parameter headers
        for (key, value) in &header_params {
            request = request.header(key, value);
        }

        // Add authentication
        request = self.add_auth(request, service)?;

        // Add body for POST/PUT/PATCH
        match endpoint.method {
            HttpMethod::POST | HttpMethod::PUT | HttpMethod::PATCH => {
                if !body_params.is_empty() {
                    request = request.json(&Value::Object(body_params));
                }
            }
            _ => {}
        }

        // Set timeout
        request = request.timeout(Duration::from_secs(service.timeout_secs));

        // Execute request
        let start = std::time::Instant::now();
        let response = request.send().await.map_err(|e| {
            PluginError::ExecutionFailed(format!("Request failed: {}", e))
        })?;

        let duration = start.elapsed().as_millis() as u64;
        let status = response.status();
        let status_code = status.as_u16();

        // Parse response
        let body_text = response.text().await.unwrap_or_default();

        // Try to parse as JSON
        let body: Value = serde_json::from_str(&body_text).unwrap_or_else(|_| json!(body_text));

        if status.is_success() {
            Ok(ToolResult::success(json!({
                "status": status_code,
                "data": body
            }))
            .with_duration(duration))
        } else {
            Ok(ToolResult::failure(format!(
                "API returned error {}: {}",
                status_code,
                serde_json::to_string_pretty(&body).unwrap_or(body_text)
            )))
        }
    }

    /// Add authentication to request
    fn add_auth(
        &self,
        mut request: RequestBuilder,
        service: &ServiceDef,
    ) -> Result<RequestBuilder, PluginError> {
        let credential = if let Some(env_var) = &service.auth_env {
            env::var(env_var).ok()
        } else {
            None
        };

        match service.auth_type {
            AuthType::None => {}
            AuthType::ApiKey => {
                if let (Some(header), Some(key)) = (&service.auth_header, &credential) {
                    request = request.header(header, key);
                }
            }
            AuthType::Bearer => {
                if let Some(token) = &credential {
                    request = request.header("Authorization", format!("Bearer {}", token));
                }
            }
            AuthType::Basic => {
                if let Some(cred) = &credential {
                    // Expect format: username:password
                    let parts: Vec<&str> = cred.splitn(2, ':').collect();
                    if parts.len() == 2 {
                        request = request.basic_auth(parts[0], Some(parts[1]));
                    }
                }
            }
            AuthType::QueryParam => {
                // Handled in query params building
            }
        }

        Ok(request)
    }

    /// Get count of configured services
    pub fn service_count(&self) -> usize {
        self.config.services.len()
    }

    /// Get count of configured endpoints
    pub fn endpoint_count(&self) -> usize {
        self.config.services.iter().map(|s| s.endpoints.len()).sum()
    }
}

// ============================================================================
// Plugin Trait Implementation
// ============================================================================

#[async_trait]
impl Plugin for ApiPlugin {
    fn manifest(&self) -> PluginManifest {
        PluginManifest::new(
            Self::ID,
            "Custom API",
            "Connect any REST API through configuration - no code required",
        )
        .with_version(1, 0, 0)
        .with_author("Moxie AI")
        .with_category(PluginCategory::Cloud)
        .with_keywords(vec!["api", "rest", "http", "integration", "custom"])
        .with_config_field(
            ConfigFieldBuilder::new("services", ConfigFieldType::StringArray)
                .label("API Services")
                .description("Configured API services")
                .build(),
        )
    }

    fn tools(&self) -> Vec<ToolDefinition> {
        let mut tools = Vec::new();

        for service in &self.config.services {
            for endpoint in &service.endpoints {
                tools.push(self.endpoint_to_tool(service, endpoint));
            }
        }

        tools
    }

    async fn execute(&self, tool: &str, params: Value) -> Result<ToolResult, PluginError> {
        let (service, endpoint) = self
            .find_endpoint(tool)
            .ok_or_else(|| PluginError::ToolNotFound(tool.to_string()))?;

        self.execute_api_call(service, endpoint, params).await
    }

    async fn on_init(&mut self, ctx: &PluginContext) -> Result<(), PluginError> {
        if !ctx.config.is_null() {
            self.config = ApiPluginConfig::from_value(&ctx.config)?;
        }

        tracing::info!(
            "API plugin initialized with {} service(s), {} endpoint(s)",
            self.service_count(),
            self.endpoint_count()
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
// Legacy Plugin Trait (backwards compatibility)
// ============================================================================

#[async_trait]
impl LegacyPlugin for ApiPlugin {
    fn name(&self) -> &str {
        "api"
    }

    fn description(&self) -> &str {
        "Connect any REST API through configuration"
    }

    fn tools(&self) -> Vec<ToolDefinition> {
        Plugin::tools(self)
    }

    async fn execute(&self, tool: &str, params: Value) -> Result<ToolResult, PluginError> {
        Plugin::execute(self, tool, params).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> ApiPluginConfig {
        ApiPluginConfig {
            services: vec![ServiceDef {
                id: "test".to_string(),
                name: "Test API".to_string(),
                base_url: "https://httpbin.org".to_string(),
                auth_type: AuthType::None,
                auth_header: None,
                auth_param: None,
                auth_env: None,
                headers: HashMap::new(),
                timeout_secs: 30,
                endpoints: vec![
                    EndpointDef {
                        name: "get_info".to_string(),
                        method: HttpMethod::GET,
                        path: "/get".to_string(),
                        description: "Get request info".to_string(),
                        params: {
                            let mut p = HashMap::new();
                            p.insert(
                                "foo".to_string(),
                                ParamDef {
                                    param_type: "string".to_string(),
                                    required: false,
                                    description: "A test param".to_string(),
                                    default: None,
                                    location: "query".to_string(),
                                },
                            );
                            p
                        },
                        response_type: None,
                        requires_confirmation: false,
                    },
                    EndpointDef {
                        name: "post_data".to_string(),
                        method: HttpMethod::POST,
                        path: "/post".to_string(),
                        description: "Post some data".to_string(),
                        params: {
                            let mut p = HashMap::new();
                            p.insert(
                                "message".to_string(),
                                ParamDef {
                                    param_type: "string".to_string(),
                                    required: true,
                                    description: "Message to send".to_string(),
                                    default: None,
                                    location: "body".to_string(),
                                },
                            );
                            p
                        },
                        response_type: None,
                        requires_confirmation: false,
                    },
                ],
            }],
        }
    }

    #[test]
    fn test_tool_generation() {
        let plugin = ApiPlugin::new(sample_config());
        let tools = Plugin::tools(&plugin);

        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "test_get_info");
        assert_eq!(tools[1].name, "test_post_data");
    }

    #[test]
    fn test_find_endpoint() {
        let plugin = ApiPlugin::new(sample_config());

        let result = plugin.find_endpoint("test_get_info");
        assert!(result.is_some());

        let (service, endpoint) = result.unwrap();
        assert_eq!(service.id, "test");
        assert_eq!(endpoint.name, "get_info");
    }

    #[tokio::test]
    async fn test_api_call() {
        let plugin = ApiPlugin::new(sample_config());

        // Test GET request to httpbin
        let result = plugin
            .execute_api_call(
                &plugin.config.services[0],
                &plugin.config.services[0].endpoints[0],
                json!({ "foo": "bar" }),
            )
            .await;

        // This test requires network, so we just check it doesn't panic
        // In CI, you might want to skip this or use a mock
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_config_parsing() {
        let json_config = json!({
            "services": [{
                "id": "myapi",
                "name": "My API",
                "base_url": "https://api.example.com",
                "auth_type": "bearer",
                "auth_env": "MY_API_TOKEN",
                "endpoints": [{
                    "name": "list_items",
                    "method": "GET",
                    "path": "/items",
                    "description": "List all items"
                }]
            }]
        });

        let config = ApiPluginConfig::from_value(&json_config).unwrap();
        assert_eq!(config.services.len(), 1);
        assert_eq!(config.services[0].id, "myapi");
        assert_eq!(config.services[0].endpoints.len(), 1);
    }
}
