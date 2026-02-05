//! Moxie - Bold AI Chatbot API
//!
//! Provides a unified API for integrating AI chatbots into websites.
//! Moxie is a self-hosted AI assistant platform with a plugin-based architecture
//! for deep system integration.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod conversation;
mod core;
mod plugins;
mod providers;
mod routes;

use config::Config;
use core::{ChatEngine, MemoryStore};
use plugins::filesystem::{FilesystemConfig, FilesystemPlugin};
use plugins::PluginRegistry;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub chat_engine: Arc<ChatEngine>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "moxie_ai=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::from_env()?;
    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;

    // Initialize memory store
    let data_dir = std::env::var("MOXIE_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./data"));

    let memory = Arc::new(
        MemoryStore::new(&data_dir.join("moxie.db"))
            .await
            .expect("Failed to initialize memory store"),
    );

    // Initialize plugin registry
    let mut registry = PluginRegistry::new();

    // Register filesystem plugin with default config
    // In production, this would be loaded from client config
    let fs_config = FilesystemConfig {
        allowed_paths: vec![
            std::env::current_dir().unwrap_or_default(),
        ],
        allow_write: false,
        max_file_size: 10 * 1024 * 1024, // 10 MB
    };
    registry.register(FilesystemPlugin::new(fs_config));

    tracing::info!("📦 Loaded {} plugin(s)", registry.len());

    // Initialize chat engine
    let chat_engine = Arc::new(ChatEngine::new(
        config.clone(),
        Arc::new(registry),
        memory,
    ));

    let state = AppState {
        config,
        chat_engine,
    };

    let app = Router::new()
        .merge(routes::router())
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    tracing::info!("🔥 Moxie API running at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
