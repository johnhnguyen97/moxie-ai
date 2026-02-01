//! API routes

use axum::{
    extract::State,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::conversation::Message;
use crate::providers::Provider;

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<Message>,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_model")]
    pub model: String,
}

fn default_provider() -> String {
    "ollama".into()
}

fn default_model() -> String {
    "llama3.2".into()
}

#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub message: Message,
    pub usage: Option<Usage>,
}

#[derive(Debug, Serialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn chat(
    State(config): State<Config>,
    Json(request): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, String> {
    let provider = Provider::from_name(&request.provider, &config)
        .map_err(|e| e.to_string())?;

    let response = provider
        .chat(&request.messages, &request.model)
        .await
        .map_err(|e| e.to_string())?;

    Ok(Json(ChatResponse {
        message: response,
        usage: None,
    }))
}

pub fn router() -> Router<Config> {
    Router::new()
        .route("/health", get(health))
        .route("/v1/chat", post(chat))
}
