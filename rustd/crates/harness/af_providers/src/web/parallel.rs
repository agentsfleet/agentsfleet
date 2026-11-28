//! Parallel Fast search and full-content extraction for a single URL.

use std::time::Duration;

use af_crypto::secret::SecretString;
use af_wire::web::{ExtractResponse, SearchResult};
use futures_util::future::BoxFuture;
use reqwest::{Client, Url};
use serde::Deserialize;
use serde_json::json;

use super::{
    RESULT_COUNT, Vendor, WebFailure,
    client::{self, Object},
};
use crate::Result;

const SEARCH: &str = "https://api.parallel.ai/v1/search";
const EXTRACT: &str = "https://api.parallel.ai/v1/extract";

/// Parallel's two paid operations share one pool and hold no credentials.
#[derive(Clone, Debug)]
pub struct Parallel {
    client: Client,
    search: Url,
    extract: Url,
}

impl Parallel {
    /// Connect to Parallel's reviewed endpoints with automatic retry disabled.
    ///
    /// # Errors
    /// Returns the endpoint or guarded HTTP client construction failure.
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: client::guarded()?,
            search: Url::parse(SEARCH)?,
            extract: Url::parse(EXTRACT)?,
        })
    }

    /// Inject the fixture's client and endpoints for a real socket test.
    #[cfg(any(test, feature = "test-util"))]
    #[must_use]
    pub const fn with_endpoints(client: Client, search: Url, extract: Url) -> Self {
        Self {
            client,
            search,
            extract,
        }
    }

    /// Read one URL as Markdown using the vendor's dynamic fetch policy.
    ///
    /// # Errors
    /// Distinguishes per-page HTTP errors from vendor refusals, ambiguous
    /// associations and incomplete content. Only the requested URL is exposed.
    pub async fn extract(
        &self,
        key: &SecretString,
        url: &str,
        timeout: Duration,
    ) -> Result<ExtractResponse, WebFailure> {
        let request = json!({"urls": [url], "advanced_settings": {"full_content": true}});
        let reply: ExtractReply =
            client::post(&self.client, &self.extract, key, request, timeout).await?;
        reply.page(url, key)
    }
}

impl Vendor for Parallel {
    fn search<'a>(
        &'a self,
        key: &'a SecretString,
        query: &'a str,
        timeout: Duration,
    ) -> BoxFuture<'a, Result<Vec<SearchResult>, WebFailure>> {
        Box::pin(async move {
            let request = json!({
                "mode": "fast", "search_queries": [query],
                "advanced_settings": {"max_results": RESULT_COUNT},
            });
            let reply: SearchReply =
                client::post(&self.client, &self.search, key, request, timeout).await?;
            let rows = reply
                .results
                .into_iter()
                .map(|Object(row)| SearchResult {
                    url: row.url,
                    title: row.title,
                    published: row.publish_date,
                    excerpts: row.excerpts,
                })
                .collect();
            client::results(rows, key)
        })
    }
}

#[derive(Deserialize)]
struct SearchReply {
    results: Vec<Object<Row>>,
}

#[derive(Deserialize)]
struct Row {
    url: String,
    title: Option<String>,
    publish_date: Option<String>,
    excerpts: Vec<String>,
}

#[derive(Deserialize)]
struct ExtractReply {
    results: Vec<Object<Page>>,
    errors: Vec<Object<PageError>>,
}

#[derive(Deserialize)]
struct Page {
    url: String,
    title: Option<String>,
    full_content: String,
}

#[derive(Deserialize)]
struct PageError {
    url: String,
    error_type: String,
    // Parallel supplies an explicit null when the origin never replied.
    #[serde(deserialize_with = "Option::deserialize")]
    http_status_code: Option<u16>,
}

impl ExtractReply {
    fn page(self, requested: &str, key: &SecretString) -> Result<ExtractResponse, WebFailure> {
        let requested_url = client::checked_url(requested)?;
        let mut pages = self.results.into_iter();
        let mut errors = self.errors.into_iter();
        match (pages.next(), errors.next()) {
            (Some(Object(page)), None) if pages.next().is_none() => {
                if client::checked_url(&page.url)? != requested_url
                    || page.full_content.trim().is_empty()
                    || [
                        Some(requested),
                        page.title.as_deref(),
                        Some(&page.full_content),
                    ]
                    .into_iter()
                    .flatten()
                    .any(|text| text.contains(key.expose()))
                {
                    return Err(WebFailure::InvalidResponse);
                }
                Ok(ExtractResponse {
                    vendor: "parallel".to_owned(),
                    url: requested.to_owned(),
                    title: page.title,
                    content: page.full_content,
                })
            }
            (None, Some(Object(error))) if errors.next().is_none() => {
                if client::checked_url(&error.url)? != requested_url {
                    return Err(WebFailure::InvalidResponse);
                }
                match (error.error_type.as_str(), error.http_status_code) {
                    ("http_error", Some(status @ 400..=599)) => Err(WebFailure::Page { status }),
                    ("connect_error", None) => Err(WebFailure::Transport),
                    _ => Err(WebFailure::InvalidResponse),
                }
            }
            _ => Err(WebFailure::InvalidResponse),
        }
    }
}
