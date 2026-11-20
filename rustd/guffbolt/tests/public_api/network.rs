//! The tools that reach past the machine, driven through their public
//! seams: `http_request` and `web_fetch` over an `Outbound` hook, the
//! standalone `LocalOutbound` and its refusals, an MCP server over the run's
//! `Env`, and the browser tools over an environment's browser.
//!
//! Nothing here reaches the network. A fake hook stands at the `Outbound`
//! boundary, a fake lookup answers every name, and a fake environment plays
//! an MCP server over the pipes a process would have.

use std::collections::HashMap;
use std::error::Error as StdError;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use bytes::Bytes;
use guffbolt::tools::env::{
    Browser, Ending, Env, EnvError, EnvErrorKind, EnvFuture, Feed, FileContent, Listing, Process,
    ProcessId, Spawn, Stream, pipe,
};
use guffbolt::tools::mcp::{
    ListMcpResourceTemplates, ListMcpResources, ReadMcpResource, Server, tool_name,
};
use guffbolt::tools::outbound::{
    Admitted, Allowlist, LocalOutbound, Outbound, OutboundFuture, Placement, Redirects, Refused,
    Request, Response,
};
use guffbolt::tools::{Context, ContextTool, ToolsConfig, Typed, code};
use guffbolt::{CallRef, Image, ImageKind, ToolOutput, Tools};
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use serde_json::{Value, json};

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Runs `name` from `tools` with `arguments`, as call 0 of step 1.
async fn dispatch(tools: &Tools<ContextTool<'_>>, name: &str, arguments: Value) -> ToolOutput {
    let call_ref = CallRef {
        step: 1,
        index: 0,
        call_id: "call-1".to_owned(),
    };
    let context = Context {
        call_ref: &call_ref,
        run_id: "run-1",
        wake: None,
    };
    tools.dispatch(name, &arguments, context).await
}

/// The tools `names` builds over `outbound`.
fn web<'run>(outbound: &'run dyn Outbound, names: &[&str]) -> Tools<ContextTool<'run>> {
    let config = ToolsConfig::new(names).expect("names");
    Tools::builder()
        .with_outbound(outbound)
        .build(&config)
        .expect("an outbound is given")
}

// ---------------------------------------------------------------------------
// A hook that answers every request with one canned answer.
// ---------------------------------------------------------------------------

/// What a canned hook says the secret it placed is.
const PLACED: &str = "tok-123";

/// A hook that answers each request with `answer`, masks [`PLACED`], and
/// records every request and every mask it was asked for. It keeps the
/// trait's own lookup.
#[derive(Debug)]
struct Canned {
    answer: Result<Response, Refused>,
    sent: Mutex<Vec<(String, Request)>>,
    masked: AtomicU64,
}

impl Canned {
    fn answering(answer: Result<Response, Refused>) -> Self {
        Self {
            answer,
            sent: Mutex::new(Vec::new()),
            masked: AtomicU64::new(0),
        }
    }

    fn ok(response: Response) -> Self {
        Self::answering(Ok(response))
    }

    fn sent(&self) -> Vec<(String, Request)> {
        lock(&self.sent).clone()
    }
}

impl Outbound for Canned {
    fn send<'a>(&'a self, tool: &'a str, request: Request) -> OutboundFuture<'a, Response> {
        lock(&self.sent).push((tool.to_owned(), request));
        Box::pin(std::future::ready(self.answer.clone()))
    }

    fn mask(&self, text: String) -> Pin<Box<dyn Future<Output = String> + Send + '_>> {
        self.masked.fetch_add(1, Ordering::SeqCst);
        Box::pin(std::future::ready(text.replace(PLACED, "[masked]")))
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
        Box::pin(std::future::ready(Ok(Admitted {
            url: url.to_owned(),
            headers: Vec::new(),
        })))
    }
}

fn response(status: u16, body: &str) -> Response {
    Response {
        status,
        body: body.to_owned(),
        ..Response::default()
    }
}

// ---------------------------------------------------------------------------
// http_request
// ---------------------------------------------------------------------------

/// A bodyless GET follows its redirects; the answer names the status, the
/// media type and the hops, then the body.
#[tokio::test]
async fn http_request_get_follows_redirects_and_reads_status_type_and_body() {
    let hook = Canned::ok(Response {
        content_type: Some("application/json".to_owned()),
        followed_redirects: 2,
        ..response(200, r#"{"ok":true}"#)
    });
    let tools = web(&hook, &["http_request"]);

    let output = dispatch(
        &tools,
        "http_request",
        json!({"url": "https://api.test/v1", "headers": {"X-B": "2", "Authorization": "Bearer ${secrets.api.token}"}}),
    )
    .await;

    assert_eq!(output.error_code, None);
    assert_eq!(
        output.text,
        "Status: 200\nContent-Type: application/json\nRedirects followed: 2\n\nResponse Body:\n{\"ok\":true}"
    );
    assert_eq!(
        hook.sent(),
        [(
            "http_request".to_owned(),
            Request {
                method: "GET".to_owned(),
                url: "https://api.test/v1".to_owned(),
                headers: vec![
                    (
                        "Authorization".to_owned(),
                        "Bearer ${secrets.api.token}".to_owned()
                    ),
                    ("X-B".to_owned(), "2".to_owned()),
                ],
                body: None,
                placement: Placement::Authorization,
                redirects: Redirects::Follow,
            }
        )],
        "the placeholder reaches the hook unplaced, in the Authorization header"
    );
}

/// A request with a body, or another method, stops at a redirect and reads
/// where it points; a status outside 2xx fails as `upstream_status`.
#[tokio::test]
async fn http_request_with_a_body_stops_at_a_redirect_and_fails_as_upstream_status() {
    let hook = Canned::ok(Response {
        location: Some("https://api.test/elsewhere".to_owned()),
        ..response(302, "")
    });
    let tools = web(&hook, &["http_request"]);

    let posted = dispatch(
        &tools,
        "http_request",
        json!({"url": "https://api.test/", "method": "POST", "body": "{}"}),
    )
    .await;
    let got_with_body = dispatch(
        &tools,
        "http_request",
        json!({"url": "https://api.test/", "body": "x"}),
    )
    .await;
    let lower_get = dispatch(
        &tools,
        "http_request",
        json!({"url": "https://api.test/", "method": "get"}),
    )
    .await;

    assert_eq!(posted.error_code, Some(code::UPSTREAM_STATUS));
    assert_eq!(
        posted.text,
        "[upstream_status] Status: 302\nLocation: https://api.test/elsewhere\n\nResponse Body:\n"
    );
    assert_eq!(got_with_body.error_code, Some(code::UPSTREAM_STATUS));
    assert_eq!(lower_get.error_code, Some(code::UPSTREAM_STATUS));
    let followed: Vec<(String, Redirects)> = hook
        .sent()
        .into_iter()
        .map(|(_tool, request)| (request.method, request.redirects))
        .collect();
    assert_eq!(
        followed,
        [
            ("POST".to_owned(), Redirects::Stop),
            ("GET".to_owned(), Redirects::Stop),
            ("get".to_owned(), Redirects::Follow),
        ]
    );
}

/// A cut body is never handed to the model as if it were whole: the answer
/// says where it was cut and omits the prefix; replaced bytes are named.
#[tokio::test]
async fn http_request_omits_a_truncated_body_and_names_replaced_bytes() {
    let cut = Canned::ok(Response {
        truncated: true,
        lossy: true,
        ..response(200, "half of it")
    });
    let empty_cut = Canned::ok(Response {
        truncated: true,
        ..response(200, "")
    });
    let cut_tools = web(&cut, &["http_request"]);
    let empty_tools = web(&empty_cut, &["http_request"]);

    let read = dispatch(
        &cut_tools,
        "http_request",
        json!({"url": "https://api.test/"}),
    )
    .await;
    let read_empty = dispatch(
        &empty_tools,
        "http_request",
        json!({"url": "https://api.test/"}),
    )
    .await;

    assert_eq!(
        read.text,
        "Status: 200\n[Response truncated at 1048576 bytes]\n[Response contains replaced non-UTF-8 bytes]\n\n\
         Response Body:\n[Incomplete response body omitted; the source was truncated]"
    );
    assert_eq!(
        read_empty.text,
        "Status: 200\n[Response truncated at 1048576 bytes]\n\nResponse Body:\n"
    );
}

#[tokio::test]
async fn http_request_reads_the_hooks_refusal_as_its_code_and_sentence() {
    let hook = Canned::answering(Err(Refused::new(
        code::HOST_NOT_ALLOWED,
        "api.test is not an allowed host",
    )));
    let tools = web(&hook, &["http_request"]);

    let output = dispatch(&tools, "http_request", json!({"url": "https://api.test/"})).await;

    assert_eq!(output.error_code, Some(code::HOST_NOT_ALLOWED));
    assert_eq!(
        output.text,
        "[host_not_allowed] api.test is not an allowed host"
    );
}

#[tokio::test]
async fn http_request_with_arguments_outside_its_schema_sends_nothing() {
    let hook = Canned::ok(response(200, ""));
    let tools = web(&hook, &["http_request"]);

    let unknown = dispatch(
        &tools,
        "http_request",
        json!({"url": "https://api.test/", "verb": "GET"}),
    )
    .await;
    let missing = dispatch(&tools, "http_request", json!({"method": "GET"})).await;
    let mistyped = dispatch(
        &tools,
        "http_request",
        json!({"url": "https://api.test/", "headers": ["a"]}),
    )
    .await;

    for output in [&unknown, &missing, &mistyped] {
        assert_eq!(
            output.error_code,
            Some(code::INVALID_ARGUMENTS),
            "{}",
            output.text
        );
    }
    assert_eq!(missing.text, "[invalid_arguments] missing field `url`");
    assert!(
        unknown
            .text
            .starts_with("[invalid_arguments] unknown field `verb`"),
        "{}",
        unknown.text
    );
    assert_eq!(hook.sent(), Vec::new(), "no request reached the hook");
}

// ---------------------------------------------------------------------------
// web_fetch
// ---------------------------------------------------------------------------

/// A page is read as its text, and masked again once decoded: an
/// entity-encoded secret only reads as the secret after the markup is gone.
#[tokio::test]
async fn web_fetch_reads_a_page_as_text_and_masks_what_decoding_reveals() {
    let hook = Canned::ok(Response {
        content_type: Some("text/html; charset=utf-8".to_owned()),
        ..response(
            200,
            "<html><body><h1>Docs</h1><p>key tok&#45;123 here</p></body></html>",
        )
    });
    let tools = web(&hook, &["web_fetch"]);

    let output = dispatch(
        &tools,
        "web_fetch",
        json!({"url": "https://docs.test/page"}),
    )
    .await;

    assert_eq!(output.error_code, None);
    assert!(output.text.contains("Docs"), "{}", output.text);
    assert!(output.text.contains("key [masked] here"), "{}", output.text);
    assert!(
        !output.text.contains("tok-123") && !output.text.contains("<p>"),
        "{}",
        output.text
    );
    assert_eq!(hook.masked.load(Ordering::SeqCst), 1);
    assert_eq!(
        hook.sent(),
        [(
            "web_fetch".to_owned(),
            Request {
                method: "GET".to_owned(),
                url: "https://docs.test/page".to_owned(),
                headers: Vec::new(),
                body: None,
                placement: Placement::Nowhere,
                redirects: Redirects::Stop,
            }
        )],
        "a fetch carries no header, no body and no placeholder, and follows nothing"
    );
}

/// A body that is not a page is read as it came, unmasked again, and cut at
/// `max_chars` characters, never inside one.
#[tokio::test]
async fn web_fetch_cuts_a_plain_body_at_max_chars_by_character() {
    let hook = Canned::ok(Response {
        content_type: Some("text/plain".to_owned()),
        ..response(200, "h\u{e9}llo w\u{f6}rld")
    });
    let tools = web(&hook, &["web_fetch"]);

    let cut = dispatch(
        &tools,
        "web_fetch",
        json!({"url": "https://docs.test/", "max_chars": 4}),
    )
    .await;
    let whole = dispatch(
        &tools,
        "web_fetch",
        json!({"url": "https://docs.test/", "max_chars": 11}),
    )
    .await;
    let none = dispatch(
        &tools,
        "web_fetch",
        json!({"url": "https://docs.test/", "max_chars": 0}),
    )
    .await;

    assert_eq!(
        cut.text,
        "h\u{e9}ll\n\n[Content truncated at 4 chars, total 11 chars]"
    );
    assert_eq!(whole.text, "h\u{e9}llo w\u{f6}rld");
    assert_eq!(
        none.text,
        "\n\n[Content truncated at 0 chars, total 11 chars]"
    );
    assert_eq!(
        hook.masked.load(Ordering::SeqCst),
        0,
        "plain text is not masked again"
    );
}

#[tokio::test]
async fn web_fetch_reads_at_most_fifty_thousand_chars_when_no_limit_is_named() {
    let long = "a".repeat(50_001);
    let hook = Canned::ok(response(200, &long));
    let tools = web(&hook, &["web_fetch"]);

    let output = dispatch(&tools, "web_fetch", json!({"url": "https://docs.test/"})).await;

    assert!(
        output
            .text
            .ends_with("\n\n[Content truncated at 50000 chars, total 50001 chars]"),
        "{}",
        output
            .text
            .get(output.text.len().saturating_sub(80)..)
            .unwrap_or_default()
    );
    assert_eq!(
        output.text.split("\n\n").next().map(str::len),
        Some(50_000),
        "the kept text is the first fifty thousand characters"
    );
}

#[tokio::test]
async fn web_fetch_fails_on_a_status_outside_2xx_and_on_the_hooks_refusal() {
    let missing = Canned::ok(response(404, "not here"));
    let refused = Canned::answering(Err(Refused::new(
        code::HTTPS_REQUIRED,
        "http://docs.test/ is not https",
    )));
    let missing_tools = web(&missing, &["web_fetch"]);
    let refused_tools = web(&refused, &["web_fetch"]);

    let not_found = dispatch(
        &missing_tools,
        "web_fetch",
        json!({"url": "https://docs.test/"}),
    )
    .await;
    let not_https = dispatch(
        &refused_tools,
        "web_fetch",
        json!({"url": "http://docs.test/"}),
    )
    .await;
    let bad = dispatch(
        &missing_tools,
        "web_fetch",
        json!({"url": "https://docs.test/", "max_chars": -1}),
    )
    .await;

    assert_eq!(
        (not_found.error_code, not_found.text.as_str()),
        (Some(code::UPSTREAM_STATUS), "[upstream_status] not here")
    );
    assert_eq!(
        not_https.text,
        "[https_required] http://docs.test/ is not https"
    );
    assert_eq!(bad.error_code, Some(code::INVALID_ARGUMENTS));
}

/// A hook that keeps the trait's default lookup looks names up as this host
/// does: `localhost` resolves without leaving the machine.
#[tokio::test]
async fn an_outbound_hook_looks_names_up_through_this_hosts_resolver_by_default() {
    let hook = Canned::ok(response(200, ""));
    let name: Name = "localhost".parse().expect("a name");

    let addresses: Vec<SocketAddr> = hook
        .lookup()
        .resolve(name)
        .await
        .expect("localhost resolves")
        .collect();

    assert_ne!(addresses, Vec::<SocketAddr>::new());
    assert!(
        addresses.iter().all(|address| address.ip().is_loopback()),
        "{addresses:?}"
    );
}

// ---------------------------------------------------------------------------
// LocalOutbound
// ---------------------------------------------------------------------------

/// A lookup with fixed answers, refusing any other name, recording each name
/// it was asked.
#[derive(Debug, Default)]
struct Fixed {
    answers: HashMap<String, Vec<SocketAddr>>,
    asked: Mutex<Vec<String>>,
}

impl Fixed {
    fn answering(mut self, name: &str, address: &str) -> Self {
        let address: SocketAddr = address.parse().expect("an address");
        self.answers.insert(name.to_owned(), vec![address]);
        self
    }

    fn asked(&self) -> Vec<String> {
        lock(&self.asked).clone()
    }
}

impl Resolve for Fixed {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        lock(&self.asked).push(host.clone());
        let answer: Result<Addrs, Box<dyn StdError + Send + Sync>> =
            self.answers.get(&host).map_or_else(
                || Err(format!("no answer for {host}").into()),
                |addresses| Ok(Box::new(addresses.clone().into_iter()) as Addrs),
            );
        Box::pin(std::future::ready(answer))
    }
}

#[test]
fn an_allowlist_allows_a_listed_host_and_its_subdomains_in_any_case() {
    let hosts = Allowlist::Hosts(vec!["Example.com".to_owned()]);

    let judged: Vec<bool> = [
        "example.com",
        "API.example.COM",
        "a.b.example.com",
        "badexample.com",
        "example.com.evil",
        "",
    ]
    .iter()
    .map(|host| hosts.allows(host))
    .collect();

    assert_eq!(judged, [true, true, true, false, false, false]);
    assert!(Allowlist::Any.allows("anything.test"));
    assert!(
        !Allowlist::Hosts(Vec::new()).allows("example.com"),
        "an empty list allows nothing"
    );
}

/// Every request `LocalOutbound` will not send is refused before a lookup or
/// a connection: a method it does not send, a URL that is no URL, plain
/// HTTP, a private literal, and a host off the allowlist.
#[tokio::test]
async fn local_outbound_refuses_a_request_before_it_leaves() {
    let lookup = Arc::new(Fixed::default());
    let outbound = LocalOutbound::resolving(
        Allowlist::Hosts(vec!["docs.test".to_owned()]),
        Arc::<Fixed>::clone(&lookup),
    )
    .expect("the client builds");
    let tools = web(&outbound, &["http_request"]);

    let mut read = Vec::new();
    for arguments in [
        json!({"url": "https://docs.test/", "method": "TRACE"}),
        json!({"url": "not a url"}),
        json!({"url": "http://docs.test/"}),
        json!({"url": "https://127.0.0.1/"}),
        json!({"url": "https://[::ffff:10.0.0.1]/"}),
        json!({"url": "https://elsewhere.test/"}),
    ] {
        read.push(dispatch(&tools, "http_request", arguments).await.text);
    }

    assert_eq!(
        read,
        [
            "[method_not_allowed] TRACE is not a method this tool sends",
            "[invalid_arguments] not a url is not a URL: relative URL without a base",
            "[https_required] http://docs.test/ is not https",
            "[egress_denied] 127.0.0.1 is a private, loopback or reserved address",
            "[egress_denied] [::ffff:a00:1] is a private, loopback or reserved address",
            "[host_not_allowed] elsewhere.test is not an allowed host",
        ]
    );
    assert_eq!(
        lookup.asked(),
        Vec::<String>::new(),
        "no name was looked up"
    );
}

/// A name that resolves to a private address is refused whole by the
/// client's guard, after the credential was placed and before any
/// connection; a name no lookup answers reads as unreachable, with no URL.
#[tokio::test]
async fn local_outbound_refuses_a_name_that_resolves_private_or_not_at_all() {
    let lookup = Arc::new(Fixed::default().answering("inside.test", "10.0.0.7:443"));
    let outbound = LocalOutbound::resolving(Allowlist::Any, Arc::<Fixed>::clone(&lookup))
        .expect("the client builds")
        .with_secret("api", "token", "s3cret");
    let tools = web(&outbound, &["http_request"]);

    let inside = dispatch(
        &tools,
        "http_request",
        json!({"url": "https://inside.test/v1", "method": "POST", "body": "{}",
               "headers": {"Authorization": "Bearer ${secrets.api.token}", "X-Trace": "1"}}),
    )
    .await;
    let nowhere = dispatch(
        &tools,
        "http_request",
        json!({"url": "https://nowhere.test/secret-path"}),
    )
    .await;

    assert_eq!(
        inside.text,
        "[egress_denied] inside.test resolves to a private, loopback or reserved address"
    );
    assert_eq!(nowhere.error_code, Some(code::UPSTREAM_UNREACHABLE));
    assert!(
        !nowhere.text.contains("secret-path"),
        "the refusal carries no URL: {}",
        nowhere.text
    );
    assert_eq!(lookup.asked(), ["inside.test", "nowhere.test"]);
}

#[tokio::test]
async fn local_outbound_masks_places_and_admits_as_its_secrets_and_allowlist_say() {
    let lookup = Arc::new(Fixed::default().answering("docs.test", "192.0.2.10:443"));
    let outbound = LocalOutbound::resolving(
        Allowlist::Hosts(vec!["docs.test".to_owned()]),
        Arc::<Fixed>::clone(&lookup),
    )
    .expect("the client builds")
    .with_secret("api", "token", "s3cret")
    .with_secret("api", "user", "ada")
    .with_secret("blank", "value", "");

    let masked = outbound
        .mask("ada sent s3cret, and s3cret again".to_owned())
        .await;
    let token = outbound.secret("http_request", "api", "token").await;
    let no_field = outbound.secret("http_request", "api", "password").await;
    let no_secret = outbound.secret("http_request", "db", "token").await;
    let admitted = outbound.connect("mcp", "https://docs.test/mcp").await;
    let plain = outbound.connect("mcp", "http://docs.test/mcp").await;
    let resolved: Vec<SocketAddr> = outbound
        .lookup()
        .resolve("docs.test".parse().expect("a name"))
        .await
        .expect("the fixed lookup answers")
        .collect();

    assert_eq!(
        masked, "[masked] sent [masked], and [masked] again",
        "a blank secret masks nothing"
    );
    assert_eq!(token, Ok("s3cret".to_owned()));
    assert_eq!(
        no_field,
        Err(Refused::new(
            code::SECRET_NOT_FOUND,
            "no secret api with a field password"
        ))
    );
    assert_eq!(
        no_secret,
        Err(Refused::new(
            code::SECRET_NOT_FOUND,
            "no secret db with a field token"
        ))
    );
    assert_eq!(
        admitted,
        Ok(Admitted {
            url: "https://docs.test/mcp".to_owned(),
            headers: Vec::new()
        })
    );
    assert_eq!(
        plain,
        Err(Refused::new(
            code::HTTPS_REQUIRED,
            "http://docs.test/mcp is not https"
        ))
    );
    assert_eq!(
        resolved,
        ["192.0.2.10:443".parse::<SocketAddr>().expect("an address")]
    );
    assert_eq!(
        lookup.asked(),
        ["docs.test"],
        "its lookup is the one it was built with"
    );
    assert_eq!(
        format!("{outbound:?}"),
        r#"LocalOutbound { allowlist: Hosts(["docs.test"]), .. }"#,
        "its Debug never prints a secret"
    );
}

#[test]
fn local_outbound_over_this_hosts_resolver_builds() {
    let outbound = LocalOutbound::new(Allowlist::Any).expect("the TLS backend takes the client");

    assert_eq!(
        format!("{outbound:?}"),
        "LocalOutbound { allowlist: Any, .. }"
    );
}

/// A refused request reads as its code and sentence wherever it lands.
#[test]
fn a_refusal_reads_as_a_failed_call() {
    let output = ToolOutput::from(Refused::new(code::EGRESS_DENIED, "10.0.0.1 is private"));

    assert_eq!(output.error_code, Some(code::EGRESS_DENIED));
    assert_eq!(output.text, "[egress_denied] 10.0.0.1 is private");
}

// ---------------------------------------------------------------------------
// MCP over HTTP: what refuses before a connection.
// ---------------------------------------------------------------------------

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

/// Why `Server::http` refused, read as the model would.
async fn http_refusal(url: &str, outbound: &dyn Outbound) -> String {
    Server::http("docs", url, outbound)
        .await
        .expect_err("the server is refused")
        .to_string()
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
            "relative URL without a base",
            "[upstream_unreachable] mcp.test could not be looked up: no answer for mcp.test",
            "invalid HTTP header name",
            "failed to parse header value",
        ]
    );
    assert_eq!(unanswered.lookup.asked(), ["mcp.test"]);
}

// ---------------------------------------------------------------------------
// MCP over stdio: a fake environment plays the server on a process's pipes.
// ---------------------------------------------------------------------------

/// How the fake server behaves.
#[derive(Debug, Default, Clone)]
struct Behaviour {
    /// Answers `initialize` with an error.
    refuse_initialize: bool,
    /// Methods it never answers.
    silent: Vec<&'static str>,
    /// Methods it answers with a JSON-RPC error.
    failing: Vec<&'static str>,
    /// Methods it exits on, unanswered.
    exits_on: Vec<&'static str>,
}

/// An environment whose every process is a fake MCP server, and whose
/// browser, when it has one, is [`FakeBrowser`].
#[derive(Debug, Default)]
struct FakeEnv {
    behaviour: Behaviour,
    browser: Option<FakeBrowser>,
    refuse_spawn: bool,
    feeds: Mutex<HashMap<u64, Feed>>,
    next: AtomicU64,
    spawned: Mutex<Vec<Vec<String>>>,
    killed: Mutex<Vec<u64>>,
    killed_note: tokio::sync::Notify,
}

impl FakeEnv {
    fn serving(behaviour: Behaviour) -> Self {
        Self {
            behaviour,
            ..Self::default()
        }
    }

    /// What the server answers to `method`: its result, or its error.
    fn answer(&self, method: &str, params: &Value) -> Option<Result<Value, Value>> {
        if self.behaviour.silent.contains(&method) {
            return None;
        }
        if self.behaviour.failing.contains(&method) {
            return Some(Err(
                json!({"code": -32_603, "message": format!("{method} broke")}),
            ));
        }
        let result = match method {
            "initialize" if self.behaviour.refuse_initialize => {
                return Some(Err(
                    json!({"code": -32_600, "message": "initialize refused"}),
                ));
            }
            "initialize" => json!({
                "protocolVersion": params.get("protocolVersion").cloned().unwrap_or(Value::Null),
                "capabilities": {"tools": {}, "resources": {}},
                "serverInfo": {"name": "fake", "version": "1.0.0"}
            }),
            "tools/list" => json!({"tools": [
                {"name": "search.v2", "description": "Searches the docs.",
                 "inputSchema": {"type": "object", "properties": {"q": {"type": "string"}}}},
                {"name": "stats", "inputSchema": {"type": "object"}},
                {"name": "explode", "inputSchema": {"type": "object"}}
            ]}),
            "tools/call" => return Some(Ok(call_answer(params))),
            "resources/list" => json!({"resources": [
                {"uri": "file:///guide.md", "name": "guide"}
            ]}),
            "resources/templates/list" => json!({"resourceTemplates": [
                {"uriTemplate": "file:///{path}", "name": "files"}
            ]}),
            "resources/read" => json!({"contents": [
                {"uri": "file:///guide.md", "text": "# Guide"},
                {"uri": "file:///logo.png", "mimeType": "image/png", "blob": "AAAA"},
                {"uri": "file:///raw", "blob": "AAAA"}
            ]}),
            other => {
                return Some(Err(
                    json!({"code": -32_601, "message": format!("{other} is unknown")}),
                ));
            }
        };
        Some(Ok(result))
    }
}

/// What the fake server's tools answer, by the tool called.
fn call_answer(params: &Value) -> Value {
    match params.get("name").and_then(Value::as_str) {
        Some("search.v2") => json!({"content": [
            {"type": "text", "text": format!("found {}", params.get("arguments").and_then(|a| a.get("q")).and_then(Value::as_str).unwrap_or("nothing"))},
            {"type": "image", "data": "AAAA", "mimeType": "image/png"},
            {"type": "resource", "resource": {"uri": "file:///a", "text": "embedded"}},
            {"type": "resource", "resource": {"uri": "file:///b", "blob": "AAAA"}},
            {"type": "resource_link", "uri": "file:///c", "name": "c"}
        ]}),
        Some("stats") => json!({"content": [], "structuredContent": {"pages": 3}}),
        _ => json!({"content": [{"type": "text", "text": "it blew up"}], "isError": true}),
    }
}

impl Env for FakeEnv {
    fn root(&self) -> &'static str {
        "/fake"
    }

    fn spawn<'a>(&'a self, spawn: &'a Spawn) -> EnvFuture<'a, Process> {
        lock(&self.spawned).push(spawn.argv().to_vec());
        if self.refuse_spawn {
            return Box::pin(std::future::ready(Err(EnvError::new(
                EnvErrorKind::Unavailable,
                "no such program",
            ))));
        }
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (feed, piped) = pipe();
        // A server's own log line, on both streams, comes before any message.
        feed.output(Stream::Stderr, Bytes::from_static(b"starting up\n"));
        feed.output(Stream::Stdout, Bytes::from_static(b"fake server ready\n"));
        lock(&self.feeds).insert(id, feed);
        Box::pin(std::future::ready(Ok(Process {
            id: ProcessId::new(id),
            events: Box::new(piped),
        })))
    }

    fn write(&self, process: ProcessId, data: Bytes) -> EnvFuture<'_, ()> {
        let message: Value = serde_json::from_slice(&data).expect("the client writes JSON lines");
        let id = message.get("id").cloned();
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        if self.behaviour.exits_on.contains(&method.as_str()) {
            let feed = lock(&self.feeds).remove(&process.get());
            if let Some(feed) = feed {
                feed.end(Ending::Exited(1), false);
            }
            return Box::pin(std::future::ready(Ok(())));
        }
        if let (Some(id), Some(answer)) = (id, self.answer(&method, &params)) {
            let reply = match answer {
                Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
                Err(error) => json!({"jsonrpc": "2.0", "id": id, "error": error}),
            };
            let mut line = serde_json::to_vec(&reply).expect("a reply encodes");
            line.push(b'\n');
            let feeds = lock(&self.feeds);
            if let Some(feed) = feeds.get(&process.get()) {
                // Split in two chunks, so a message arrives across reads.
                let (head, tail) = line.split_at(line.len() / 2);
                feed.output(Stream::Stdout, Bytes::copy_from_slice(head));
                feed.output(Stream::Stdout, Bytes::copy_from_slice(tail));
            }
        }
        Box::pin(std::future::ready(Ok(())))
    }

    fn kill(&self, process: ProcessId) -> EnvFuture<'_, ()> {
        let feed = lock(&self.feeds).remove(&process.get());
        if let Some(feed) = feed {
            feed.end(Ending::Signaled(9), false);
        }
        lock(&self.killed).push(process.get());
        self.killed_note.notify_one();
        Box::pin(std::future::ready(Ok(())))
    }

    fn read_file<'a>(&'a self, _path: &'a str, _max_bytes: u64) -> EnvFuture<'a, FileContent> {
        Box::pin(std::future::ready(Err(EnvError::new(
            EnvErrorKind::Unavailable,
            "no files",
        ))))
    }

    fn write_file<'a>(&'a self, _path: &'a str, _data: Bytes) -> EnvFuture<'a, ()> {
        Box::pin(std::future::ready(Err(EnvError::new(
            EnvErrorKind::Unavailable,
            "no files",
        ))))
    }

    fn append_file<'a>(&'a self, _path: &'a str, _data: Bytes) -> EnvFuture<'a, ()> {
        Box::pin(std::future::ready(Err(EnvError::new(
            EnvErrorKind::Unavailable,
            "no files",
        ))))
    }

    fn delete_file<'a>(&'a self, _path: &'a str) -> EnvFuture<'a, ()> {
        Box::pin(std::future::ready(Err(EnvError::new(
            EnvErrorKind::Unavailable,
            "no files",
        ))))
    }

    fn list_dir<'a>(&'a self, _path: &'a str) -> EnvFuture<'a, Listing> {
        Box::pin(std::future::ready(Err(EnvError::new(
            EnvErrorKind::Unavailable,
            "no files",
        ))))
    }

    fn browser(&self) -> Result<&dyn Browser, &str> {
        self.browser
            .as_ref()
            .map(|browser| browser as &dyn Browser)
            .ok_or("has no browser here")
    }
}

/// Starts the fake server `env` plays, named `docs`.
async fn started(env: &Arc<FakeEnv>) -> Arc<Server> {
    let spawn = Spawn::program("docs-mcp").arg("--stdio");
    let server = Server::stdio("docs", Arc::<FakeEnv>::clone(env), &spawn)
        .await
        .expect("the server initializes");
    Arc::new(server)
}

/// The server's tools and the three resource tools, as one run holds them.
fn mcp_tools<'run>(
    server: &Arc<Server>,
    offered: Vec<Box<ContextTool<'run>>>,
) -> Tools<ContextTool<'run>> {
    let servers: Vec<Arc<Server>> = vec![Arc::clone(server)];
    let mut tools = Tools::new(offered);
    tools.offer(Box::new(Typed::new(ListMcpResources::new(servers.clone()))));
    tools.offer(Box::new(Typed::new(ListMcpResourceTemplates::new(
        servers.clone(),
    ))));
    tools.offer(Box::new(Typed::new(ReadMcpResource::new(servers))));
    tools
}

/// A stdio server's tools are offered under `mcp__<server>__<tool>`, and a
/// call reads each part of the answer as the model can read it.
#[tokio::test]
async fn an_mcp_server_over_stdio_offers_its_tools_and_answers_their_calls() {
    let env = Arc::new(FakeEnv::serving(Behaviour::default()));
    let server = started(&env).await;

    let listed = server.tools().await.expect("the server lists its tools");
    let remotes: Vec<String> = listed.iter().map(|tool| tool.remote().to_owned()).collect();
    let offered: Vec<Box<ContextTool<'_>>> = listed
        .into_iter()
        .map(|tool| Box::new(tool) as Box<ContextTool<'_>>)
        .collect();
    let tools = mcp_tools(&server, offered);
    let searched = dispatch(&tools, "mcp__docs__search_v2", json!({"q": "install"})).await;
    let stats = dispatch(&tools, "mcp__docs__stats", Value::Null).await;
    let exploded = dispatch(&tools, "mcp__docs__explode", json!({})).await;
    let not_object = dispatch(&tools, "mcp__docs__stats", json!([1, 2])).await;

    assert_eq!(server.name(), "docs");
    assert_eq!(format!("{server:?}"), r#"Server { name: "docs", .. }"#);
    assert_eq!(remotes, ["search.v2", "stats", "explode"]);
    assert_eq!(
        tools
            .specs()
            .map(|spec| (spec.name.to_owned(), spec.description.to_owned()))
            .collect::<Vec<_>>()
            .first(),
        Some(&(
            "mcp__docs__search_v2".to_owned(),
            "Searches the docs.".to_owned()
        ))
    );
    assert_eq!(
        tools.names().collect::<Vec<_>>(),
        [
            "mcp__docs__search_v2",
            "mcp__docs__stats",
            "mcp__docs__explode",
            "list_mcp_resources",
            "list_mcp_resource_templates",
            "read_mcp_resource",
        ]
    );
    assert_eq!(
        (searched.error_code, searched.text.as_str()),
        (
            None,
            "found install\n[image image/png]\nembedded\n[resource file:///b]\n[resource file:///c]"
        )
    );
    assert_eq!(
        (stats.error_code, stats.text.as_str()),
        (None, r#"{"pages":3}"#),
        "the structured answer when no part spoke"
    );
    assert_eq!(
        (exploded.error_code, exploded.text.as_str()),
        (Some(code::MCP_FAILED), "[mcp_failed] it blew up")
    );
    assert_eq!(
        not_object.text,
        "[invalid_arguments] an MCP tool's arguments are an object"
    );
    assert_eq!(
        lock(&env.spawned).clone(),
        [vec!["docs-mcp".to_owned(), "--stdio".to_owned()]]
    );
}

#[tokio::test]
async fn the_mcp_resource_tools_list_and_read_what_a_server_holds() {
    let env = Arc::new(FakeEnv::serving(Behaviour::default()));
    let server = started(&env).await;
    let tools = mcp_tools(&server, Vec::new());

    let resources = dispatch(&tools, "list_mcp_resources", json!({})).await;
    let templates = dispatch(
        &tools,
        "list_mcp_resource_templates",
        json!({"server": "docs"}),
    )
    .await;
    let read = dispatch(
        &tools,
        "read_mcp_resource",
        json!({"server": "docs", "uri": "file:///guide.md"}),
    )
    .await;

    assert_eq!(
        resources.text,
        r#"docs: {"uri":"file:///guide.md","name":"guide"}"#
    );
    assert_eq!(
        templates.text,
        r#"docs: {"uriTemplate":"file:///{path}","name":"files"}"#
    );
    assert_eq!(
        read.text,
        "# Guide\n[image/png file:///logo.png]\n[bytes file:///raw]"
    );
}

/// A server the call does not name is refused by name; a run with no server
/// lists nothing.
#[tokio::test]
async fn the_mcp_resource_tools_refuse_a_server_the_run_does_not_have() {
    let env = Arc::new(FakeEnv::serving(Behaviour::default()));
    let server = started(&env).await;
    let tools = mcp_tools(&server, Vec::new());
    let none: Vec<Arc<Server>> = Vec::new();
    let mut empty: Tools<ContextTool<'_>> = Tools::new(Vec::new());
    empty.offer(Box::new(Typed::new(ListMcpResources::new(none.clone()))));
    empty.offer(Box::new(Typed::new(ListMcpResourceTemplates::new(none))));

    let listed = dispatch(&tools, "list_mcp_resources", json!({"server": "wiki"})).await;
    let templates = dispatch(
        &tools,
        "list_mcp_resource_templates",
        json!({"server": "wiki"}),
    )
    .await;
    let read = dispatch(
        &tools,
        "read_mcp_resource",
        json!({"server": "wiki", "uri": "file:///x"}),
    )
    .await;
    let nothing = dispatch(&empty, "list_mcp_resources", json!({})).await;
    let no_templates = dispatch(&empty, "list_mcp_resource_templates", json!({})).await;

    let refusal = "[mcp_server_not_found] wiki is not one of this run's MCP servers";
    assert_eq!(
        [
            listed.text.as_str(),
            templates.text.as_str(),
            read.text.as_str()
        ],
        [refusal; 3]
    );
    assert_eq!(listed.error_code, Some(code::MCP_SERVER_NOT_FOUND));
    assert_eq!(
        (nothing.text.as_str(), no_templates.text.as_str()),
        ("no server lists any", "no server lists any")
    );
}

/// A server's error answers the call that asked, as `mcp_failed`; the run
/// goes on.
#[tokio::test]
async fn a_servers_error_fails_the_call_that_asked_as_mcp_failed() {
    let env = Arc::new(FakeEnv::serving(Behaviour {
        failing: vec![
            "tools/call",
            "resources/list",
            "resources/templates/list",
            "resources/read",
        ],
        ..Behaviour::default()
    }));
    let server = started(&env).await;
    let listed = server.tools().await.expect("the server lists its tools");
    let offered: Vec<Box<ContextTool<'_>>> = listed
        .into_iter()
        .map(|tool| Box::new(tool) as Box<ContextTool<'_>>)
        .collect();
    let tools = mcp_tools(&server, offered);

    let outputs = [
        dispatch(&tools, "mcp__docs__stats", json!({})).await,
        dispatch(&tools, "list_mcp_resources", json!({})).await,
        dispatch(&tools, "list_mcp_resource_templates", json!({})).await,
        dispatch(
            &tools,
            "read_mcp_resource",
            json!({"server": "docs", "uri": "file:///x"}),
        )
        .await,
    ];

    for (output, method) in outputs.iter().zip([
        "tools/call",
        "resources/list",
        "resources/templates/list",
        "resources/read",
    ]) {
        assert_eq!(output.error_code, Some(code::MCP_FAILED), "{}", output.text);
        assert!(
            output.text.contains(&format!("{method} broke")),
            "{}",
            output.text
        );
    }
}

/// A server that never answers fails each call once the call's time is up;
/// the clock is paused, so the wait costs nothing.
#[tokio::test(start_paused = true)]
async fn a_server_that_never_answers_fails_each_call_at_its_timeout() {
    let env = Arc::new(FakeEnv::serving(Behaviour {
        silent: vec![
            "tools/call",
            "resources/list",
            "resources/templates/list",
            "resources/read",
        ],
        ..Behaviour::default()
    }));
    let server = started(&env).await;
    let listed = server.tools().await.expect("the server lists its tools");
    let offered: Vec<Box<ContextTool<'_>>> = listed
        .into_iter()
        .map(|tool| Box::new(tool) as Box<ContextTool<'_>>)
        .collect();
    let tools = mcp_tools(&server, offered);

    let called = dispatch(&tools, "mcp__docs__stats", json!({})).await;
    let listed = dispatch(&tools, "list_mcp_resources", json!({})).await;
    let templates = dispatch(&tools, "list_mcp_resource_templates", json!({})).await;
    let read = dispatch(
        &tools,
        "read_mcp_resource",
        json!({"server": "docs", "uri": "file:///x"}),
    )
    .await;

    assert_eq!(called.text, "[mcp_failed] docs did not answer within 120s");
    assert_eq!(listed.text, "[mcp_failed] deadline has elapsed");
    assert_eq!(templates.text, "[mcp_failed] deadline has elapsed");
    assert_eq!(read.text, "[mcp_failed] deadline has elapsed");
}

#[tokio::test(start_paused = true)]
async fn a_server_that_never_lists_its_tools_fails_at_the_timeout() {
    let env = Arc::new(FakeEnv::serving(Behaviour {
        silent: vec!["tools/list"],
        ..Behaviour::default()
    }));
    let server = started(&env).await;

    let refused = server.tools().await.expect_err("no listing came");

    assert_eq!(refused.to_string(), "deadline has elapsed");
}

#[tokio::test]
async fn a_server_that_lists_no_tools_on_error_says_why() {
    let env = Arc::new(FakeEnv::serving(Behaviour {
        failing: vec!["tools/list"],
        ..Behaviour::default()
    }));
    let server = started(&env).await;

    let refused = server.tools().await.expect_err("the listing failed");

    assert!(
        refused.to_string().contains("tools/list broke"),
        "{refused}"
    );
}

/// A server that exits with a request unanswered ends its session: the
/// request fails then, and does not wait out its timeout.
#[tokio::test(start_paused = true)]
async fn a_server_that_exits_mid_request_fails_the_request() {
    let env = Arc::new(FakeEnv::serving(Behaviour {
        exits_on: vec!["tools/list"],
        ..Behaviour::default()
    }));
    let server = started(&env).await;

    let refused = server.tools().await.expect_err("the server is gone");

    assert_ne!(refused.to_string(), "deadline has elapsed", "{refused}");
}

#[tokio::test]
async fn a_stdio_server_that_will_not_start_or_initialize_is_refused() {
    let unstartable = Arc::new(FakeEnv {
        refuse_spawn: true,
        ..FakeEnv::default()
    });
    let refusing = Arc::new(FakeEnv::serving(Behaviour {
        refuse_initialize: true,
        ..Behaviour::default()
    }));
    let spawn = Spawn::program("docs-mcp");

    let not_started = Server::stdio("docs", Arc::<FakeEnv>::clone(&unstartable), &spawn)
        .await
        .expect_err("no process");
    let not_initialized = Server::stdio("docs", Arc::<FakeEnv>::clone(&refusing), &spawn)
        .await
        .expect_err("refused");

    assert_eq!(not_started.to_string(), "no such program");
    assert!(
        not_initialized.to_string().contains("initialize refused"),
        "{not_initialized}"
    );
    assert_eq!(lock(&unstartable.spawned).len(), 1);
}

/// Dropping the last handle to a server ends its process through the
/// environment.
#[tokio::test]
async fn a_dropped_server_is_killed_through_its_environment() {
    let env = Arc::new(FakeEnv::serving(Behaviour::default()));
    let server = started(&env).await;
    let killed = env.killed_note.notified();

    drop(server);
    killed.await;

    assert_eq!(lock(&env.killed).clone(), [0]);
}

#[test]
fn an_mcp_tool_name_is_prefixed_sanitized_and_capped_at_64() {
    let long = "t".repeat(80);

    let named = [
        tool_name("docs", "search"),
        tool_name("my docs", "search.v2/\u{e9}"),
        tool_name("a-b", "c_d"),
        tool_name("docs", &long),
    ];

    assert_eq!(
        named.get(..3).map(<[String]>::to_vec),
        Some(vec![
            "mcp__docs__search".to_owned(),
            "mcp__my_docs__search_v2__".to_owned(),
            "mcp__a-b__c_d".to_owned(),
        ])
    );
    assert_eq!(named.get(3).map(String::len), Some(64));
    assert!(
        named
            .get(3)
            .is_some_and(|name| name.starts_with("mcp__docs__ttt"))
    );
}

// ---------------------------------------------------------------------------
// The browser tools
// ---------------------------------------------------------------------------

/// A browser that opens and screenshots, and whose every action fails; a
/// closed one fails its screenshots too.
#[derive(Debug, Default)]
struct FakeBrowser {
    opened: Mutex<Vec<Value>>,
    closed: bool,
}

impl Browser for FakeBrowser {
    fn open<'a>(&'a self, arguments: &'a Value) -> EnvFuture<'a, String> {
        lock(&self.opened).push(arguments.clone());
        Box::pin(std::future::ready(Ok("opened Example Domain".to_owned())))
    }

    fn act<'a>(&'a self, _arguments: &'a Value) -> EnvFuture<'a, String> {
        Box::pin(std::future::ready(Err(EnvError::new(
            EnvErrorKind::Unavailable,
            "the page closed",
        ))))
    }

    fn screenshot<'a>(&'a self, _arguments: &'a Value) -> EnvFuture<'a, Image> {
        if self.closed {
            return Box::pin(std::future::ready(Err(EnvError::new(
                EnvErrorKind::Unavailable,
                "no page is open",
            ))));
        }
        Box::pin(std::future::ready(Ok(Image {
            kind: ImageKind::Png,
            data: Bytes::from_static(b"\x89PNG"),
        })))
    }
}

fn browser_tools(env: &dyn Env) -> Tools<ContextTool<'_>> {
    let config = ToolsConfig::new(["browser_open", "browser", "screenshot"]).expect("three names");
    Tools::builder()
        .with_env(env)
        .build(&config)
        .expect("an env is given")
}

/// An environment with no browser refuses each browser tool with a code
/// naming why; nothing is spawned and the run goes on.
#[tokio::test]
async fn the_browser_tools_refuse_in_an_environment_with_no_browser() {
    let env = FakeEnv::default();
    let tools = browser_tools(&env);

    let mut read = Vec::new();
    for name in ["browser_open", "browser", "screenshot"] {
        let output = dispatch(&tools, name, json!({"url": "https://example.com"})).await;
        assert_eq!(output.error_code, Some(code::BROWSER_UNAVAILABLE));
        read.push(output.text);
    }

    assert_eq!(
        read,
        [
            "[browser_unavailable] browser_open has no browser here",
            "[browser_unavailable] browser has no browser here",
            "[browser_unavailable] screenshot has no browser here",
        ]
    );
    assert_eq!(lock(&env.spawned).len(), 0, "nothing was spawned");
}

#[tokio::test]
async fn the_browser_tools_drive_the_environments_browser() {
    let env = FakeEnv {
        browser: Some(FakeBrowser::default()),
        ..FakeEnv::default()
    };
    let tools = browser_tools(&env);

    let opened = dispatch(
        &tools,
        "browser_open",
        json!({"url": "https://example.com"}),
    )
    .await;
    let acted = dispatch(&tools, "browser", json!({"action": "click"})).await;
    let shot = dispatch(&tools, "screenshot", json!({})).await;

    assert_eq!(
        (opened.error_code, opened.text.as_str()),
        (None, "opened Example Domain")
    );
    assert_eq!(
        (acted.error_code, acted.text.as_str()),
        (
            Some(code::BROWSER_UNAVAILABLE),
            "[browser_unavailable] the page closed"
        )
    );
    assert_eq!(
        shot.text,
        "Attached a screenshot (4 bytes, image/png) for your next step"
    );
    assert_eq!(
        shot.image,
        Some(Image {
            kind: ImageKind::Png,
            data: Bytes::from_static(b"\x89PNG")
        })
    );
    assert_eq!(
        env.browser
            .as_ref()
            .map(|browser| lock(&browser.opened).clone()),
        Some(vec![json!({"url": "https://example.com"})])
    );
}

/// A screenshot the browser cannot take is a failed call carrying no image.
#[tokio::test]
async fn a_screenshot_the_browser_cannot_take_fails_without_an_image() {
    let env = FakeEnv {
        browser: Some(FakeBrowser {
            closed: true,
            ..FakeBrowser::default()
        }),
        ..FakeEnv::default()
    };
    let tools = browser_tools(&env);

    let shot = dispatch(&tools, "screenshot", json!({})).await;

    assert_eq!(
        (shot.error_code, shot.text.as_str(), shot.image),
        (
            Some(code::BROWSER_UNAVAILABLE),
            "[browser_unavailable] no page is open",
            None
        )
    );
}
