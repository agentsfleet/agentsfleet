//! Fenced runner calls to the platform's web vendors. Each request is sent once.

use std::time::Duration;

use af_validate::{TEXT_MAX_CHARS, nul_free};
use garde::Validate;
use serde::{Deserialize, Serialize};

/// One daemon budget for key resolution, reservations, vendor calls and validation.
pub const WEB_DEADLINE: Duration = Duration::from_secs(12);

/// Search the public web for the given run.
#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct SearchRequest {
    /// The current holder's fence.
    #[garde(skip)]
    pub fencing_token: u64,
    /// The model's one-based tool call number.
    #[garde(range(min = 1))]
    pub call_number: u64,
    /// Search terms submitted to the platform's configured vendors.
    #[garde(length(chars, max = TEXT_MAX_CHARS), custom(nonblank), custom(nul_free))]
    pub query: String,
}

fn nonblank<C: ?Sized>(value: &str, _: &C) -> garde::Result {
    if value.trim().is_empty() {
        Err(garde::Error::new("query must contain a search term"))
    } else {
        Ok(())
    }
}

/// Read a single public page through a vendor.
#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ExtractRequest {
    /// The current holder's fence.
    #[garde(skip)]
    pub fencing_token: u64,
    /// The model's one-based tool call number.
    #[garde(range(min = 1))]
    pub call_number: u64,
    /// The page's HTTPS URL. The daemon checks destination policy before dispatch.
    #[garde(length(chars, min = 1, max = TEXT_MAX_CHARS), custom(nul_free))]
    pub url: String,
}

/// One web source in vendor order.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SearchResult {
    /// Source URL.
    pub url: String,
    /// Page title, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Publication date, when supplied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<String>,
    /// Relevant passages from this page.
    pub excerpts: Vec<String>,
}

/// The first vendor's successful search.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SearchResponse {
    /// The vendor that answered.
    pub vendor: String,
    /// Matching pages in relevance order.
    pub results: Vec<SearchResult>,
}

/// One extracted page and its requested URL.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ExtractResponse {
    /// The vendor that read the page.
    pub vendor: String,
    /// The requested page; vendors may follow redirects internally.
    pub url: String,
    /// Page title, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Full Markdown content returned by the reader.
    pub content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_requires_terms_and_a_positive_call_number() {
        for (number, query, valid) in [
            (1, " rust ", true),
            (1, " \t", false),
            (1, "\u{2003}", false),
            (1, "a\0b", false),
            (0, "rust", false),
        ] {
            let request = SearchRequest {
                fencing_token: 7,
                call_number: number,
                query: query.to_owned(),
            };
            assert_eq!(request.validate().is_ok(), valid);
        }
    }
}
