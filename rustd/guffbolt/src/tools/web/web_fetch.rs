//! Credential-free page reads. The embedder routes the request and masks
//! text, including any secret revealed by decoding HTML entities.

use schemars::JsonSchema;
use serde::Deserialize;
use url::Url;

use super::{SearchBackend, answered};
use crate::Source;
use crate::tool::ToolOutput;
use crate::tools::outbound::{Outbound, Placement, Redirects, Refused, Request, Response};
use crate::tools::process::output::DEFAULT_BYTES;
use crate::tools::{Context, Handler, code};

/// `web_fetch`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Fetch {
    /// An HTTPS URL to read under the run's network policy.
    url: String,
    /// Maximum characters to return. The default output budget also applies.
    max_chars: Option<usize>,
}

/// Fetches one page through the run's web backend or standalone outbound hook.
#[derive(Debug)]
pub struct WebFetch<'a> {
    outbound: &'a dyn Outbound,
    search: Option<&'a dyn SearchBackend>,
}

impl<'a> WebFetch<'a> {
    /// A local page reader over the supplied policy and transport.
    #[must_use]
    pub const fn new(outbound: &'a dyn Outbound) -> Self {
        Self {
            outbound,
            search: None,
        }
    }

    pub(crate) const fn with_search(mut self, search: Option<&'a dyn SearchBackend>) -> Self {
        self.search = search;
        self
    }

    async fn read(
        &self,
        url: &str,
        context: Context<'_>,
    ) -> Result<(Response, Option<String>), Refused> {
        if let Some(search) = self.search {
            return search.extract(context.call_ref, url).await;
        }
        let draft = Request {
            method: "GET".to_owned(),
            url: url.to_owned(),
            headers: Vec::new(),
            body: None,
            placement: Placement::Nowhere,
            redirects: Redirects::Follow,
        };
        self.outbound
            .send(Self::NAME, draft)
            .await
            .map(|inbound| (inbound, None))
    }
}

impl Handler for WebFetch<'_> {
    const NAME: &'static str = "web_fetch";
    const DESCRIPTION: &'static str = "Read one HTTPS page under the run's network policy. Returns text or Markdown. No credential is sent.";
    type Arguments = Fetch;

    async fn run(&self, arguments: Fetch, context: Context<'_>) -> ToolOutput {
        let (mut page, final_origin) = match self.read(&arguments.url, context).await {
            Ok(page) => page,
            Err(refused) => return refused.into(),
        };
        let requested = arguments.url.as_str();
        let masked = self.outbound.mask(requested.to_owned()).await;
        let shown = (masked == requested)
            .then(|| citation_url(requested))
            .flatten();
        let cleaned = |text: String| {
            let visible = shown.as_deref().unwrap_or("[redacted URL]");
            if (shown.is_some() && !requested.contains(['?', '#'])) || requested.is_empty() {
                return text;
            }
            let text = text.replace(requested, visible);
            if masked != requested && !masked.is_empty() {
                text.replace(&masked, visible)
            } else {
                text
            }
        };
        let status = page.status;
        page.body = cleaned(page.body);
        let text = match content(page, self.outbound).await {
            Ok(text) => text,
            Err(refusal) => return refusal.into(),
        };
        let output = answered(status, bounded(cleaned(text), arguments.max_chars));
        if output.error_code.is_some() {
            return output;
        }
        let sources = shown
            .into_iter()
            .chain(final_origin)
            .map(|url| Source { url, title: None })
            .collect();
        output.with_sources(sources)
    }
}

/// A requested link safe to quote on every output surface. A run's secret
/// mask is checked separately before the URL reaches this function.
fn citation_url(raw: &str) -> Option<String> {
    let mut url = Url::parse(raw).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.has_host()
        || !url.username().is_empty()
        || url.password().is_some()
        || raw.chars().any(char::is_control)
    {
        return None;
    }
    url.set_query(None);
    url.set_fragment(None);
    Some(url.into())
}

async fn content(page: Response, outbound: &dyn Outbound) -> Result<String, Refused> {
    let media = page
        .content_type
        .as_deref()
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if page.lossy || page.body.contains('\0') || !text_media(&media) {
        let media = if media.is_empty() {
            "application/octet-stream"
        } else {
            &media
        };
        return Err(Refused::new(code::UNSUPPORTED_MEDIA, media));
    }
    if matches!(media.as_str(), "text/html" | "application/xhtml+xml") {
        let markdown = super::html::markdown(&page.body).map_err(|_render| {
            Refused::new(
                code::UNSUPPORTED_MEDIA,
                "HTML could not be rendered as text",
            )
        })?;
        return Ok(outbound.mask(markdown).await);
    }
    Ok(page.body)
}

fn text_media(media: &str) -> bool {
    media.is_empty()
        || media.starts_with("text/")
        || media.ends_with("+xml")
        || media.ends_with("+json")
        || matches!(
            media,
            "application/json" | "application/xml" | "application/javascript"
        )
}

/// Keep whole characters within the requested character and default byte budgets.
pub(super) fn bounded(text: String, chars: Option<usize>) -> String {
    let bytes = text.floor_char_boundary(DEFAULT_BYTES.min(text.len()));
    let cut = chars
        .and_then(|limit| text.char_indices().nth(limit))
        .map_or(bytes, |(offset, _)| offset.min(bytes));
    if cut == text.len() {
        return text;
    }
    let kept = text.get(..cut).unwrap_or_default();
    let count = kept.chars().count();
    let total = text.chars().count();
    format!("{kept}\n\n[Content truncated at {count} chars, total {total} chars]")
}
