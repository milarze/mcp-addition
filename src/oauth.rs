//! Minimal in-memory OAuth 2.1 authorization server for local testing.
//!
//! Implements:
//!   - RFC 8414 / MCP discovery metadata
//!   - RFC 7591 dynamic client registration
//!   - Authorization code grant with PKCE (S256)
//!   - Refresh token grant (with rotation)
//!   - RFC 7009 token revocation
//!
//! State is held in-memory and lost on restart. Not for production.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    Form, Json,
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const ACCESS_TOKEN_TTL: Duration = Duration::from_secs(3600);
const AUTH_CODE_TTL: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct OAuthState {
    pub issuer: String,
    inner: Arc<Mutex<Inner>>,
}

#[derive(Default)]
struct Inner {
    clients: HashMap<String, ClientRecord>,
    auth_codes: HashMap<String, AuthCodeRecord>,
    access_tokens: HashMap<String, TokenRecord>,
    refresh_tokens: HashMap<String, RefreshRecord>,
}

#[derive(Clone)]
struct ClientRecord {
    redirect_uris: Vec<String>,
}

struct AuthCodeRecord {
    client_id: String,
    redirect_uri: String,
    code_challenge: String,
    expires_at: Instant,
}

struct TokenRecord {
    client_id: String,
    expires_at: Instant,
}

struct RefreshRecord {
    client_id: String,
}

impl OAuthState {
    pub fn new(issuer: String) -> Self {
        Self {
            issuer,
            inner: Arc::new(Mutex::new(Inner::default())),
        }
    }

    /// Validate a bearer access token. Returns the client_id if valid.
    pub fn validate_access_token(&self, token: &str) -> Option<String> {
        let mut g = self.inner.lock().unwrap();
        let now = Instant::now();
        // Lazy expiry
        if let Some(rec) = g.access_tokens.get(token) {
            if rec.expires_at > now {
                return Some(rec.client_id.clone());
            }
        }
        g.access_tokens.remove(token);
        None
    }
}

fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

// ---------- discovery ----------

pub async fn protected_resource_metadata(
    State(s): State<OAuthState>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "resource": s.issuer,
        "authorization_servers": [s.issuer],
        "bearer_methods_supported": ["header"],
    }))
}

pub async fn authorization_server_metadata(
    State(s): State<OAuthState>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "issuer": s.issuer,
        "registration_endpoint": format!("{}/oauth/register", s.issuer),
        "authorization_endpoint": format!("{}/oauth/authorize", s.issuer),
        "token_endpoint": format!("{}/oauth/token", s.issuer),
        "revocation_endpoint": format!("{}/oauth/revoke", s.issuer),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
    }))
}

// ---------- dynamic client registration ----------

#[derive(Deserialize)]
pub struct RegisterRequest {
    redirect_uris: Vec<String>,
    #[serde(default)]
    client_name: Option<String>,
}

#[derive(Serialize)]
pub struct RegisterResponse {
    client_id: String,
    redirect_uris: Vec<String>,
    token_endpoint_auth_method: &'static str,
    grant_types: Vec<&'static str>,
    response_types: Vec<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_name: Option<String>,
}

pub async fn register(
    State(s): State<OAuthState>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, (StatusCode, Json<serde_json::Value>)> {
    if req.redirect_uris.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_redirect_uri",
                "error_description": "redirect_uris is required",
            })),
        ));
    }
    let client_id = uuid::Uuid::new_v4().to_string();
    s.inner.lock().unwrap().clients.insert(
        client_id.clone(),
        ClientRecord {
            redirect_uris: req.redirect_uris.clone(),
        },
    );
    tracing::info!("Registered client {}", client_id);
    Ok(Json(RegisterResponse {
        client_id,
        redirect_uris: req.redirect_uris,
        token_endpoint_auth_method: "none",
        grant_types: vec!["authorization_code", "refresh_token"],
        response_types: vec!["code"],
        client_name: req.client_name,
    }))
}

// ---------- authorize (GET = approval page, POST = decision) ----------

#[derive(Deserialize, Clone)]
pub struct AuthorizeQuery {
    pub response_type: String,
    pub client_id: String,
    pub redirect_uri: String,
    #[serde(default)]
    pub state: Option<String>,
    pub code_challenge: String,
    pub code_challenge_method: String,
    #[serde(default)]
    pub scope: Option<String>,
}

pub async fn authorize_page(
    State(s): State<OAuthState>,
    Query(q): Query<AuthorizeQuery>,
) -> Response {
    if let Err(resp) = validate_authorize(&s, &q) {
        return resp;
    }

    let escaped_state = q.state.clone().unwrap_or_default();
    let scope = q.scope.clone().unwrap_or_default();

    // Build a tiny HTML page with a form that POSTs the same params back.
    let html = format!(
        r#"<!DOCTYPE html>
<html><head><title>Authorize</title>
<style>body{{font-family:sans-serif;max-width:560px;margin:3em auto;padding:1em}}
button{{font-size:1em;padding:.6em 1.2em;margin-right:.5em}}
.approve{{background:#2d7;color:#fff;border:0;border-radius:4px}}
.deny{{background:#d33;color:#fff;border:0;border-radius:4px}}
code{{background:#eee;padding:.1em .3em;border-radius:3px}}</style>
</head><body>
<h1>Authorize Access</h1>
<p>Client <code>{client_id}</code> is requesting authorization.</p>
<p>Redirect URI: <code>{redirect_uri}</code></p>
<p>Scope: <code>{scope}</code></p>
<form method="POST" action="/oauth/authorize/decision">
  <input type="hidden" name="client_id" value="{client_id}">
  <input type="hidden" name="redirect_uri" value="{redirect_uri}">
  <input type="hidden" name="state" value="{state}">
  <input type="hidden" name="code_challenge" value="{code_challenge}">
  <input type="hidden" name="code_challenge_method" value="{code_challenge_method}">
  <input type="hidden" name="scope" value="{scope}">
  <button class="approve" name="decision" value="approve" type="submit">Approve</button>
  <button class="deny" name="decision" value="deny" type="submit">Deny</button>
</form>
</body></html>"#,
        client_id = html_escape(&q.client_id),
        redirect_uri = html_escape(&q.redirect_uri),
        state = html_escape(&escaped_state),
        code_challenge = html_escape(&q.code_challenge),
        code_challenge_method = html_escape(&q.code_challenge_method),
        scope = html_escape(&scope),
    );
    Html(html).into_response()
}

#[derive(Deserialize)]
pub struct AuthorizeDecision {
    decision: String,
    client_id: String,
    redirect_uri: String,
    #[serde(default)]
    state: Option<String>,
    code_challenge: String,
    code_challenge_method: String,
    #[serde(default)]
    scope: Option<String>,
}

pub async fn authorize_decision(
    State(s): State<OAuthState>,
    Form(f): Form<AuthorizeDecision>,
) -> Response {
    let q = AuthorizeQuery {
        response_type: "code".to_string(),
        client_id: f.client_id,
        redirect_uri: f.redirect_uri,
        state: f.state,
        code_challenge: f.code_challenge,
        code_challenge_method: f.code_challenge_method,
        scope: f.scope,
    };
    if let Err(resp) = validate_authorize(&s, &q) {
        return resp;
    }
    if f.decision != "approve" {
        return redirect_with_error(&q.redirect_uri, "access_denied", q.state.as_deref());
    }

    // Issue an authorization code.
    let code = random_token();
    s.inner.lock().unwrap().auth_codes.insert(
        code.clone(),
        AuthCodeRecord {
            client_id: q.client_id.clone(),
            redirect_uri: q.redirect_uri.clone(),
            code_challenge: q.code_challenge.clone(),
            expires_at: Instant::now() + AUTH_CODE_TTL,
        },
    );
    tracing::info!("Issued auth code for client {}", q.client_id);

    let mut url = format!("{}?code={}", q.redirect_uri, urlencode(&code));
    if let Some(st) = q.state.as_deref() {
        url.push_str(&format!("&state={}", urlencode(st)));
    }
    Redirect::to(&url).into_response()
}

fn validate_authorize(s: &OAuthState, q: &AuthorizeQuery) -> Result<(), Response> {
    if q.response_type != "code" {
        return Err((
            StatusCode::BAD_REQUEST,
            "unsupported response_type (must be \"code\")",
        )
            .into_response());
    }
    if q.code_challenge_method != "S256" {
        return Err((
            StatusCode::BAD_REQUEST,
            "code_challenge_method must be S256",
        )
            .into_response());
    }
    let g = s.inner.lock().unwrap();
    let Some(client) = g.clients.get(&q.client_id) else {
        return Err((StatusCode::BAD_REQUEST, "unknown client_id").into_response());
    };
    if !client.redirect_uris.contains(&q.redirect_uri) {
        return Err((StatusCode::BAD_REQUEST, "redirect_uri not registered").into_response());
    }
    Ok(())
}

fn redirect_with_error(redirect_uri: &str, err: &str, state: Option<&str>) -> Response {
    let mut url = format!("{}?error={}", redirect_uri, urlencode(err));
    if let Some(st) = state {
        url.push_str(&format!("&state={}", urlencode(st)));
    }
    Redirect::to(&url).into_response()
}

// ---------- token ----------

#[derive(Deserialize)]
pub struct TokenRequest {
    grant_type: String,
    // authorization_code grant
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    redirect_uri: Option<String>,
    #[serde(default)]
    code_verifier: Option<String>,
    // refresh_token grant
    #[serde(default)]
    refresh_token: Option<String>,
    // common
    #[serde(default)]
    client_id: Option<String>,
}

#[derive(Serialize)]
pub struct TokenResponse {
    access_token: String,
    token_type: &'static str,
    expires_in: u64,
    refresh_token: String,
}

pub async fn token(
    State(s): State<OAuthState>,
    Form(req): Form<TokenRequest>,
) -> Result<Json<TokenResponse>, (StatusCode, Json<serde_json::Value>)> {
    match req.grant_type.as_str() {
        "authorization_code" => grant_authorization_code(&s, req),
        "refresh_token" => grant_refresh(&s, req),
        other => Err(token_error(
            StatusCode::BAD_REQUEST,
            "unsupported_grant_type",
            &format!("grant_type {} not supported", other),
        )),
    }
}

fn grant_authorization_code(
    s: &OAuthState,
    req: TokenRequest,
) -> Result<Json<TokenResponse>, (StatusCode, Json<serde_json::Value>)> {
    let code = req.code.ok_or_else(|| {
        token_error(StatusCode::BAD_REQUEST, "invalid_request", "missing code")
    })?;
    let verifier = req.code_verifier.ok_or_else(|| {
        token_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "missing code_verifier",
        )
    })?;
    let client_id = req.client_id.ok_or_else(|| {
        token_error(StatusCode::BAD_REQUEST, "invalid_request", "missing client_id")
    })?;
    let redirect_uri = req.redirect_uri.ok_or_else(|| {
        token_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "missing redirect_uri",
        )
    })?;

    let mut g = s.inner.lock().unwrap();
    let rec = g.auth_codes.remove(&code).ok_or_else(|| {
        token_error(StatusCode::BAD_REQUEST, "invalid_grant", "unknown code")
    })?;
    if rec.expires_at < Instant::now() {
        return Err(token_error(
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            "code expired",
        ));
    }
    if rec.client_id != client_id {
        return Err(token_error(
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            "client_id mismatch",
        ));
    }
    if rec.redirect_uri != redirect_uri {
        return Err(token_error(
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            "redirect_uri mismatch",
        ));
    }
    // PKCE verify: base64url(SHA256(verifier)) == code_challenge
    let computed = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    if computed != rec.code_challenge {
        return Err(token_error(
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            "PKCE verification failed",
        ));
    }

    Ok(Json(issue_tokens(&mut g, &client_id)))
}

fn grant_refresh(
    s: &OAuthState,
    req: TokenRequest,
) -> Result<Json<TokenResponse>, (StatusCode, Json<serde_json::Value>)> {
    let refresh = req.refresh_token.ok_or_else(|| {
        token_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "missing refresh_token",
        )
    })?;
    let client_id = req.client_id.ok_or_else(|| {
        token_error(StatusCode::BAD_REQUEST, "invalid_request", "missing client_id")
    })?;

    let mut g = s.inner.lock().unwrap();
    let rec = g.refresh_tokens.remove(&refresh).ok_or_else(|| {
        token_error(
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            "unknown refresh_token",
        )
    })?;
    if rec.client_id != client_id {
        return Err(token_error(
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            "client_id mismatch",
        ));
    }
    Ok(Json(issue_tokens(&mut g, &client_id)))
}

fn issue_tokens(g: &mut Inner, client_id: &str) -> TokenResponse {
    let access = random_token();
    let refresh = random_token();
    g.access_tokens.insert(
        access.clone(),
        TokenRecord {
            client_id: client_id.to_string(),
            expires_at: Instant::now() + ACCESS_TOKEN_TTL,
        },
    );
    g.refresh_tokens.insert(
        refresh.clone(),
        RefreshRecord {
            client_id: client_id.to_string(),
        },
    );
    TokenResponse {
        access_token: access,
        token_type: "Bearer",
        expires_in: ACCESS_TOKEN_TTL.as_secs(),
        refresh_token: refresh,
    }
}

fn token_error(
    status: StatusCode,
    err: &str,
    desc: &str,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        status,
        Json(serde_json::json!({
            "error": err,
            "error_description": desc,
        })),
    )
}

// ---------- revoke ----------

#[derive(Deserialize)]
pub struct RevokeRequest {
    token: String,
    #[serde(default)]
    #[allow(dead_code)]
    token_type_hint: Option<String>,
}

pub async fn revoke(State(s): State<OAuthState>, Form(req): Form<RevokeRequest>) -> StatusCode {
    let mut g = s.inner.lock().unwrap();
    g.access_tokens.remove(&req.token);
    g.refresh_tokens.remove(&req.token);
    StatusCode::OK
}

// ---------- helpers ----------

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn urlencode(s: &str) -> String {
    // Minimal percent-encoding of reserved chars for query strings.
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        let c = b as char;
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
            out.push(c);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}
