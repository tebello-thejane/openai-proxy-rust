# OpenAI Compatible Proxy (Rust)

A lightweight, high-fidelity strictly-logging proxy for OpenAI-compatible APIs.

## Features
- **Full Logging**: Every request/response pair is logged to a detailed JSON file in `log/`.
- **"Dumb" Proxy**: Forwards requests exactly as received (including Authorization headers).
- **Configurable**: Change port or upstream target via environment variables.

## Quick Start
### Prerequisites
- Rust/Cargo
- Git

### Build & Run
```bash
# Navigate to project
cd ~/dev/exp/openai-proxy-rust

# Run in development mode
cargo run

# Run in release mode (faster)
cargo run --release
```

### Configuration
You can configure the server using environment variables:

```bash
# Example: Change port and upstream target
export PORT=8080
export OPENAI_API_BASE="https://api.openai.com/v1"
cargo run
```

## Testing
Verify the proxy is working with `curl`:

```bash
curl http://localhost:3000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer sk-YOUR-KEY" \
  -d '{
    "model": "gpt-3.5-turbo",
    "messages": [{"role": "user", "content": "Hello!"}]
  }'
```

Check the `log/` directory for the output `tx_*.json` files.
