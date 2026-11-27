//! Exa web search with highlight extraction in the same request.

use std::time::Duration;

use af_crypto::secret::SecretString;
use af_wire::web::SearchResult;
use futures_util::future::BoxFuture;
use reqwest::{Client, Url};
use serde::Deserialize;
use serde_json::json;

use super::{
    RESULT_COUNT, Vendor, WebFailure,
    client::{self, Object},
};
use crate::Result;

const SEARCH: &str = "https://api.exa.ai/search";

/// Exa's pooled search client. The platform supplies a fresh key per dispatch.
#[derive(Clone, Debug)]
pub struct Exa {
    client: Client,
    search: Url,
}

impl Exa {
    /// Connect through guarded HTTP with automatic retry disabled.
    ///
    /// # Errors
    /// Returns the endpoint or HTTP client construction failure.
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: client::guarded()?,
            search: Url::parse(SEARCH)?,
        })
    }

    /// Inject the fixture's client and endpoint for a real socket test.
    #[cfg(any(test, feature = "test-util"))]
    #[must_use]
    pub const fn with_endpoint(client: Client, search: Url) -> Self {
        Self { client, search }
    }
}

impl Vendor for Exa {
    fn search<'a>(
        &'a self,
        key: &'a SecretString,
        query: &'a str,
        timeout: Duration,
    ) -> BoxFuture<'a, Result<Vec<SearchResult>, WebFailure>> {
        Box::pin(async move {
            let request = json!({
                "query": query,
                "type": "auto",
                "numResults": RESULT_COUNT,
                "contents": {"highlights": true},
            });
            let reply: SearchReply =
                client::post(&self.client, &self.search, key, request, timeout).await?;
            let rows = reply
                .results
                .into_iter()
                .map(|Object(row)| SearchResult {
                    url: row.url,
                    title: row.title,
                    published: row.published_date,
                    excerpts: row.highlights,
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
#[serde(rename_all = "camelCase")]
struct Row {
    url: String,
    title: Option<String>,
    published_date: Option<String>,
    #[serde(default)]
    highlights: Vec<String>,
}
