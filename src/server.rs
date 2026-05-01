use rmcp::{
    ErrorData as McpError,
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct AddArgs {
    pub a: f64,
    pub b: f64,
}

#[derive(Clone)]
pub struct AdditionServer {
    tool_router: ToolRouter<AdditionServer>,
}

impl AdditionServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

impl Default for AdditionServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router]
impl AdditionServer {
    #[tool(name = "add", description = "Add two numbers together")]
    async fn add(
        &self,
        Parameters(args): Parameters<AddArgs>,
    ) -> Result<CallToolResult, McpError> {
        let result = args.a + args.b;
        tracing::info!("Adding {} + {} = {}", args.a, args.b, result);

        Ok(CallToolResult {
            content: vec![Content::text(
                serde_json::json!({ "result": result }).to_string(),
            )],
            is_error: Some(false),
            meta: None,
            structured_content: None,
        })
    }
}

#[tool_handler]
impl ServerHandler for AdditionServer {
    fn get_info(&self) -> InitializeResult {
        InitializeResult {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability {
                    list_changed: Some(false),
                }),
                ..Default::default()
            },
            server_info: Implementation {
                name: "mcp-addition".to_string(),
                version: "0.1.0".to_string(),
                icons: None,
                title: None,
                website_url: None,
            },
            instructions: Some(
                "An MCP server that provides addition functionality".to_string(),
            ),
        }
    }
}
