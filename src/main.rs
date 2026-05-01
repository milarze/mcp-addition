use anyhow::Result;
use rmcp::{ServiceExt, transport};
use tracing_subscriber::{self, EnvFilter};

mod http;
mod oauth;
mod server;

use server::AdditionServer;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let transport_kind = std::env::var("MCP_TRANSPORT").unwrap_or_else(|_| "stdio".to_string());

    match transport_kind.as_str() {
        "http" => {
            let bind = std::env::var("MCP_BIND_ADDR")
                .unwrap_or_else(|_| "127.0.0.1:8000".to_string());
            let addr = bind.parse()?;
            let issuer = std::env::var("MCP_ISSUER")
                .unwrap_or_else(|_| format!("http://{}", bind));
            tracing::info!("Starting MCP Addition Server (streamable HTTP + OAuth)");
            http::serve(addr, issuer).await?;
        }
        "stdio" | "" => {
            tracing::info!("Starting MCP Addition Server (stdio)");
            let server = AdditionServer::new();
            let service = server.serve(transport::stdio()).await?;
            service.waiting().await?;
        }
        other => {
            anyhow::bail!(
                "Unknown MCP_TRANSPORT={}; expected \"stdio\" or \"http\"",
                other
            );
        }
    }

    Ok(())
}
