//! `list_mcp_resources`, `list_mcp_resource_templates` and
//! `read_mcp_resource`: what the run's MCP servers hold besides tools, in
//! Codex's shape (`core/src/tools/handlers/mcp_resource.rs`).

use std::sync::Arc;

use rmcp::model::ReadResourceRequestParams;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

use super::{CALL_TIMEOUT, Server};
use crate::tool::ToolOutput;
use crate::tools::{Context, Handler, code};

/// What a listing over no resource reads back.
const NOTHING_LISTED: &str = "no server lists any";

/// The arguments naming which servers to list.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Servers {
    /// The server to list, by name; every server when absent.
    #[serde(default)]
    server: Option<String>,
}

/// `read_mcp_resource`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Read {
    /// The server that holds the resource, by name.
    server: String,
    /// The resource's URI, as a listing named it.
    uri: String,
}

/// The run's servers, shared by the three resource tools.
type Shared = Arc<[Arc<Server>]>;

/// The servers `named` picks: the one so named, or every one.
fn picked<'s>(servers: &'s Shared, named: Option<&str>) -> Result<Vec<&'s Server>, ToolOutput> {
    named.map_or_else(
        || Ok(servers.iter().map(AsRef::as_ref).collect()),
        |name| one(servers, name).map(|server| vec![server]),
    )
}

/// The server named `name`.
fn one<'s>(servers: &'s Shared, name: &str) -> Result<&'s Server, ToolOutput> {
    servers
        .iter()
        .find(|server| server.name() == name)
        .map(AsRef::as_ref)
        .ok_or_else(|| not_found(name))
}

/// What a call naming no server of the run reads back.
fn not_found(name: &str) -> ToolOutput {
    ToolOutput::failed(
        code::MCP_SERVER_NOT_FOUND,
        &format!("{name} is not one of this run's MCP servers"),
    )
}

/// Each item as one JSON line, prefixed by its server; what a listing over
/// nothing reads otherwise.
fn listed<T: Serialize>(found: &[(&str, T)]) -> String {
    let lines: Vec<String> = found
        .iter()
        .map(|(server, item)| {
            let item = serde_json::to_value(item).unwrap_or_default();
            format!("{server}: {item}")
        })
        .collect();
    if lines.is_empty() {
        NOTHING_LISTED.to_owned()
    } else {
        lines.join("\n")
    }
}

/// Lists the resources the run's MCP servers hold.
#[derive(Debug, Clone)]
pub struct ListMcpResources {
    servers: Shared,
}

/// Lists the resource templates the run's MCP servers offer.
#[derive(Debug, Clone)]
pub struct ListMcpResourceTemplates {
    servers: Shared,
}

/// Reads one resource an MCP server holds.
#[derive(Debug, Clone)]
pub struct ReadMcpResource {
    servers: Shared,
}

impl ListMcpResources {
    /// The tool over `servers`.
    #[must_use]
    pub fn new(servers: impl Into<Arc<[Arc<Server>]>>) -> Self {
        Self {
            servers: servers.into(),
        }
    }
}

impl ListMcpResourceTemplates {
    /// The tool over `servers`.
    #[must_use]
    pub fn new(servers: impl Into<Arc<[Arc<Server>]>>) -> Self {
        Self {
            servers: servers.into(),
        }
    }
}

impl ReadMcpResource {
    /// The tool over `servers`.
    #[must_use]
    pub fn new(servers: impl Into<Arc<[Arc<Server>]>>) -> Self {
        Self {
            servers: servers.into(),
        }
    }
}

impl Handler for ListMcpResources {
    const NAME: &'static str = "list_mcp_resources";
    const DESCRIPTION: &'static str = "List the resources this run's MCP servers hold: each \
        one's server, URI, name and description. Read one with read_mcp_resource.";
    type Arguments = Servers;

    async fn run(&self, arguments: Servers, _context: Context<'_>) -> ToolOutput {
        let servers = match picked(&self.servers, arguments.server.as_deref()) {
            Ok(servers) => servers,
            Err(refused) => return refused,
        };
        let mut found = Vec::new();
        for server in servers {
            match tokio::time::timeout(CALL_TIMEOUT, server.service.list_all_resources()).await {
                Ok(Ok(resources)) => {
                    found.extend(
                        resources
                            .into_iter()
                            .map(|resource| (server.name(), resource)),
                    );
                }
                Ok(Err(failure)) => {
                    return ToolOutput::failed(code::MCP_FAILED, &failure.to_string());
                }
                Err(elapsed) => return ToolOutput::failed(code::MCP_FAILED, &elapsed.to_string()),
            }
        }
        ToolOutput::succeeded(listed(&found))
    }
}

impl Handler for ListMcpResourceTemplates {
    const NAME: &'static str = "list_mcp_resource_templates";
    const DESCRIPTION: &'static str = "List the resource templates this run's MCP servers \
        offer: each one's server, URI template, name and description.";
    type Arguments = Servers;

    async fn run(&self, arguments: Servers, _context: Context<'_>) -> ToolOutput {
        let servers = match picked(&self.servers, arguments.server.as_deref()) {
            Ok(servers) => servers,
            Err(refused) => return refused,
        };
        let mut found = Vec::new();
        for server in servers {
            let listing = server.service.list_all_resource_templates();
            match tokio::time::timeout(CALL_TIMEOUT, listing).await {
                Ok(Ok(templates)) => {
                    found.extend(
                        templates
                            .into_iter()
                            .map(|template| (server.name(), template)),
                    );
                }
                Ok(Err(failure)) => {
                    return ToolOutput::failed(code::MCP_FAILED, &failure.to_string());
                }
                Err(elapsed) => return ToolOutput::failed(code::MCP_FAILED, &elapsed.to_string()),
            }
        }
        ToolOutput::succeeded(listed(&found))
    }
}

impl Handler for ReadMcpResource {
    const NAME: &'static str = "read_mcp_resource";
    const DESCRIPTION: &'static str = "Read one resource an MCP server of this run holds, by \
        the server's name and the resource's URI, and read back its text.";
    type Arguments = Read;

    async fn run(&self, arguments: Read, _context: Context<'_>) -> ToolOutput {
        let server = match one(&self.servers, &arguments.server) {
            Ok(server) => server,
            Err(refused) => return refused,
        };
        let params = ReadResourceRequestParams::new(arguments.uri);
        match tokio::time::timeout(CALL_TIMEOUT, server.service.read_resource(params)).await {
            Ok(Ok(read)) => {
                let value = serde_json::to_value(&read).unwrap_or_default();
                let parts: Vec<String> = value
                    .get("contents")
                    .and_then(Value::as_array)
                    .map_or_default(|contents| contents.iter().map(content).collect());
                ToolOutput::succeeded(parts.join("\n"))
            }
            Ok(Err(failure)) => ToolOutput::failed(code::MCP_FAILED, &failure.to_string()),
            Err(elapsed) => ToolOutput::failed(code::MCP_FAILED, &elapsed.to_string()),
        }
    }
}

/// One part of a read resource: its text, or a line naming its bytes.
fn content(part: &Value) -> String {
    part.get("text").and_then(Value::as_str).map_or_else(
        || {
            let uri = part.get("uri").and_then(Value::as_str).unwrap_or_default();
            let kind = part
                .get("mimeType")
                .and_then(Value::as_str)
                .unwrap_or("bytes");
            format!("[{kind} {uri}]")
        },
        str::to_owned,
    )
}
