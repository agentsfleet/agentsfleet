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
//! URL, and every request to it carries what the hook said to carry. Its
//! host is never a private address: a literal is refused before the client
//! is built, and a name is looked up through [`Outbound::lookup`] and judged
//! then and again by the client's own resolver as it connects. What a server
//! answers is text the model reads; a server's error is a failed call, never
//! a failed run.

pub mod resources;
mod stdio;

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use reqwest::Url;
use reqwest::header::{HeaderName, HeaderValue, InvalidHeaderName, InvalidHeaderValue};
use rmcp::ServiceError;
use rmcp::model::{CallToolRequestParams, CallToolResult};
use rmcp::service::{ClientInitializeError, RunningService};
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::{RoleClient, ServiceExt as _};
use serde_json::Value;
use tokio::time::error::Elapsed;

use crate::tool::{Describe, Tool, ToolFuture, ToolOutput, ToolSpec};
use crate::tools::env::{Env, EnvError, Spawn};
use crate::tools::outbound::{Outbound, Refused, client, reachable};
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

/// What a fallible MCP call answers.
pub type Result<T, E = McpError> = core::result::Result<T, E>;

/// Why a server could not be reached, or would not answer. Each kind but a
/// refusal holds the failure that caused it, as its `source()`.
#[derive(Debug)]
#[non_exhaustive]
pub enum McpError {
    /// The run's environment would not start the server's process.
    Spawn(EnvError),
    /// The server would not initialize its session.
    Initialize(Box<ClientInitializeError>),
    /// The outbound hook refused the server, or its host is or resolves to a
    /// private address: the code and sentence the model reads.
    Refused(Refused),
    /// The URL the hook admitted is not a URL.
    Url(url::ParseError),
    /// A header the hook asked to carry has a name HTTP refuses.
    HeaderName(InvalidHeaderName),
    /// A header the hook asked to carry has a value HTTP refuses.
    HeaderValue(InvalidHeaderValue),
    /// The HTTP client would not build.
    Client(reqwest::Error),
    /// The server did not answer within [`CALL_TIMEOUT`].
    Timeout(Elapsed),
    /// The server answered a request with an error.
    Service(Box<ServiceError>),
}

impl fmt::Display for McpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(_) => f.write_str("the server's process would not start"),
            Self::Initialize(_) => f.write_str("the server would not initialize"),
            Self::Refused(refused) => write!(f, "[{}] {}", refused.code, refused.detail),
            Self::Url(_) => f.write_str("the admitted URL is not one"),
            Self::HeaderName(_) => f.write_str("an admitted header's name is not one"),
            Self::HeaderValue(_) => f.write_str("an admitted header's value is not one"),
            Self::Client(_) => f.write_str("the HTTP client would not build"),
            Self::Timeout(_) => write!(f, "the server did not answer within {CALL_TIMEOUT:?}"),
            Self::Service(_) => f.write_str("the server would not answer"),
        }
    }
}

impl std::error::Error for McpError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Spawn(source) => Some(source),
            Self::Initialize(source) => Some(&**source),
            Self::Refused(_) => None,
            Self::Url(source) => Some(source),
            Self::HeaderName(source) => Some(source),
            Self::HeaderValue(source) => Some(source),
            Self::Client(source) => Some(source),
            Self::Timeout(source) => Some(source),
            Self::Service(source) => Some(&**source),
        }
    }
}

impl From<Refused> for McpError {
    fn from(refused: Refused) -> Self {
        Self::Refused(refused)
    }
}

impl From<EnvError> for McpError {
    fn from(source: EnvError) -> Self {
        Self::Spawn(source)
    }
}

impl From<ClientInitializeError> for McpError {
    fn from(source: ClientInitializeError) -> Self {
        Self::Initialize(Box::new(source))
    }
}

impl From<url::ParseError> for McpError {
    fn from(source: url::ParseError) -> Self {
        Self::Url(source)
    }
}

impl From<InvalidHeaderName> for McpError {
    fn from(source: InvalidHeaderName) -> Self {
        Self::HeaderName(source)
    }
}

impl From<InvalidHeaderValue> for McpError {
    fn from(source: InvalidHeaderValue) -> Self {
        Self::HeaderValue(source)
    }
}

impl From<reqwest::Error> for McpError {
    fn from(source: reqwest::Error) -> Self {
        Self::Client(source)
    }
}

impl From<Elapsed> for McpError {
    fn from(source: Elapsed) -> Self {
        Self::Timeout(source)
    }
}

impl From<ServiceError> for McpError {
    fn from(source: ServiceError) -> Self {
        Self::Service(Box::new(source))
    }
}

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
    pub async fn stdio(name: impl Into<String>, env: Arc<dyn Env>, spawn: &Spawn) -> Result<Self> {
        let process = env.spawn(spawn).await?;
        let transport = EnvStdio::new(env, process);
        let service = ().serve(transport).await?;
        Ok(Self {
            name: name.into(),
            service,
        })
    }

    /// Connects to the server at `url` over streamable HTTP, once `outbound`
    /// admits it and its host is not a private address.
    ///
    /// # Errors
    /// The hook refused the URL, its host is or resolves to a private
    /// address (`[egress_denied]`), or the server would not initialize.
    pub async fn http(name: impl Into<String>, url: &str, outbound: &dyn Outbound) -> Result<Self> {
        let admitted = outbound.connect(TOOL, url).await?;
        let lookup = outbound.lookup();
        let target = Url::parse(&admitted.url)?;
        reachable(&target, &*lookup).await?;
        let mut headers = HashMap::new();
        for (name, value) in admitted.headers {
            let name = HeaderName::try_from(name)?;
            let value = HeaderValue::try_from(value)?;
            headers.insert(name, value);
        }
        let config =
            StreamableHttpClientTransportConfig::with_uri(admitted.url).custom_headers(headers);
        let client = client(CALL_TIMEOUT, lookup).build()?;
        let transport = StreamableHttpClientTransport::with_client(client, config);
        let service = ().serve(transport).await?;
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
    pub async fn tools(self: &Arc<Self>) -> Result<Vec<McpTool>> {
        let listed = tokio::time::timeout(CALL_TIMEOUT, self.service.list_all_tools()).await??;
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

#[cfg(test)]
#[path = "http_tests.rs"]
mod http_tests;
