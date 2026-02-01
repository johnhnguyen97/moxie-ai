//! Ollama provider implementation

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::conversation::{Message, Role};

use super::ProviderError;

pub struct OllamaProvider {
    client: Client,
    base_url: String,
}

#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    message: OllamaMessage,
}

impl OllamaProvider {
    pub fn new(base_url: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
        }
    }

    pub async fn chat(&self, messages: &[Message], model: &str) -> Result<Message, ProviderError> {
        let ollama_messages: Vec<OllamaMessage> = messages
            .iter()
            .map(|m| OllamaMessage {
                role: match m.role {
                    Role::System => "system".to_string(),
                    Role::User => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                },
                content: m.content.clone(),
            })
            .collect();

        let request = OllamaRequest {
            model: model.to_string(),
            messages: ollama_messages,
            stream: false,
        };

        let response = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::InvalidResponse(format!(
                "{}: {}",
                status, body
            )));
        }

        let ollama_response: OllamaResponse = response.json().await?;

        Ok(Message {
            role: Role::Assistant,
            content: ollama_response.message.content,
        })
    }
}
