//! Chat engine with tool calling orchestration
//!
//! The ChatEngine is the core of Moxie's AI capabilities. It:
//! 1. Receives user messages
//! 2. Loads relevant context and memory
//! 3. Sends messages to the LLM with available tools
//! 4. Executes tool calls and feeds results back to the LLM
//! 5. Returns the final response
//! 6. Saves the conversation to memory

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::config::{Config, prompts_builtin};
use crate::conversation::{Message, Role};
use crate::plugins::{PluginError, PluginRegistry, ToolDefinition, ToolResult};
use crate::providers::{Provider, ProviderError};

use super::memory::MemoryStore;

/// Maximum number of tool call iterations to prevent infinite loops
const MAX_TOOL_ITERATIONS: usize = 10;

/// A tool call requested by the LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// Request to the chat engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    /// The user's message
    pub message: String,

    /// Optional conversation ID to continue
    #[serde(default)]
    pub conversation_id: Option<String>,

    /// Optional system prompt override (takes precedence over persona)
    #[serde(default)]
    pub system_prompt: Option<String>,

    /// Optional persona name to use (e.g., "business_analyst")
    /// Built-in options: "default", "business_analyst", "tech_support", "data_entry"
    /// Or load from configs/prompts/{persona}.toml
    #[serde(default)]
    pub persona: Option<String>,

    /// Provider to use (defaults to "ollama")
    #[serde(default = "default_provider")]
    pub provider: String,

    /// Model to use (provider-specific)
    #[serde(default = "default_model")]
    pub model: String,
}

fn default_provider() -> String {
    "ollama".to_string()
}

fn default_model() -> String {
    "llama3.2".to_string()
}

/// Response from the chat engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    /// The assistant's response
    pub message: String,

    /// Conversation ID for continuation
    pub conversation_id: String,

    /// Tools that were called during this response
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCallSummary>,
}

/// Summary of a tool call for the response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallSummary {
    pub name: String,
    pub success: bool,
}

/// Errors from the chat engine
#[derive(Debug, thiserror::Error)]
pub enum ChatError {
    #[error("Provider error: {0}")]
    Provider(#[from] ProviderError),

    #[error("Plugin error: {0}")]
    Plugin(#[from] PluginError),

    #[error("Memory error: {0}")]
    Memory(String),

    #[error("Max tool iterations exceeded")]
    MaxIterationsExceeded,
}

/// The core chat engine
pub struct ChatEngine {
    config: Config,
    plugins: Arc<PluginRegistry>,
    memory: Arc<MemoryStore>,
    system_prompt: String,
}

impl ChatEngine {
    /// Create a new chat engine
    pub fn new(
        config: Config,
        plugins: Arc<PluginRegistry>,
        memory: Arc<MemoryStore>,
    ) -> Self {
        Self {
            config,
            plugins,
            memory,
            system_prompt: default_system_prompt(),
        }
    }

    /// Set a custom system prompt
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = prompt.into();
        self
    }

    /// Resolve a persona name to a system prompt
    /// Supports built-in personas and can be extended to load from files
    fn resolve_persona(&self, persona: &str) -> String {
        match persona.to_lowercase().as_str() {
            "default" => prompts_builtin::DEFAULT.to_string(),
            "business_analyst" | "analyst" => prompts_builtin::BUSINESS_ANALYST.to_string(),
            "tech_support" | "support" => prompts_builtin::TECH_SUPPORT.to_string(),
            "data_entry" | "data" => prompts_builtin::DATA_ENTRY.to_string(),
            // For unknown personas, return default with a note
            _ => format!(
                "{}\n\nNote: Unknown persona '{}', using default.",
                prompts_builtin::DEFAULT,
                persona
            ),
        }
    }

    /// Process a chat request and return a response
    pub async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ChatError> {
        // Get or create conversation ID
        let conversation_id = request
            .conversation_id
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        // Load conversation history from memory
        let history = self
            .memory
            .get_conversation(&conversation_id)
            .await
            .map_err(|e| ChatError::Memory(e.to_string()))?;

        // Build messages array
        let mut messages = Vec::new();

        // Resolve system prompt: explicit > persona > default
        let system_prompt = if let Some(ref prompt) = request.system_prompt {
            prompt.clone()
        } else if let Some(ref persona) = request.persona {
            self.resolve_persona(persona)
        } else {
            self.system_prompt.clone()
        };

        messages.push(Message {
            role: Role::System,
            content: self.build_system_prompt(&system_prompt),
        });

        // Add conversation history
        messages.extend(history);

        // Add new user message
        messages.push(Message {
            role: Role::User,
            content: request.message.clone(),
        });

        // Save user message to memory
        self.memory
            .save_message(&conversation_id, &messages.last().unwrap())
            .await
            .map_err(|e| ChatError::Memory(e.to_string()))?;

        // Create provider
        let provider = Provider::from_name(&request.provider, &self.config)?;

        // Tool calling loop
        let mut tool_calls_made = Vec::new();
        let mut iterations = 0;

        loop {
            iterations += 1;
            if iterations > MAX_TOOL_ITERATIONS {
                return Err(ChatError::MaxIterationsExceeded);
            }

            // Get response from LLM
            let response = provider.chat(&messages, &request.model).await?;

            // Check if the response contains tool calls
            if let Some(tool_calls) = self.extract_tool_calls(&response.content) {
                // Execute each tool call
                for tool_call in tool_calls {
                    let result = self.plugins.execute(&tool_call.name, tool_call.arguments.clone()).await;

                    let tool_result = match result {
                        Ok(r) => r,
                        Err(e) => ToolResult::failure(e.to_string()),
                    };

                    tool_calls_made.push(ToolCallSummary {
                        name: tool_call.name.clone(),
                        success: tool_result.success,
                    });

                    // Add tool call and result to messages
                    messages.push(Message {
                        role: Role::Assistant,
                        content: format!(
                            "Tool call: {} with arguments: {}",
                            tool_call.name,
                            serde_json::to_string_pretty(&tool_call.arguments).unwrap_or_default()
                        ),
                    });

                    messages.push(Message {
                        role: Role::System,
                        content: format!(
                            "Tool result for {}: {}",
                            tool_call.name,
                            serde_json::to_string_pretty(&tool_result).unwrap_or_default()
                        ),
                    });
                }

                // Continue the loop to let the LLM respond to tool results
                continue;
            }

            // No tool calls - this is the final response
            // Save assistant message to memory
            self.memory
                .save_message(&conversation_id, &response)
                .await
                .map_err(|e| ChatError::Memory(e.to_string()))?;

            return Ok(ChatResponse {
                message: response.content,
                conversation_id,
                tool_calls: tool_calls_made,
            });
        }
    }

    /// Build the system prompt with tool information
    fn build_system_prompt(&self, base_prompt: &str) -> String {
        let tools = self.plugins.all_tools();

        if tools.is_empty() {
            return base_prompt.to_string();
        }

        let tools_description = tools
            .iter()
            .map(|t| format!("- {}: {}", t.name, t.description))
            .collect::<Vec<_>>()
            .join("\n");

        let tools_json = serde_json::to_string_pretty(&tools).unwrap_or_default();

        format!(
            "{}\n\n## Available Tools\n\nYou have access to the following tools:\n\n{}\n\n\
            To use a tool, respond with a JSON block in this format:\n\
            ```tool_call\n{{\n  \"name\": \"tool_name\",\n  \"arguments\": {{}}\n}}\n```\n\n\
            Tool schemas:\n```json\n{}\n```",
            base_prompt, tools_description, tools_json
        )
    }

    /// Extract tool calls from an LLM response
    fn extract_tool_calls(&self, content: &str) -> Option<Vec<ToolCall>> {
        // Look for ```tool_call blocks
        let mut calls = Vec::new();

        for block in content.split("```tool_call") {
            if let Some(end) = block.find("```") {
                let json_str = &block[..end].trim();
                if let Ok(call) = serde_json::from_str::<Value>(json_str) {
                    if let (Some(name), Some(arguments)) = (
                        call.get("name").and_then(|n| n.as_str()),
                        call.get("arguments"),
                    ) {
                        calls.push(ToolCall {
                            id: uuid::Uuid::new_v4().to_string(),
                            name: name.to_string(),
                            arguments: arguments.clone(),
                        });
                    }
                }
            }
        }

        if calls.is_empty() {
            None
        } else {
            Some(calls)
        }
    }

    /// Get all available tools
    pub fn available_tools(&self) -> Vec<ToolDefinition> {
        self.plugins.all_tools()
    }
}

fn default_system_prompt() -> String {
    prompts_builtin::DEFAULT.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to extract tool calls without needing a full ChatEngine
    fn extract_tool_calls_helper(content: &str) -> Option<Vec<ToolCall>> {
        // Look for ```tool_call blocks
        let mut calls = Vec::new();

        for block in content.split("```tool_call") {
            if let Some(end) = block.find("```") {
                let json_str = &block[..end].trim();
                if let Ok(call) = serde_json::from_str::<Value>(json_str) {
                    if let (Some(name), Some(arguments)) = (
                        call.get("name").and_then(|n| n.as_str()),
                        call.get("arguments"),
                    ) {
                        calls.push(ToolCall {
                            id: uuid::Uuid::new_v4().to_string(),
                            name: name.to_string(),
                            arguments: arguments.clone(),
                        });
                    }
                }
            }
        }

        if calls.is_empty() {
            None
        } else {
            Some(calls)
        }
    }

    #[test]
    fn test_extract_tool_calls() {
        let content = r#"I'll read that file for you.

```tool_call
{
  "name": "read_file",
  "arguments": {
    "path": "/tmp/test.txt"
  }
}
```"#;

        let calls = extract_tool_calls_helper(content);
        assert!(calls.is_some());
        let calls = calls.unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read_file");
    }

    #[test]
    fn test_no_tool_calls() {
        let content = "Just a regular response with no tool calls.";
        let calls = extract_tool_calls_helper(content);
        assert!(calls.is_none());
    }
}
