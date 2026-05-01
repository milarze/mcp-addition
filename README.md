# MCP Addition Server

A simple Model Context Protocol (MCP) server written in Rust that provides an addition tool for adding two numbers together.

## Features

- **Stdio MCP transport** - Communicates via standard input/output
- **Streamable HTTP MCP transport** - HTTP/SSE transport with OAuth 2.1 protected access
- **Local OAuth 2.1 authorization server** - Discovery, dynamic client registration, PKCE, refresh tokens. In-memory, for local testing only.
- **Add tool** - Takes two numbers and returns their sum
- **JSON Schema validation** - Input parameters are validated automatically
- **Structured logging** - Uses tracing for monitoring operations

## Installation

### Prerequisites

- Rust 1.70 or higher
- Cargo (comes with Rust)

### Building

```bash
cargo build --release
```

The compiled binary will be available at `target/release/mcp-addition`.

## Usage

### Running the Server

```bash
cargo run
```

Or run the compiled binary:

```bash
./target/release/mcp-addition
```

### Connecting with MCP Inspector

To test the server with MCP Inspector:

```bash
npx @modelcontextprotocol/inspector cargo run
```

### Using with MCP Clients

The server implements the MCP protocol and can be used with any MCP-compatible client. Add it to your client's configuration:

```json
{
  "mcpServers": {
    "addition": {
      "command": "/path/to/mcp-addition"
    }
  }
}
```

## Available Tools

### `add`

Adds two numbers together.

**Parameters:**
- `a` (number, required) - The first number
- `b` (number, required) - The second number

**Returns:**
```json
{
  "result": 42.0
}
```

**Example:**
```json
{
  "a": 10.5,
  "b": 31.5
}
```

## Development

### Running in Development Mode

```bash
cargo run
```

### Running Tests

```bash
cargo test
```

### Checking Code

```bash
cargo check
```

## Logging

The server uses the `tracing` crate for logging. Set the `RUST_LOG` environment variable to control log levels:

```bash
RUST_LOG=info cargo run
RUST_LOG=debug cargo run
```

## Streamable HTTP transport (with OAuth)

Run the server in HTTP mode:

```bash
MCP_TRANSPORT=http cargo run
# defaults: bind 127.0.0.1:8000, issuer http://127.0.0.1:8000
```

Environment variables:

- `MCP_TRANSPORT` — `stdio` (default) or `http`
- `MCP_BIND_ADDR` — bind address, default `127.0.0.1:8000`
- `MCP_ISSUER` — OAuth issuer URL advertised in metadata, default `http://<bind>`

The `/mcp` endpoint requires `Authorization: Bearer <token>`. Unauthorized requests
get `401` with a `WWW-Authenticate: Bearer resource_metadata="..."` header so
spec-compliant clients can auto-discover the authorization server.

### OAuth endpoints

| Method | Path | Purpose |
| --- | --- | --- |
| GET  | `/.well-known/oauth-protected-resource`  | Resource metadata (RFC 9728-style) |
| GET  | `/.well-known/oauth-authorization-server` | Authorization server metadata (RFC 8414) |
| POST | `/oauth/register`                         | Dynamic client registration (RFC 7591) |
| GET  | `/oauth/authorize`                        | Renders Approve/Deny page (PKCE S256 required) |
| POST | `/oauth/authorize/decision`               | Form target for the Approve/Deny buttons |
| POST | `/oauth/token`                            | `authorization_code` and `refresh_token` grants |
| POST | `/oauth/revoke`                           | RFC 7009 revocation |

All state is in-memory and lost on restart. Public clients only (`token_endpoint_auth_methods_supported: ["none"]`); PKCE `S256` is required.

### End-to-end manual test

```bash
# 1. Start the server
MCP_TRANSPORT=http cargo run

# 2. PKCE pair
VERIFIER=$(openssl rand -base64 64 | tr -d '=+/\n' | head -c 64)
CHALLENGE=$(printf '%s' "$VERIFIER" | openssl dgst -sha256 -binary | base64 | tr '+/' '-_' | tr -d '=')

# 3. Register a client (DCR)
curl -sS -X POST http://127.0.0.1:8000/oauth/register \
  -H 'content-type: application/json' \
  -d '{"redirect_uris":["http://localhost:9000/cb"]}'
# => { "client_id": "...", ... }

# 4. Open this URL in a browser, click "Approve":
#    http://127.0.0.1:8000/oauth/authorize
#      ?response_type=code
#      &client_id=<CLIENT_ID>
#      &redirect_uri=http://localhost:9000/cb
#      &state=xyz
#      &code_challenge=$CHALLENGE
#      &code_challenge_method=S256
# You'll be redirected to http://localhost:9000/cb?code=<CODE>&state=xyz

# 5. Exchange the code for tokens
curl -sS -X POST http://127.0.0.1:8000/oauth/token \
  -d "grant_type=authorization_code" \
  -d "code=<CODE>" \
  -d "redirect_uri=http://localhost:9000/cb" \
  -d "code_verifier=$VERIFIER" \
  -d "client_id=<CLIENT_ID>"
# => { "access_token": "...", "refresh_token": "...", ... }

# 6. Call the MCP endpoint
curl -sS -X POST http://127.0.0.1:8000/mcp \
  -H "Authorization: Bearer <ACCESS_TOKEN>" \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -d '{"jsonrpc":"2.0","method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"curl","version":"0"}},"id":1}'

# 7. Refresh
curl -sS -X POST http://127.0.0.1:8000/oauth/token \
  -d "grant_type=refresh_token" \
  -d "refresh_token=<REFRESH_TOKEN>" \
  -d "client_id=<CLIENT_ID>"
```

## Project Structure

```
mcp-addition/
├── Cargo.toml          # Project dependencies and metadata
├── Cargo.lock          # Locked dependency versions
├── README.md           # This file
└── src/
    └── main.rs         # Main server implementation
```

## Dependencies

- `rmcp` - Rust MCP SDK for building MCP servers
- `tokio` - Async runtime
- `serde` / `serde_json` - Serialization/deserialization
- `schemars` - JSON Schema generation
- `anyhow` - Error handling
- `tracing` / `tracing-subscriber` - Structured logging

## License

This project is provided as-is for educational and demonstration purposes.

## Contributing

This is a simple example server. Feel free to extend it with additional mathematical operations or other functionality!
