# System Design: OpenAI Compatible Proxy

## 1. Overview
This system is a lightweight, high-fidelity reverse proxy designed to intercept, log, and forward HTTP requests to an OpenAI-compatible upstream API. It is built in Rust for performance and safety.

## 2. Architecture

### 2.1 Components
- **Server**: Built on `axum`, running an asynchronous event loop via `tokio`.
- **HTTP Client**: Uses `reqwest` to forward requests to the upstream provider (e.g., OpenAI, local LLM).
- **Logging Subsystem**:
    - **Stdout**: Real-time structured logs using `tracing`.
    - **File System**: Detailed, per-transaction JSON logs stored in `log/`.

### 2.2 Data Flow
1. **Request Ingestion**: The server receives an HTTP POST request at `/v1/chat/completions`.
2. **Request Processing**:
    - A unique correlation ID (UUID) is generated.
    - Timestamp is recorded.
    - Headers and Body are read and buffered.
    - **Logging (Phase 1)**: Request details are captured in memory structure.
3. **Upstream Forwarding**:
    - A new upstream request is constructed, copying the method, body, and relevant headers (specifically `Authorization`).
    - The request is sent to `OPENAI_API_BASE` (default: `https://api.openai.com/v1`).
4. **Response Handling**:
    - The upstream response headers and body are received and buffered.
    - **Logging (Phase 2)**: Request and Response are combined into a `Transaction` object and written to `log/tx_<timestamp>_<uuid>.json`.
5. **Client Response**: The response is returned to the original client.

## 3. Data Structures

### 3.1 Log File Format
Each file `log/tx_<timestamp>_<uuid>.json` contains a single JSON object:

```json
{
  "id": "uuid-v4",
  "timestamp": "ISO-8601",
  "request": {
    "method": "POST",
    "url": "...",
    "headers": { ... },
    "body": { ... } // Parsed JSON if possible, else string
  },
  "response": {
    "status": 200,
    "headers": { ... },
    "body": { ... },
    "latency_ms": 123
  }
}
```

## 4. Configuration
- **Environment Variables**:
    - `OPENAI_API_BASE`: Base URL for the upstream API (default: `https://api.openai.com/v1`).
    - `PORT`: Listening port (default: `3000`).

## 5. Security Considerations
- **API Keys**: Keys are passed through via the `Authorization` header. They are logged. **CAUTION**: This is a debug proxy; production usage requires redaction of sensitive keys.
