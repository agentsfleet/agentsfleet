//! `http_request`: one HTTPS request under the network policy and the
//! daemon's origin rules.
//!
//! The arguments are `NullClaw`'s (`url`, `method`, `headers`, `body`), and so
//! is the answer's shape, `Status: N` then the body. What differs is where the
//! policy lives: every rule is the lease's guard's (`afr_egress`), so this
//! handler only drafts the request and words the answer.

use std::collections::BTreeMap;

use afr_egress::{Draft, Inbound, Placement, RESPONSE_MAX_BYTES};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::catalog::{Entry, HTTP_REQUEST};
use crate::egress::{self, SharedTransport};
use crate::handler::Handler;
use crate::runtime::{ToolContext, ToolOutput};

/// The method a request that names none is sent with.
const DEFAULT_METHOD: &str = "GET";

/// `http_request`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    /// An https URL on a host the fleet's network policy lists. Its whole
    /// host may be written `${secrets.NAME.host}`.
    url: String,
    /// `GET` (the default), `HEAD`, `POST`, `PUT`, `PATCH`, `DELETE` or
    /// `OPTIONS`.
    method: Option<String>,
    /// Request headers. `Authorization` may carry `${secrets.NAME.FIELD}`
    /// placeholders, put in place when the request is sent; no other header
    /// may carry one.
    headers: Option<BTreeMap<String, String>>,
    /// The body, sent as written; it may carry no placeholder.
    body: Option<String>,
}

/// Sends one request through the lease's guard.
#[derive(Debug)]
pub(crate) struct HttpRequest {
    transport: SharedTransport,
}

impl HttpRequest {
    /// A handler sending through `transport`.
    pub(crate) fn new(transport: SharedTransport) -> Self {
        Self { transport }
    }
}

#[async_trait::async_trait]
impl Handler for HttpRequest {
    const ENTRY: &'static Entry = &HTTP_REQUEST;
    const DESCRIPTION: &'static str = "Send one HTTPS request to a host this fleet's \
        network policy allows, and read back its status and up to 1 MiB of its body. \
        Write a credential as ${secrets.NAME.FIELD} in the Authorization header; it is \
        put in place when the request is sent.";
    type Arguments = Request;

    async fn run(&self, arguments: Request, context: ToolContext<'_, '_>) -> ToolOutput {
        let draft = Draft {
            method: arguments
                .method
                .unwrap_or_else(|| DEFAULT_METHOD.to_owned()),
            url: arguments.url,
            headers: arguments.headers.unwrap_or_default().into_iter().collect(),
            body: arguments.body,
            placement: Placement::Authorization,
        };
        match egress::send(Self::ENTRY, self.transport.as_ref(), context.lease, draft).await {
            Ok(inbound) => egress::answered(inbound.status, render(&inbound)),
            Err(refused) => refused,
        }
    }
}

/// What the model reads back: the status, where a redirect points, and the
/// body, with a line saying where it was cut.
fn render(inbound: &Inbound) -> String {
    let location = inbound
        .location
        .as_deref()
        .map(|origin| format!("Location: {origin}\n"))
        .unwrap_or_default();
    let cut = if inbound.truncated {
        format!("\n[Response truncated at {RESPONSE_MAX_BYTES} bytes]")
    } else {
        String::new()
    };
    format!(
        "Status: {}\n{location}\nResponse Body:\n{}{cut}",
        inbound.status, inbound.body
    )
}

#[cfg(test)]
#[path = "http_request/tests.rs"]
mod tests;
