//! Pages a successful web tool actually returned.
use super::TraceRejection;
use garde::Validate;
use serde::{Deserialize, Serialize};
use url::Url;

/// A sanitized page citation shared by a completion, a trace row and its saved item.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct WebSource {
    /// Displayable URL, stripped of credentials, query and fragment by the runner.
    #[garde(custom(displayable_url))]
    pub url: String,
    /// Displayable page title, when supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[garde(inner(custom(source_text)))]
    pub title: Option<String>,
}

impl WebSource {
    /// The location safe to cite for a requested page or search result. The
    /// run separately refuses values that match its credential masks.
    #[must_use]
    pub fn display_url(raw: &str) -> Option<Url> {
        if raw.chars().any(char::is_control) {
            return None;
        }
        let mut url = Url::parse(raw).ok()?;
        if !matches!(url.scheme(), "https" | "http")
            || url.host().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return None;
        }
        url.set_query(None);
        url.set_fragment(None);
        Some(url)
    }

    /// A title is display text. An embedded URL or query can reveal a second destination.
    #[must_use]
    pub fn displayable_title(title: &str) -> bool {
        !title.chars().any(char::is_control)
            && !title.contains("://")
            && !title
                .split_once('?')
                .is_some_and(|(_, rest)| rest.contains('='))
    }
}

fn source_text<C: ?Sized>(value: &str, _context: &C) -> garde::Result {
    if !super::free_of_nul(value) {
        Err(garde::Error::new(TraceRejection::HoldsNul.as_str()))
    } else if WebSource::displayable_title(value) {
        Ok(())
    } else {
        Err(garde::Error::new("undisclosable source title"))
    }
}

fn displayable_url<C: ?Sized>(value: &str, _context: &C) -> garde::Result {
    let url = WebSource::display_url(value)
        .filter(|url| url.as_str() == value || url.origin().ascii_serialization() == value);
    if url.is_some() {
        Ok(())
    } else {
        Err(garde::Error::new("undisclosable source URL"))
    }
}

/// An independently established redirect can name only its origin. Any source
/// with a path on a fetch must equal the requested URL on the same stored call.
pub(crate) fn requested_source(
    tool: &str,
    args: &serde_json::Map<String, serde_json::Value>,
    sources: &[WebSource],
    bounded: bool,
) -> bool {
    if tool != "web_fetch" {
        return true;
    }
    let Some(page) = sources.first() else {
        return true;
    };
    if Url::parse(&page.url).is_ok_and(|url| url.path() == "/") {
        return true;
    }
    let arg = args.get("url").and_then(serde_json::Value::as_str);
    // The saved trace cuts arguments at the leaf limit and may drop the
    // whole object. The uncut item and live payload keep the complete URL.
    match arg {
        Some(url) if bounded && url.len() >= super::ARGS_LEAF_MAX_BYTES => {
            page.url.starts_with(url)
        }
        Some(url) => page.url == url,
        None => bounded,
    }
}

/// The first fetch citation is the requested page. Later citations can only
/// disclose an origin. A failed call or a non-web tool supplies no sources.
pub(crate) fn source_scope(tool: &str, succeeded: bool, sources: &[WebSource]) -> bool {
    sources.is_empty()
        || succeeded
            && match tool {
                "web_search" => true,
                "web_fetch" => sources.iter().enumerate().all(|(i, source)| {
                    source.title.is_none()
                        && (i == 0 || Url::parse(&source.url).is_ok_and(|url| url.path() == "/"))
                }),
                _ => false,
            }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_trace::ToolTrace;
    use serde_json::json;

    #[test]
    fn untrusted_sources_cannot_persist_active_urls_signed_destinations_or_redirect_titles()
    -> Result<(), serde_json::Error> {
        let safe = json!({"url":"https://docs.test/requested"});
        for (name, status, sources) in [
            (
                "web_fetch",
                "succeeded",
                json!([safe, {"url":"https://final.test/signed/path"}]),
            ),
            (
                "web_fetch",
                "succeeded",
                json!([{"url":"https://docs.test/requested","title":"https://final.test/path"}]),
            ),
            (
                "web_search",
                "succeeded",
                json!([{"url":"javascript:alert(1)"}]),
            ),
            (
                "web_search",
                "succeeded",
                json!([{"url":"https://user@docs.test/"}]),
            ),
            (
                "web_search",
                "succeeded",
                json!([{"url":"https://docs.test/?token=private"}]),
            ),
            (
                "web_search",
                "succeeded",
                json!([{"url":"https://docs.test/#fragment"}]),
            ),
            (
                "web_search",
                "succeeded",
                json!([{"url":"https://docs.test/","title":"?sig=private"}]),
            ),
            ("shell", "succeeded", json!([safe])),
            ("web_search", "failed", json!([safe])),
        ] {
            let encoded = json!({"calls":[{"call_id":"1","name":name,"arguments":{},"status":status,"duration_ms":1,"sources":sources}]}).to_string();
            let parsed: ToolTrace<'_> = serde_json::from_str(&encoded)?;
            assert_eq!(
                parsed.validate(),
                Err(TraceRejection::Malformed),
                "{encoded}"
            );
        }
        Ok(())
    }
}
