//! AI provider integrations
//!
//! Moxie supports multiple LLM providers:
//!
//! - **Ollama** - Local LLM inference (default, fully on-premise)
//! - **OpenAI** - GPT-4, GPT-3.5 (requires API key)
//! - **Groq** - Fast inference with Llama, Mixtral (requires API key)
//! - **OpenAI-compatible** - Works with vLLM, LM Studio, LocalAI, etc.
//!
//! # Example Usage
//!
//! ```rust,ignore
//! // Use Ollama (local)
//! let provider = Provider::from_name("ollama", &config)?;
//!
//! // Use OpenAI
//! let provider = Provider::from_name("openai", &config)?;
//!
//! // Use Groq
//! let provider = Provider::from_name("groq", &config)?;
//! ```

mod ollama;
mod openai_compat;

use std::env;
use thiserror::Error;

use crate::config::Config;
use crate::conversation::Message;

pub use openai_compat::{OpenAICompatConfig, OpenAICompatProvider, ToolDef, FunctionDef};

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

/// Supported LLM providers
pub enum Provider {
    /// Local Ollama server
    Ollama(ollama::OllamaProvider),
    /// OpenAI-compatible API (OpenAI, Groq, vLLM, etc.)
    OpenAICompat(openai_compat::OpenAICompatProvider),
}

impl Provider {
    /// Create a provider from name and configuration
    ///
    /// Supported names:
    /// - "ollama" - Local Ollama server
    /// - "openai" - OpenAI API (requires OPENAI_API_KEY)
    /// - "groq" - Groq API (requires GROQ_API_KEY)
    /// - "local" - Local OpenAI-compatible server (uses OPENAI_BASE_URL)
    pub fn from_name(name: &str, config: &Config) -> Result<Self, ProviderError> {
        match name.to_lowercase().as_str() {
            "ollama" => {
                let url = config
                    .ollama_url
                    .clone()
                    .unwrap_or_else(|| "http://localhost:11434".into());
                Ok(Provider::Ollama(ollama::OllamaProvider::new(url)))
            }
            "openai" | "gpt" | "gpt4" => {
                let api_key = config
                    .openai_api_key
                    .clone()
                    .or_else(|| env::var("OPENAI_API_KEY").ok())
                    .ok_or_else(|| {
                        ProviderError::NotConfigured(
                            "OpenAI API key not found. Set OPENAI_API_KEY environment variable."
                                .to_string(),
                        )
                    })?;

                Ok(Provider::OpenAICompat(
                    openai_compat::OpenAICompatProvider::openai(api_key),
                ))
            }
            "groq" => {
                let api_key = env::var("GROQ_API_KEY").map_err(|_| {
                    ProviderError::NotConfigured(
                        "Groq API key not found. Set GROQ_API_KEY environment variable.".to_string(),
                    )
                })?;

                Ok(Provider::OpenAICompat(
                    openai_compat::OpenAICompatProvider::groq(api_key),
                ))
            }
            "local" | "vllm" | "lmstudio" | "localai" => {
                // For local servers, use OPENAI_BASE_URL or default to localhost:8000
                let base_url = env::var("OPENAI_BASE_URL")
                    .unwrap_or_else(|_| "http://localhost:8000/v1".to_string());
                let model =
                    env::var("LOCAL_MODEL").unwrap_or_else(|_| "default".to_string());

                Ok(Provider::OpenAICompat(
                    openai_compat::OpenAICompatProvider::local(base_url, model),
                ))
            }
            "anthropic" | "claude" => {
                // Anthropic has a different API format - for now, suggest alternatives
                Err(ProviderError::NotConfigured(
                    "Anthropic/Claude is not yet implemented. Use 'openai' or 'ollama' instead."
                        .to_string(),
                ))
            }
            _ => Err(ProviderError::UnknownProvider(name.to_string())),
        }
    }

    /// Send a chat completion request
    pub async fn chat(
        &self,
        messages: &[Message],
        model: &str,
    ) -> Result<Message, ProviderError> {
        match self {
            Provider::Ollama(p) => p.chat(messages, model).await,
            Provider::OpenAICompat(p) => p.chat(messages, model).await,
        }
    }

    /// Get the provider name
    pub fn name(&self) -> &str {
        match self {
            Provider::Ollama(_) => "ollama",
            Provider::OpenAICompat(_) => "openai-compatible",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_from_name_ollama() {
        let config = Config {
            host: "127.0.0.1".to_string(),
            port: 3000,
            openai_api_key: None,
            anthropic_api_key: None,
            ollama_url: Some("http://localhost:11434".to_string()),
        };

        let provider = Provider::from_name("ollama", &config);
        assert!(provider.is_ok());
        assert_eq!(provider.unwrap().name(), "ollama");
    }

    #[test]
    fn test_provider_from_name_unknown() {
        let config = Config {
            host: "127.0.0.1".to_string(),
            port: 3000,
            openai_api_key: None,
            anthropic_api_key: None,
            ollama_url: None,
        };

        let provider = Provider::from_name("unknown_provider", &config);
        assert!(provider.is_err());
    }

    #[test]
    fn test_provider_aliases() {
        let config = Config {
            host: "127.0.0.1".to_string(),
            port: 3000,
            openai_api_key: Some("test-key".to_string()),
            anthropic_api_key: None,
            ollama_url: None,
        };

        // "gpt" should work as alias for openai
        let provider = Provider::from_name("gpt", &config);
        assert!(provider.is_ok());
    }
}
