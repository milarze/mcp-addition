use std::{net::SocketAddr, sync::Arc};

use anyhow::Result;
use axum::{
    Router,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use rmcp::transport::{
    StreamableHttpServerConfig,
    streamable_http_server::{session::local::LocalSessionManager, tower::StreamableHttpService},
};
use tower_http::cors::{Any, CorsLayer};

use crate::{oauth::OAuthState, server::AdditionServer};

const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html><head><title>mcp-addition</title>
<style>body{font-family:sans-serif;max-width:720px;margin:3em auto;padding:1em}
code,pre{background:#eee;padding:.1em .3em;border-radius:3px}
pre{padding:1em;overflow-x:auto}</style></head>
<body>
<h1>mcp-addition (Streamable HTTP + OAuth)</h1>
<p>MCP endpoint: <code>/mcp</code> (requires <code>Authorization: Bearer &lt;token&gt;</code>)</p>
<h2>OAuth discovery</h2>
<ul>
<li><a href="/.well-known/oauth-protected-resource">/.well-known/oauth-protected-resource</a></li>
<li><a href="/.well-known/oauth-authorization-server">/.well-known/oauth-authorization-server</a></li>
</ul>
<h2>Endpoints</h2>
<ul>
<li><code>POST /oauth/register</code> &mdash; dynamic client registration (RFC 7591)</li>
<li><code>GET  /oauth/authorize</code> &mdash; authorization page (PKCE S256)</li>
<li><code>POST /oauth/token</code> &mdash; authorization_code &amp; refresh_token grants</li>
<li><code>POST /oauth/revoke</code> &mdash; revoke access/refresh token</li>
</ul>
</body></html>"#;

pub async fn serve(addr: SocketAddr, issuer: String) -> Result<()> {
    let oauth = OAuthState::new(issuer.clone());

    // The MCP streamable HTTP service.
    let mcp_service: StreamableHttpService<AdditionServer, LocalSessionManager> =
        StreamableHttpService::new(
            || Ok(AdditionServer::new()),
            Arc::new(LocalSessionManager::default()),
            StreamableHttpServerConfig::default(),
        );

    // Protected MCP routes.
    let protected = Router::new()
        .nest_service("/mcp", mcp_service)
        .layer(middleware::from_fn_with_state(
            oauth.clone(),
            auth_middleware,
        ));

    // Public OAuth + meta routes.
    let public = Router::new()
        .route("/", get(|| async { Html(INDEX_HTML) }))
        .route(
            "/.well-known/oauth-protected-resource",
            get(crate::oauth::protected_resource_metadata),
        )
        .route(
            "/.well-known/oauth-authorization-server",
            get(crate::oauth::authorization_server_metadata),
        )
        .route("/oauth/register", post(crate::oauth::register))
        .route("/oauth/authorize", get(crate::oauth::authorize_page))
        .route(
            "/oauth/authorize/decision",
            post(crate::oauth::authorize_decision),
        )
        .route("/oauth/token", post(crate::oauth::token))
        .route("/oauth/revoke", post(crate::oauth::revoke))
        .with_state(oauth);

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = public.merge(protected).layer(cors);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Streamable HTTP MCP server listening on http://{}", addr);
    tracing::info!("Issuer: {}", issuer);

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("Shutting down...");
        })
        .await?;

    Ok(())
}

async fn auth_middleware(
    State(oauth): State<OAuthState>,
    headers: HeaderMap,
    req: Request,
    next: Next,
) -> Response {
    let token = extract_bearer(&headers);
    match token.and_then(|t| oauth.validate_access_token(&t)) {
        Some(_client_id) => next.run(req).await,
        None => unauthorized(&oauth.issuer),
    }
}

fn extract_bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

fn unauthorized(issuer: &str) -> Response {
    let challenge = format!(
        "Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource\"",
        issuer
    );
    let mut resp = (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    if let Ok(val) = HeaderValue::from_str(&challenge) {
        resp.headers_mut().insert(header::WWW_AUTHENTICATE, val);
    }
    resp
}
