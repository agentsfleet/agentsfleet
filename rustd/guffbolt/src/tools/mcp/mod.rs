//! A Model Context Protocol (MCP) client: a server's tools offered to the
//! model as the run's own, and its resources listed and read, over the
//! official SDK (`rmcp`), the way Codex and pi reach a server.
//!
//! ```text
//!  Server::stdio(name, env, spawn) ── a process through the run's Env
//!  Server::http(name, url, outbound) ─ admitted by the run's Outbound hook
//!        │
//!        ├─► Server::tools() ─► McpTool per server tool, named mcp__<server>__<tool>
//!        └─► ListMcpResources · ListMcpResourceTemplates · ReadMcpResource
//! ```
//!
//! A stdio server runs where the run's commands run: its process is started
//! through the [`Env`], never on the host behind the harness's back. A
//! remote server is connected to only once the [`Outbound`] hook admits its
//! URL, and every request to it carries what the hook said to carry. What a
//! server answers is text the model reads; a server's error is a failed call,
//! never a failed run.

pub mod resources;
mod stdio;

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{HeaderName, HeaderValue};
use rmcp::model::{CallToolRequestParams, CallToolResult};
use rmcp::service::RunningService;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::{RoleClient, ServiceExt as _};
use serde_json::Value;

use crate::tool::{Describe, Tool, ToolFuture, ToolOutput, ToolSpec};
use crate::tools::env::{Env, Spawn};
use crate::tools::outbound::{Outbound, client};
use crate::tools::{Context, code};

pub use self::resources::{ListMcpResourceTemplates, ListMcpResources, ReadMcpResource};
use self::stdio::EnvStdio;

/// What every MCP tool's name starts with, before its server's name.
pub const PREFIX: &str = "mcp__";
/// What parts a server's name from its tool's in a tool's name.
const SEPARATOR: &str = "__";
/// The longest name a provider's function wire takes.
const NAME_MAX: usize = 64;
/// How long one request to a server may take.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(120);
/// What the outbound hook names an MCP connection's tool as.
const TOOL: &str = "mcp";

/// Why a server could not be reached, or would not answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpError(String);

impl McpError {
    fn new(why: impl fmt::Display) -> Self {
        Self(why.to_string())
    }
}

impl fmt::Display for McpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for McpError {}

/// One connected MCP server.
pub struct Server {
    name: String,
    service: RunningService<RoleClient, ()>,
}

impl fmt::Debug for Server {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Server")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl Server {
    /// Starts the server `spawn` names through `env` and speaks to it over
    /// its standard input and output.
    ///
    /// # Errors
    /// The process would not start, or the server would not initialize.
    pub async fn stdio(
        name: impl Into<String>,
        env: Arc<dyn Env>,
        spawn: &Spawn,
    ) -> Result<Self, McpError> {
        let process = env.spawn(spawn).await.map_err(McpError::new)?;
        let transport = EnvStdio::new(env, process);
        let service = ().serve(transport).await.map_err(McpError::new)?;
        Ok(Self {
            name: name.into(),
            service,
        })
    }

    /// Connects to the server at `url` over streamable HTTP, once `outbound`
    /// admits it.
    ///
    /// # Errors
    /// The hook refused the URL, or the server would not initialize.
    pub async fn http(
        name: impl Into<String>,
        url: &str,
        outbound: &dyn Outbound,
    ) -> Result<Self, McpError> {
        let admitted = outbound
            .connect(TOOL, url)
            .await
            .map_err(|refused| McpError::new(format!("[{}] {}", refused.code, refused.detail)))?;
        let mut headers = HashMap::new();
        for (name, value) in admitted.headers {
            let name = HeaderName::try_from(name).map_err(McpError::new)?;
            let value = HeaderValue::try_from(value).map_err(McpError::new)?;
            headers.insert(name, value);
        }
        let config =
            StreamableHttpClientTransportConfig::with_uri(admitted.url).custom_headers(headers);
        let client = client(CALL_TIMEOUT).build().map_err(McpError::new)?;
        let transport = StreamableHttpClientTransport::with_client(client, config);
        let service = ().serve(transport).await.map_err(McpError::new)?;
        Ok(Self {
            name: name.into(),
            service,
        })
    }

    /// The name the run knows the server by.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Every tool the server offers, each named `mcp__<server>__<tool>`.
    ///
    /// # Errors
    /// The server would not list its tools.
    pub async fn tools(self: &Arc<Self>) -> Result<Vec<McpTool>, McpError> {
        let listed = tokio::time::timeout(CALL_TIMEOUT, self.service.list_all_tools())
            .await
            .map_err(McpError::new)?
            .map_err(McpError::new)?;
        Ok(listed
            .into_iter()
            .map(|tool| McpTool {
                server: Arc::clone(self),
                name: tool_name(&self.name, &tool.name),
                remote: tool.name.into_owned(),
                description: tool.description.map_or_default(String::from),
                parameters: Value::Object((*tool.input_schema).clone()),
            })
            .collect())
    }

    /// Calls the server's tool `remote` with `arguments`.
    async fn call(&self, remote: &str, arguments: &Value) -> ToolOutput {
        let mut params = CallToolRequestParams::new(remote.to_owned());
        match arguments {
            Value::Object(arguments) => params = params.with_arguments(arguments.clone()),
            Value::Null => {}
            _other => {
                return ToolOutput::failed(
                    code::INVALID_ARGUMENTS,
                    "an MCP tool's arguments are an object",
                );
            }
        }
        match tokio::time::timeout(CALL_TIMEOUT, self.service.call_tool(params)).await {
            Ok(Ok(result)) => answered(&result),
            Ok(Err(failure)) => ToolOutput::failed(code::MCP_FAILED, &failure.to_string()),
            Err(_elapsed) => ToolOutput::failed(
                code::MCP_FAILED,
                &format!("{} did not answer within {CALL_TIMEOUT:?}", self.name),
            ),
        }
    }
}

/// One tool an MCP server offers, as the model is offered it.
#[derive(Debug)]
pub struct McpTool {
    server: Arc<Server>,
    name: String,
    remote: String,
    description: String,
    parameters: Value,
}

impl McpTool {
    /// The name the server knows the tool by.
    #[must_use]
    pub fn remote(&self) -> &str {
        &self.remote
    }
}

impl Describe for McpTool {
    fn spec(&self) -> ToolSpec<'_> {
        ToolSpec {
            name: &self.name,
            description: &self.description,
            parameters: &self.parameters,
        }
    }
}

impl<'c> Tool<Context<'c>> for McpTool {
    fn call<'a>(&'a self, arguments: &'a Value, _cx: Context<'c>) -> ToolFuture<'a>
    where
        Context<'c>: 'a,
    {
        Box::pin(self.server.call(&self.remote, arguments))
    }
}

/// `mcp__<server>__<tool>`, every character a function wire refuses made an
/// underscore and the whole kept to [`NAME_MAX`].
#[must_use]
pub fn tool_name(server: &str, tool: &str) -> String {
    let named: String = format!("{PREFIX}{server}{SEPARATOR}{tool}")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect();
    named.chars().take(NAME_MAX).collect()
}

/// What a tool call answered, as the model reads it: each text part, a
/// line naming each other part, and the structured answer when nothing
/// else said anything. A result the server marked an error is a failed call.
fn answered(result: &CallToolResult) -> ToolOutput {
    let value = serde_json::to_value(result).unwrap_or_default();
    let parts = value
        .get("content")
        .and_then(Value::as_array)
        .map_or_default(|parts| parts.iter().map(part).collect::<Vec<_>>());
    let mut text = parts.join("\n");
    if text.is_empty()
        && let Some(structured) = value.get("structuredContent")
    {
        text = structured.to_string();
    }
    if result.is_error == Some(true) {
        ToolOutput::failed(code::MCP_FAILED, &text)
    } else {
        ToolOutput::succeeded(text)
    }
}

/// One part of an answer, as the model reads it.
fn part(part: &Value) -> String {
    let kind = part.get("type").and_then(Value::as_str).unwrap_or_default();
    let text = |key: &str| part.get(key).and_then(Value::as_str).map(str::to_owned);
    match kind {
        "text" => text("text").unwrap_or_default(),
        "resource" => part
            .get("resource")
            .and_then(|resource| resource.get("text"))
            .and_then(Value::as_str)
            .map_or_else(
                || format!("[resource {}]", resource_uri(part)),
                str::to_owned,
            ),
        "resource_link" => format!("[resource {}]", text("uri").unwrap_or_default()),
        other => format!(
            "[{other} {}]",
            text("mimeType").unwrap_or_else(|| "content".to_owned())
        ),
    }
}

/// The URI an embedded resource names.
fn resource_uri(part: &Value) -> String {
    part.get("resource")
        .and_then(|resource| resource.get("uri"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}
