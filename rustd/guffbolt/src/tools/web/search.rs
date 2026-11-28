//! Search and page extraction through the embedder's policy-aware backend.

use schemars::JsonSchema;
use serde::Deserialize;

use crate::tools::outbound::{OutboundFuture, Response};
use crate::tools::{Context, Handler};
use crate::{CallRef, Source, ToolOutput};

/// One result returned by the search service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    /// The result's web address.
    pub url: String,
    /// Its title, when supplied.
    pub title: Option<String>,
    /// Its publication date, when supplied.
    pub published: Option<String>,
    /// Passages returned for the query.
    pub excerpts: Vec<String>,
}

/// Routes a run's web tools under the embedder's policy.
pub trait SearchBackend: std::fmt::Debug + Send + Sync {
    /// Searches once for this tool call.
    fn search<'a>(
        &'a self,
        call: &'a CallRef,
        query: &'a str,
    ) -> OutboundFuture<'a, Vec<SearchResult>>;

    /// Reads the requested page, with an independently confirmed final origin if known.
    fn extract<'a>(
        &'a self,
        call: &'a CallRef,
        url: &'a str,
    ) -> OutboundFuture<'a, (Response, Option<String>)>;
}

/// The search text supplied by the model.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Query {
    /// What to search the public web for.
    query: String,
}

backed! {
    /// Searches through the run's web backend.
    WebSearch over SearchBackend
}

impl Handler for WebSearch<'_> {
    const NAME: &'static str = "web_search";
    const DESCRIPTION: &'static str = "Search the public web for a query. Returns links, dates and excerpts. Requires public web access under the run's network policy.";
    type Arguments = Query;

    async fn run(&self, arguments: Query, context: Context<'_>) -> ToolOutput {
        let results = match self
            .backend
            .search(context.call_ref, &arguments.query)
            .await
        {
            Ok(results) => results,
            Err(refused) => return refused.into(),
        };
        if results.is_empty() {
            return ToolOutput::succeeded("No results.");
        }
        let mut blocks = Vec::new();
        let mut sources = Vec::new();
        for result in results {
            let mut lines = vec![heading(&result)];
            lines.extend(result.published);
            lines.extend(result.excerpts);
            blocks.push(lines.join("\n\n"));
            sources.push(Source {
                url: result.url,
                title: result.title,
            });
        }
        ToolOutput::succeeded(super::web_fetch::bounded(blocks.join("\n\n"), None))
            .with_sources(sources)
    }
}

/// Escape the label and delimit a destination containing parentheses as `CommonMark`
/// requires. Keep the exact URL in the destination for citation redaction.
fn heading(result: &SearchResult) -> String {
    let visible_url = result.url.split(['?', '#']).next().unwrap_or(&result.url);
    let title = result
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or(visible_url);
    let mut label = String::new();
    for ch in title.chars() {
        if matches!(ch, '\\' | '[' | ']' | '<' | '>' | '`' | '*' | '_') {
            label.push('\\');
        }
        label.push(if ch.is_control() { ' ' } else { ch });
    }
    if result.url.contains(['(', ')']) {
        format!("## [{label}](<{}>)", result.url)
    } else {
        format!("## [{label}]({})", result.url)
    }
}

#[cfg(test)]
mod tests {
    use super::{SearchResult, heading};

    #[test]
    fn a_vendor_title_cannot_replace_its_results_link() {
        let result = SearchResult {
            url: "https://tides.example/Guide_(tides)".into(),
            title: Some("Tides](elsewhere)\n# Evening".into()),
            published: None,
            excerpts: vec![],
        };
        assert_eq!(
            heading(&result),
            r"## [Tides\](elsewhere) # Evening](<https://tides.example/Guide_(tides)>)"
        );
        let blank = SearchResult {
            title: Some(" ".into()),
            ..result
        };
        assert_eq!(
            heading(&blank),
            r"## [https://tides.example/Guide\_(tides)](<https://tides.example/Guide_(tides)>)"
        );
    }
}
