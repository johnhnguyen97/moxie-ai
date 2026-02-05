//! API routes

use axum::{
    extract::State,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};

use crate::conversation::Message;
use crate::core::{ChatRequest as EngineChatRequest, ChatResponse as EngineChatResponse};
use crate::plugins::ToolDefinition;
use crate::providers::Provider;
use crate::AppState;

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

/// Legacy chat request format (for backwards compatibility)
#[derive(Debug, Deserialize)]
pub struct LegacyChatRequest {
    pub messages: Vec<Message>,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_model")]
    pub model: String,
}

/// New chat request format using the chat engine
#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    /// The user's message
    pub message: String,

    /// Optional conversation ID for continuation
    #[serde(default)]
    pub conversation_id: Option<String>,

    /// Optional system prompt override (takes precedence over persona)
    #[serde(default)]
    pub system_prompt: Option<String>,

    /// Optional persona name (e.g., "business_analyst", "tech_support")
    #[serde(default)]
    pub persona: Option<String>,

    /// Provider to use
    #[serde(default = "default_provider")]
    pub provider: String,

    /// Model to use
    #[serde(default = "default_model")]
    pub model: String,
}

fn default_provider() -> String {
    "ollama".into()
}

fn default_model() -> String {
    "llama3.2".into()
}

/// Legacy chat response format
#[derive(Debug, Serialize)]
pub struct LegacyChatResponse {
    pub message: Message,
    pub usage: Option<Usage>,
}

/// New chat response format
#[derive(Debug, Serialize)]
pub struct ChatResponse {
    /// The assistant's response message
    pub message: String,

    /// Conversation ID for continuation
    pub conversation_id: String,

    /// Tools that were called (if any)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools_used: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

/// List of available tools
#[derive(Debug, Serialize)]
pub struct ToolsResponse {
    pub tools: Vec<ToolDefinition>,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// Legacy chat endpoint (direct LLM access)
async fn legacy_chat(
    State(state): State<AppState>,
    Json(request): Json<LegacyChatRequest>,
) -> Result<Json<LegacyChatResponse>, String> {
    let provider = Provider::from_name(&request.provider, &state.config)
        .map_err(|e| e.to_string())?;

    let response = provider
        .chat(&request.messages, &request.model)
        .await
        .map_err(|e| e.to_string())?;

    Ok(Json(LegacyChatResponse {
        message: response,
        usage: None,
    }))
}

/// New chat endpoint using the chat engine with tool support
async fn chat(
    State(state): State<AppState>,
    Json(request): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, String> {
    let engine_request = EngineChatRequest {
        message: request.message,
        conversation_id: request.conversation_id,
        system_prompt: request.system_prompt,
        persona: request.persona,
        provider: request.provider,
        model: request.model,
    };

    let response = state
        .chat_engine
        .chat(engine_request)
        .await
        .map_err(|e| e.to_string())?;

    Ok(Json(ChatResponse {
        message: response.message,
        conversation_id: response.conversation_id,
        tools_used: response.tool_calls.into_iter().map(|t| t.name).collect(),
    }))
}

/// List available tools
async fn list_tools(State(state): State<AppState>) -> Json<ToolsResponse> {
    Json(ToolsResponse {
        tools: state.chat_engine.available_tools(),
    })
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        // Legacy endpoint for backwards compatibility
        .route("/v1/chat", post(legacy_chat))
        // New chat endpoint with tool support
        .route("/v2/chat", post(chat))
        // List available tools
        .route("/v2/tools", get(list_tools))
}
