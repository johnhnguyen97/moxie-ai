# Moxie

Bold AI chatbot API for website integration.

## Quick Start

```bash
# Ensure Ollama is running
ollama serve

# Run Moxie
cargo run

# Test the API
curl http://localhost:3000/health

curl -X POST http://localhost:3000/v1/chat \
  -H "Content-Type: application/json" \
  -d '{"messages": [{"role": "user", "content": "Hello!"}]}'
```

## Features

- **Multi-provider support**: Ollama (local), OpenAI, Anthropic
- **Simple REST API**: Easy integration with any frontend
- **Streaming support**: Coming soon
- **Conversation management**: Coming soon

## Configuration

Copy `.env.example` to `.env` and configure:

```bash
HOST=127.0.0.1
PORT=3000
OLLAMA_URL=http://localhost:11434
# OPENAI_API_KEY=sk-...
# ANTHROPIC_API_KEY=sk-ant-...
```

## API

### POST /v1/chat

```json
{
  "messages": [
    {"role": "system", "content": "You are a helpful assistant."},
    {"role": "user", "content": "Hello!"}
  ],
  "provider": "ollama",
  "model": "llama3.2"
}
```

Response:

```json
{
  "message": {
    "role": "assistant",
    "content": "Hello! How can I help you today?"
  }
}
```

## License

MIT
