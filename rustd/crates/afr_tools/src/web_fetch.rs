//! `web_fetch`: a credential-free `GET` of a page's text.
//!
//! The same guard as `http_request` with less reach: one method, no header, no
//! body, and no placeholder anywhere, so a page fetched for reading never
//! carries a fleet's credential. An HTML page comes back as its text, through
//! a real HTML parser (`html2text`).

use afr_egress::{Draft, Placement};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::catalog::{Entry, WEB_FETCH};
use crate::egress::{self, SharedTransport};
use crate::handler::Handler;
use crate::runtime::{ToolContext, ToolOutput};

/// The one method a fetch sends.
const METHOD: &str = "GET";
/// How many characters a fetch reads back when the model names no limit.
const DEFAULT_MAX_CHARS: usize = 50_000;
/// The columns a page's text wraps at: wide, so a paragraph stays a line.
const TEXT_WIDTH: usize = 200;
/// What marks a response as a page to read as text.
const HTML: &str = "html";

/// `web_fetch`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Fetch {
    /// An https URL on a host the fleet's network policy lists.
    url: String,
    /// The most characters to read back: 50000 when absent.
    max_chars: Option<usize>,
}

/// Fetches one page's text through the lease's guard.
#[derive(Debug)]
pub(crate) struct WebFetch {
    transport: SharedTransport,
}

impl WebFetch {
    /// A handler sending through `transport`.
    pub(crate) fn new(transport: SharedTransport) -> Self {
        Self { transport }
    }
}

#[async_trait::async_trait]
impl Handler for WebFetch {
    const ENTRY: &'static Entry = &WEB_FETCH;
    const DESCRIPTION: &'static str = "Fetch one https page from a host this fleet's \
        network policy allows and read back its text. No credential is sent.";
    type Arguments = Fetch;

    async fn run(&self, arguments: Fetch, context: ToolContext<'_, '_>) -> ToolOutput {
        let draft = Draft {
            method: METHOD.to_owned(),
            url: arguments.url,
            headers: Vec::new(),
            body: None,
            placement: Placement::Nowhere,
        };
        match egress::send(Self::ENTRY, self.transport.as_ref(), context.lease, draft).await {
            Ok(inbound) => {
                let page = inbound
                    .content_type
                    .as_deref()
                    .is_some_and(|media| media.contains(HTML));
                // Masked again once decoded: an entity-encoded echo of a minted
                // token only reads as the token after the page becomes text.
                let text = if page {
                    egress::masked(context.lease, text_of(&inbound.body)).await
                } else {
                    inbound.body
                };
                let limit = arguments.max_chars.unwrap_or(DEFAULT_MAX_CHARS);
                egress::answered(inbound.status, at_most(text, limit))
            }
            Err(refused) => refused,
        }
    }
}

/// An HTML page's text; the markup itself when the parser cannot read it.
fn text_of(html: &str) -> String {
    html2text::config::plain()
        .raw_mode(true)
        .string_from_read(html.as_bytes(), TEXT_WIDTH)
        .unwrap_or_else(|_unparsed| html.to_owned())
}

/// `text` cut to `limit` characters, saying where and from how many.
fn at_most(text: String, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((cut, _kept)) => {
            let total = text.chars().count();
            let kept = text.get(..cut).unwrap_or_default();
            format!("{kept}\n\n[Content truncated at {limit} chars, total {total} chars]")
        }
        None => text,
    }
}

#[cfg(test)]
#[path = "web_fetch/tests.rs"]
mod tests;
