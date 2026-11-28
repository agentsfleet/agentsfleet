//! Disclosure of page citations at the boundary shared by live and saved calls.
use std::borrow::Cow;
use std::sync::Arc;

use af_secrets::{Clean, Scrub};

use af_wire::tool_trace::{ToolCallStatus, WebSource, encoded_len};
use afr_tools::catalog::{WEB_FETCH, WEB_SEARCH};
use guffbolt::{Call, ToolOutput};
use serde_json::Value;
use url::Url;

/// An immutable view of all values known to the run when a call opens or closes.
pub(super) struct Disclosure<'a> {
    statics: &'a Scrub,
    minted: Option<Arc<Scrub>>,
}

impl<'a> Disclosure<'a> {
    pub(super) fn new(statics: &'a Scrub, minted: Option<&af_egress::MintedMask>) -> Self {
        Self {
            statics,
            minted: minted.and_then(af_egress::MintedMask::snapshot),
        }
    }

    fn masks(&self, text: &str) -> bool {
        matches!(self.statics.text(text), Cow::Owned(_))
            || self
                .minted
                .as_ref()
                .is_some_and(|scrub| matches!(scrub.text(text), Cow::Owned(_)))
    }

    pub(super) fn clean(&self, text: String) -> Clean<String> {
        let text = match &self.minted {
            Some(minted) => minted.clean(text).into_inner(),
            None => text,
        };
        self.statics.clean(text)
    }

    fn clean_json(&self, fields: Value) -> Clean<Value> {
        let fields = match &self.minted {
            Some(minted) => minted.clean_json(fields).into_inner(),
            None => fields,
        };
        self.statics.clean_json(fields)
    }
}

/// A URL with no embedded credential or secret. A query or fragment can carry
/// a one-use token; neither belongs in a durable citation or argument.
fn display_url(raw: &str, scrub: &Disclosure<'_>) -> Option<Url> {
    if scrub.masks(raw) {
        return None;
    }
    WebSource::display_url(raw)
}

/// Redact the requested URL before the starting frame or full item sees it.
pub(super) fn arguments(call: &Call, scrub: &Disclosure<'_>) -> Clean<Value> {
    let mut arguments = call.arguments.clone();
    if call.name == WEB_FETCH.name()
        && let Some(field) = arguments.get_mut("url")
    {
        *field = field
            .as_str()
            .and_then(|raw| display_url(raw, scrub))
            .map_or(Value::Null, |url| Value::String(url.into()));
    }
    scrub.clean_json(arguments)
}

/// Only the tool's structured results can introduce a citation. Replacing the
/// corresponding URLs in its output also keeps signed links out of text previews.
pub(super) fn returned(
    call: &Call,
    output: &mut ToolOutput,
    scrub: &Disclosure<'_>,
    status: ToolCallStatus,
) -> Vec<WebSource> {
    let fetched = call.name == WEB_FETCH.name();
    let searched = call.name == WEB_SEARCH.name();
    let requested = call.arguments.get("url").and_then(Value::as_str);
    let requested_page = requested.and_then(|raw| display_url(raw, scrub));
    if fetched && let Some(raw) = requested.filter(|raw| !raw.is_empty()) {
        let safe = requested_page.as_ref().map(Url::as_str);
        if safe != Some(raw) {
            output.text = output.text.replace(raw, safe.unwrap_or("[redacted URL]"));
        }
    }
    let mut sources = Vec::new();
    for (index, raw) in output.take_sources().into_iter().enumerate() {
        let shown = display_url(&raw.url, scrub).map(|url| {
            if fetched && (index != 0 || requested_page.as_ref() != Some(&url)) {
                url.origin().ascii_serialization()
            } else {
                url.to_string()
            }
        });
        if !raw.url.is_empty() && shown.as_deref() != Some(raw.url.as_str()) {
            output.text = output
                .text
                .replace(&raw.url, shown.as_deref().unwrap_or("[redacted URL]"));
        }
        let title = raw.title.and_then(|title| {
            if title.is_empty() {
                return None;
            }
            if fetched || !WebSource::displayable_title(&title) || raw.url.contains(['?', '#']) {
                if !WebSource::displayable_title(&title) {
                    output.text = output.text.replace(&title, "[redacted title]");
                }
                return None;
            }
            Some(scrub.clean(title).into_inner())
        });
        if let Some(url) = shown
            && status == ToolCallStatus::Succeeded
            && (fetched || searched)
        {
            sources.push(WebSource { url, title });
        }
    }
    sources
}

/// Fit whole entries. The field is omitted while empty; the first source
/// contributes the field name and brackets, later ones contribute a separator.
pub(super) fn fit(sources: &mut Vec<WebSource>, mut room: usize) {
    let count = sources
        .iter()
        .enumerate()
        .take_while(|(index, source)| {
            let overhead = if *index == 0 {
                br#","sources":[]"#.len()
            } else {
                1
            };
            let bytes = encoded_len(source).saturating_add(overhead);
            if bytes > room {
                return false;
            }
            room -= bytes;
            true
        })
        .count();
    sources.truncate(count);
}
