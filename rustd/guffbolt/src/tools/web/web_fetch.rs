//! `web_fetch`: a credential-free `GET` of a page's text.
//!
//! The same hook as `http_request` with less reach: one method, no header, no
//! body, and no placeholder anywhere, so a page fetched for reading never
//! carries a run's credential. An HTML page comes back as its text, through
//! a real HTML parser (`html2text`).

use schemars::JsonSchema;
use serde::Deserialize;

use super::answered;
use crate::tool::ToolOutput;
use crate::tools::outbound::{Placement, Redirects, Request};
use crate::tools::{Context, Handler};

/// The one method a fetch sends.
const METHOD: &str = "GET";
/// How many characters a fetch reads back when the model names no limit.
pub const DEFAULT_MAX_CHARS: usize = 50_000;
/// The columns a page's text wraps at: wide, so a paragraph stays a line.
const TEXT_WIDTH: usize = 200;
/// What marks a response as a page to read as text.
const HTML: &str = "html";

/// `web_fetch`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Fetch {
    /// An https URL on a host the network policy lists.
    url: String,
    /// The most characters to read back: 50000 when absent.
    max_chars: Option<usize>,
}

/// Fetches one page's text through the run's outbound hook.
#[derive(Debug, Clone, Copy, Default)]
pub struct WebFetch;

impl Handler for WebFetch {
    const NAME: &'static str = "web_fetch";
    const DESCRIPTION: &'static str = "Fetch one https page from a host the network \
        policy allows and read back its text. No credential is sent.";
    type Arguments = Fetch;

    async fn run(&self, arguments: Fetch, context: Context<'_>) -> ToolOutput {
        let draft = Request {
            method: METHOD.to_owned(),
            url: arguments.url,
            headers: Vec::new(),
            body: None,
            placement: Placement::Nowhere,
            redirects: Redirects::Stop,
        };
        match context.outbound.send(Self::NAME, draft).await {
            Ok(inbound) => {
                let page = inbound
                    .content_type
                    .as_deref()
                    .is_some_and(|media| media.contains(HTML));
                // Masked again once decoded: an entity-encoded echo of a minted
                // token only reads as the token after the page becomes text.
                let text = if page {
                    context.outbound.mask(text_of(&inbound.body)).await
                } else {
                    inbound.body
                };
                let limit = arguments.max_chars.unwrap_or(DEFAULT_MAX_CHARS);
                answered(inbound.status, at_most(text, limit))
            }
            Err(refused) => refused.into(),
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
