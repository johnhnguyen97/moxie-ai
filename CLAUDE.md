# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Moxie is a Rust-based AI chatbot API designed for website integration. It provides a unified interface to multiple AI providers (Ollama, OpenAI, Anthropic).

## Commands

```bash
# Build
cargo build

# Run (dev)
cargo run

# Run with logging
RUST_LOG=moxie=debug cargo run

# Test
cargo test

# Format
cargo fmt

# Lint
cargo clippy
```

## Architecture

```
src/
├── main.rs           # Entry point, server setup
├── config/           # Environment configuration
├── routes/           # API endpoints (/health, /v1/chat)
├── providers/        # AI provider implementations
│   ├── mod.rs        # Provider trait and factory
│   └── ollama.rs     # Ollama integration
└── conversation/     # Message types, conversation state
```

## API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check |
| `/v1/chat` | POST | Send chat message |

### Chat Request

```json
{
  "messages": [
    {"role": "user", "content": "Hello"}
  ],
  "provider": "ollama",
  "model": "llama3.2"
}
```

## Adding New Providers

1. Create `src/providers/newprovider.rs`
2. Implement the chat method matching `OllamaProvider`
3. Add variant to `Provider` enum in `src/providers/mod.rs`
4. Add match arm in `Provider::from_name()`

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `HOST` | 127.0.0.1 | Server host |
| `PORT` | 3000 | Server port |
| `OLLAMA_URL` | http://localhost:11434 | Ollama API URL |
| `OPENAI_API_KEY` | - | OpenAI API key |
| `ANTHROPIC_API_KEY` | - | Anthropic API key |
