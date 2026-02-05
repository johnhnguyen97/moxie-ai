//! OpenAI-compatible provider
//!
//! Works with any API that implements the OpenAI chat completions format:
//! - OpenAI (api.openai.com)
//! - Groq (api.groq.com)
//! - vLLM (local server)
//! - LM Studio (local server)
//! - LocalAI (local server)
//! - Together AI
//! - Fireworks AI
//! - And many more...
//!
//! # Configuration
//!
//! ```toml
//! [llm]
//! provider = "openai"
//! base_url = "https://api.openai.com/v1"  # or Groq, vLLM, etc.
//! api_key_env = "OPENAI_API_KEY"
//! model = "gpt-4o-mini"
//! ```

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::conversation::{Message, Role};

use super::ProviderError;

/// OpenAI-compatible chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

impl From<&Message> for ChatMessage {
    fn from(msg: &Message) -> Self {
        Self {
            role: match msg.role {
                Role::System => "system".to_string(),
                Role::User => "user".to_string(),
                Role::Assistant => "assistant".to_string(),
            },
            content: msg.content.clone(),
        }
    }
}

/// Tool definition for function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionDef,
}

/// Function definition for tool calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Value>,
}

/// Chat completion request
#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolDef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
}

/// Chat completion response
#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ResponseMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    role: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ToolCallResponse>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolCallResponse {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCallResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FunctionCallResponse {
    pub name: String,
    pub arguments: String, // JSON string of arguments
}

#[derive(Debug, Deserialize)]
struct Usage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

/// Error response from API
#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: ApiError,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    message: String,
    #[serde(rename = "type")]
    error_type: Option<String>,
}

/// OpenAI-compatible provider configuration
#[derive(Debug, Clone)]
pub struct OpenAICompatConfig {
    /// Base URL for the API (e.g., https://api.openai.com/v1)
    pub base_url: String,
    /// API key (optional for local servers)
    pub api_key: Option<String>,
    /// Default model to use
    pub default_model: String,
    /// Optional organization ID (OpenAI)
    pub organization: Option<String>,
    /// Request timeout in seconds
    pub timeout_secs: u64,
}

impl Default for OpenAICompatConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: None,
            default_model: "gpt-4o-mini".to_string(),
            organization: None,
            timeout_secs: 120,
        }
    }
}

impl OpenAICompatConfig {
    /// Create config for OpenAI
    pub fn openai(api_key: impl Into<String>) -> Self {
        Self {
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: Some(api_key.into()),
            default_model: "gpt-4o-mini".to_string(),
            organization: None,
            timeout_secs: 120,
        }
    }

    /// Create config for Groq
    pub fn groq(api_key: impl Into<String>) -> Self {
        Self {
            base_url: "https://api.groq.com/openai/v1".to_string(),
            api_key: Some(api_key.into()),
            default_model: "llama-3.3-70b-versatile".to_string(),
            organization: None,
            timeout_secs: 60,
        }
    }

    /// Create config for a local server (vLLM, LM Studio, etc.)
    pub fn local(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: None,
            default_model: model.into(),
            organization: None,
            timeout_secs: 300, // Local inference can be slower
        }
    }
}

/// OpenAI-compatible API provider
pub struct OpenAICompatProvider {
    config: OpenAICompatConfig,
    client: Client,
}

impl OpenAICompatProvider {
    /// Create a new provider with the given configuration
    pub fn new(config: OpenAICompatConfig) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .expect("Failed to build HTTP client");

        Self { config, client }
    }

    /// Create provider for OpenAI
    pub fn openai(api_key: impl Into<String>) -> Self {
        Self::new(OpenAICompatConfig::openai(api_key))
    }

    /// Create provider for Groq
    pub fn groq(api_key: impl Into<String>) -> Self {
        Self::new(OpenAICompatConfig::groq(api_key))
    }

    /// Create provider for local server
    pub fn local(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self::new(OpenAICompatConfig::local(base_url, model))
    }

    /// Send a chat completion request
    pub async fn chat(&self, messages: &[Message], model: &str) -> Result<Message, ProviderError> {
        self.chat_with_tools(messages, model, None).await
    }

    /// Send a chat completion request with tools
    pub async fn chat_with_tools(
        &self,
        messages: &[Message],
        model: &str,
        tools: Option<Vec<ToolDef>>,
    ) -> Result<Message, ProviderError> {
        let url = format!("{}/chat/completions", self.config.base_url);

        let chat_messages: Vec<ChatMessage> = messages.iter().map(ChatMessage::from).collect();

        let request = ChatCompletionRequest {
            model: if model.is_empty() {
                self.config.default_model.clone()
            } else {
                model.to_string()
            },
            messages: chat_messages,
            temperature: Some(0.7),
            max_tokens: Some(4096),
            tools,
            tool_choice: None,
        };

        let mut req_builder = self.client.post(&url);

        // Add authorization if API key is provided
        if let Some(ref api_key) = self.config.api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key));
        }

        // Add organization header if provided (OpenAI specific)
        if let Some(ref org) = self.config.organization {
            req_builder = req_builder.header("OpenAI-Organization", org);
        }

        let response = req_builder
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            // Try to parse error response
            if let Ok(error_resp) = serde_json::from_str::<ErrorResponse>(&body) {
                return Err(ProviderError::InvalidResponse(format!(
                    "API error: {}",
                    error_resp.error.message
                )));
            }
            return Err(ProviderError::InvalidResponse(format!(
                "HTTP {}: {}",
                status, body
            )));
        }

        let completion: ChatCompletionResponse = serde_json::from_str(&body).map_err(|e| {
            ProviderError::InvalidResponse(format!("Failed to parse response: {} - Body: {}", e, body))
        })?;

        let choice = completion
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| ProviderError::InvalidResponse("No choices in response".to_string()))?;

        // Handle tool calls if present
        if let Some(tool_calls) = choice.message.tool_calls {
            // Format tool calls in a way our chat engine can parse
            let tool_calls_str = tool_calls
                .iter()
                .map(|tc| {
                    format!(
                        "```tool_call\n{{\n  \"name\": \"{}\",\n  \"arguments\": {}\n}}\n```",
                        tc.function.name, tc.function.arguments
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n");

            return Ok(Message {
                role: Role::Assistant,
                content: tool_calls_str,
            });
        }

        // Regular text response
        let content = choice.message.content.unwrap_or_default();

        Ok(Message {
            role: Role::Assistant,
            content,
        })
    }

    /// List available models (if supported by the API)
    pub async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        let url = format!("{}/models", self.config.base_url);

        let mut req_builder = self.client.get(&url);

        if let Some(ref api_key) = self.config.api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = req_builder.send().await?;

        if !response.status().is_success() {
            return Ok(vec![]); // Some servers don't support model listing
        }

        let body: Value = response.json().await?;

        let models = body["data"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m["id"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        Ok(models)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_presets() {
        let openai = OpenAICompatConfig::openai("test-key");
        assert!(openai.base_url.contains("openai.com"));
        assert_eq!(openai.api_key, Some("test-key".to_string()));

        let groq = OpenAICompatConfig::groq("groq-key");
        assert!(groq.base_url.contains("groq.com"));

        let local = OpenAICompatConfig::local("http://localhost:8000/v1", "llama-3");
        assert!(local.api_key.is_none());
        assert_eq!(local.default_model, "llama-3");
    }

    #[test]
    fn test_message_conversion() {
        let msg = Message {
            role: Role::User,
            content: "Hello".to_string(),
        };
        let chat_msg = ChatMessage::from(&msg);
        assert_eq!(chat_msg.role, "user");
        assert_eq!(chat_msg.content, "Hello");
    }
}
