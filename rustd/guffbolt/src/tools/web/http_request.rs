//! `http_request`: one HTTPS request under the network policy and the
//! outbound hook's origin rules.
//!
//! The arguments are `url`, `method`, `headers` and `body`, and the answer is
//! `Status: N` then the body. Every policy rule is the run's outbound hook's,
//! so this handler only drafts the request and words the answer.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::Deserialize;

use super::answered;
use crate::tool::ToolOutput;
use crate::tools::outbound::{
    Outbound, Placement, RESPONSE_MAX_BYTES, Redirects, Request as Draft, Response,
};
use crate::tools::{Context, Handler};

/// The method a request that names none is sent with.
const DEFAULT_METHOD: &str = "GET";
const INCOMPLETE_BODY: &str = "[Incomplete response body omitted; the source was truncated]";

/// `http_request`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {
    /// An https URL on a host the network policy lists. Its whole
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

backed! {
    /// Sends one request through the run's outbound hook.
    HttpRequest over Outbound
}

impl Handler for HttpRequest<'_> {
    const NAME: &'static str = "http_request";
    const DESCRIPTION: &'static str = "Send one HTTPS request to a host the network \
        policy allows, and read back its status and up to 1 MiB of a complete body. \
        Truncated bodies are omitted with an explicit incomplete-source notice. \
        Write a credential as ${secrets.NAME.FIELD} in the Authorization header; it is \
        put in place when the request is sent. A GET without a body follows up to \
        three redirects, admitting every target under the same policy. All supplied \
        headers are removed permanently after an origin change; temporary URLs stay private.";
    type Arguments = Request;

    async fn run(&self, arguments: Request, _context: Context<'_>) -> ToolOutput {
        let method = arguments
            .method
            .unwrap_or_else(|| DEFAULT_METHOD.to_owned());
        // A bodyless GET follows its redirects, each target admitted again.
        let redirects = if method.eq_ignore_ascii_case(DEFAULT_METHOD) && arguments.body.is_none() {
            Redirects::Follow
        } else {
            Redirects::Stop
        };
        let draft = Draft {
            method,
            url: arguments.url,
            headers: arguments.headers.unwrap_or_default().into_iter().collect(),
            body: arguments.body,
            placement: Placement::Authorization,
            redirects,
        };
        match self.backend.send(Self::NAME, draft).await {
            Ok(inbound) => answered(inbound.status, render(&inbound)),
            Err(refused) => refused.into(),
        }
    }
}

/// What the model reads back: the status, where a redirect points, and the
/// body. A truncated body is unusable as a complete source; do not spend the
/// model's context on its potentially megabyte-sized prefix.
fn render(inbound: &Response) -> String {
    let location = inbound
        .location
        .as_deref()
        .map_or_default(|origin| format!("Location: {origin}\n"));
    let cut = if inbound.truncated {
        format!("[Response truncated at {RESPONSE_MAX_BYTES} bytes]\n")
    } else {
        String::new()
    };
    let content_type = inbound
        .content_type
        .as_ref()
        .map_or_default(|value| format!("Content-Type: {value}\n"));
    let lossy = if inbound.lossy {
        "[Response contains replaced non-UTF-8 bytes]\n"
    } else {
        ""
    };
    let hops = if inbound.followed_redirects > 0 {
        format!("Redirects followed: {}\n", inbound.followed_redirects)
    } else {
        String::new()
    };
    let body = if inbound.truncated && !inbound.body.is_empty() {
        INCOMPLETE_BODY
    } else {
        &inbound.body
    };
    format!(
        "Status: {}\n{location}{content_type}{hops}{cut}{lossy}\nResponse Body:\n{body}",
        inbound.status
    )
}

#[cfg(test)]
#[path = "http_request_body_tests.rs"]
mod body_tests;
