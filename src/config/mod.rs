//! Application configuration

pub mod client;
pub mod prompts;

use std::env;

use serde::{Deserialize, Serialize};

pub use client::ClientConfig;
pub use prompts::{PromptManager, PromptTemplate, builtin as prompts_builtin};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub openai_api_key: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub ollama_url: Option<String>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            host: env::var("HOST").unwrap_or_else(|_| "127.0.0.1".into()),
            port: env::var("PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(3000),
            openai_api_key: env::var("OPENAI_API_KEY").ok(),
            anthropic_api_key: env::var("ANTHROPIC_API_KEY").ok(),
            ollama_url: env::var("OLLAMA_URL").ok(),
        })
    }
}
