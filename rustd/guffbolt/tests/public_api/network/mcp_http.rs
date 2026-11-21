//! MCP over HTTP: what refuses before a connection.

use super::*;

/// A hook that admits every URL with `headers`, its names looked up through
/// `lookup`.
#[derive(Debug)]
struct Admitting {
    url: Option<String>,
    headers: Vec<(String, String)>,
    lookup: Arc<Fixed>,
}

impl Outbound for Admitting {
    fn send<'a>(&'a self, _tool: &'a str, _request: Request) -> OutboundFuture<'a, Response> {
        Box::pin(std::future::ready(Err(Refused::new(
            code::HOST_NOT_ALLOWED,
            "connections only",
        ))))
    }

    fn mask(&self, text: String) -> Pin<Box<dyn Future<Output = String> + Send + '_>> {
        Box::pin(std::future::ready(text))
    }

    fn secret<'a>(
        &'a self,
        _tool: &'a str,
        name: &'a str,
        _field: &'a str,
    ) -> OutboundFuture<'a, String> {
        Box::pin(std::future::ready(Err(Refused::new(
            code::SECRET_NOT_FOUND,
            format!("no secret {name}"),
        ))))
    }

    fn connect<'a>(&'a self, _tool: &'a str, url: &'a str) -> OutboundFuture<'a, Admitted> {
        let url = self.url.clone().unwrap_or_else(|| url.to_owned());
        Box::pin(std::future::ready(Ok(Admitted {
            url,
            headers: self.headers.clone(),
        })))
    }

    fn lookup(&self) -> Arc<dyn Resolve> {
        Arc::<Fixed>::clone(&self.lookup)
    }
}

/// Why `Server::http` refused, read as the model would, then its cause.
async fn http_refusal(url: &str, outbound: &dyn Outbound) -> String {
    let refused = Server::http("docs", url, outbound)
        .await
        .expect_err("the server is refused");
    std::error::Error::source(&refused).map_or_else(
        || refused.to_string(),
        |cause| format!("{refused}: {cause}"),
    )
}

#[tokio::test]
async fn mcp_over_http_is_refused_before_any_connection_when_its_target_is_unusable() {
    let public = || Arc::new(Fixed::default().answering("mcp.test", "192.0.2.1:443"));
    let off_list =
        LocalOutbound::resolving(Allowlist::Hosts(vec!["docs.test".to_owned()]), public())
            .expect("built");
    let rewritten = Admitting {
        url: Some("::not a url".to_owned()),
        headers: Vec::new(),
        lookup: public(),
    };
    let unanswered = Admitting {
        url: None,
        headers: Vec::new(),
        lookup: Arc::new(Fixed::default()),
    };
    let bad_name = Admitting {
        url: None,
        headers: vec![("bad header".to_owned(), "v".to_owned())],
        lookup: public(),
    };
    let bad_value = Admitting {
        url: None,
        headers: vec![
            ("x-ok".to_owned(), "fine".to_owned()),
            ("x-key".to_owned(), "line\nbreak".to_owned()),
        ],
        lookup: public(),
    };

    let refused = [
        http_refusal("https://mcp.test/mcp", &off_list).await,
        http_refusal("https://mcp.test/mcp", &rewritten).await,
        http_refusal("https://mcp.test/mcp", &unanswered).await,
        http_refusal("https://mcp.test/mcp", &bad_name).await,
        http_refusal("https://mcp.test/mcp", &bad_value).await,
    ];

    assert_eq!(
        refused,
        [
            "[host_not_allowed] mcp.test is not an allowed host",
            "the admitted URL is not one: relative URL without a base",
            "[upstream_unreachable] mcp.test could not be looked up: no answer for mcp.test",
            "an admitted header's name is not one: invalid HTTP header name",
            "an admitted header's value is not one: failed to parse header value",
        ]
    );
    assert_eq!(unanswered.lookup.asked(), ["mcp.test"]);
}

/// An HTTP client that would not build, and a server that did not answer in
/// time, each read in the harness's words and keep their cause.
#[tokio::test]
async fn an_mcp_client_or_timeout_failure_keeps_its_cause() {
    use guffbolt::tools::mcp::McpError;

    let lookup: Arc<dyn Resolve> = Arc::new(Fixed::default());
    let unbuilt = guffbolt::tools::outbound::client(std::time::Duration::from_secs(1), lookup)
        .build()
        .expect("the harness's client builds")
        .get("not a url")
        .build()
        .expect_err("a request with no URL does not build");
    let late = tokio::time::timeout(std::time::Duration::ZERO, std::future::pending::<()>())
        .await
        .expect_err("a pending call never answers");

    let client = McpError::from(unbuilt);
    let timeout = McpError::Timeout(late);

    assert_eq!(client.to_string(), "the HTTP client would not build");
    assert!(
        client.source().is_some(),
        "the client's failure is its cause"
    );
    assert!(
        timeout
            .to_string()
            .starts_with("the server did not answer within"),
        "{timeout}"
    );
    assert!(timeout.source().is_some(), "the elapsed timer is its cause");
}
