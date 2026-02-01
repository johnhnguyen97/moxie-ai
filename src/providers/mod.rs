//! AI provider integrations

mod ollama;

use thiserror::Error;

use crate::config::Config;
use crate::conversation::Message;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("Unknown provider: {0}")]
    UnknownProvider(String),

    #[error("Provider not configured: {0}")]
    NotConfigured(String),

    #[error("Request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}

pub enum Provider {
    Ollama(ollama::OllamaProvider),
}

impl Provider {
    pub fn from_name(name: &str, config: &Config) -> Result<Self, ProviderError> {
        match name.to_lowercase().as_str() {
            "ollama" => {
                let url = config
                    .ollama_url
                    .clone()
                    .unwrap_or_else(|| "http://localhost:11434".into());
                Ok(Provider::Ollama(ollama::OllamaProvider::new(url)))
            }
            _ => Err(ProviderError::UnknownProvider(name.to_string())),
        }
    }

    pub async fn chat(&self, messages: &[Message], model: &str) -> Result<Message, ProviderError> {
        match self {
            Provider::Ollama(p) => p.chat(messages, model).await,
        }
    }
}
