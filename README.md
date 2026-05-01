# MCP Addition Server

A simple Model Context Protocol (MCP) server written in Rust that provides an addition tool for adding two numbers together.

## Features

- **Stdio-based MCP server** - Communicates via standard input/output
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
