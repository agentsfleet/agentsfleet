//! The search function is built over the embedder's run-scoped service.
use super::*;
use guffbolt::tools::web::{SearchBackend, SearchResult};

#[derive(Debug)]
struct Pages;

impl SearchBackend for Pages {
    fn search<'a>(
        &'a self,
        call: &'a CallRef,
        query: &'a str,
    ) -> OutboundFuture<'a, Vec<SearchResult>> {
        assert_eq!(call.call_id, "call-1");
        assert_eq!(query, "tides");
        Box::pin(std::future::ready(Ok(vec![
            SearchResult {
                url: "https://test.example/a".into(),
                title: Some("Tides".into()),
                published: Some("2026-01-01".into()),
                excerpts: vec!["High at noon".into()],
            },
            SearchResult {
                url: "https://test.example/b".into(),
                title: None,
                published: None,
                excerpts: vec!["Low at six".into()],
            },
        ])))
    }

    fn extract<'a>(
        &'a self,
        _: &'a CallRef,
        _: &'a str,
    ) -> OutboundFuture<'a, (Response, Option<String>)> {
        Box::pin(std::future::ready(Err(Refused::new(
            code::UPSTREAM_UNREACHABLE,
            "no pages",
        ))))
    }
}

#[tokio::test]
async fn a_web_search_answers_its_results_as_markdown() {
    let tools = Tools::builder()
        .with_search(&Pages)
        .build(&ToolsConfig::new(["web_search"]).expect("name"))
        .expect("search backend");
    let result = dispatch(&tools, "web_search", json!({"query":"tides"})).await;
    assert_eq!(result.error_code, None);
    assert_eq!(
        result.text,
        "## [Tides](https://test.example/a)\n\n2026-01-01\n\nHigh at noon\n\n## [https://test.example/b](https://test.example/b)\n\nLow at six"
    );
    assert_eq!(result.sources().len(), 2);
}
