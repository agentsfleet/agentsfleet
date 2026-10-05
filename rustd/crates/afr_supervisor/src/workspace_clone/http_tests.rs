//! The fetch over HTTP, against `git http-backend` behind a listener that
//! answers only the credentials the token makes: the transport production
//! fetches GitHub through, less its TLS. `file://` never sends a header, so
//! only these prove the token reaches the origin at all.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use afr_sandbox::HostWorkspace;
use afr_tools::sandbox::Checkout;
use axum::body::{Body, Bytes};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE, WWW_AUTHENTICATE};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::Response;
use base64::Engine as _;
use tokio_util::sync::CancellationToken;

use super::tests::{BASE, Fixture, NAME, ORIGINS, OWNER, REPOSITORY, SCOPE, TOKEN};
use super::{Fetched, Mirrors, Request};
use crate::test_support::head;

/// The lease each test checks out for first.
const FIRST_LEASE: &str = "lease_1";
/// The status line a CGI program writes in place of HTTP's.
const CGI_STATUS: &str = "Status";
/// Where the CGI program's headers end and its body begins.
const HEAD_END: &[u8] = b"\r\n\r\n";

/// What the listener counted: requests it served, and requests it refused.
#[derive(Debug, Default)]
struct Seen {
    served: AtomicUsize,
    refused: AtomicUsize,
}

/// The `Authorization` value GitHub reads an installation token from over
/// git's HTTPS, spelled out from its documented form.
fn credentials(token: &str) -> String {
    let pair = format!("x-access-token:{token}");
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(pair)
    )
}

/// Serves the repositories under `root` through `git http-backend` to a
/// client presenting exactly `expected`; its base URL, and its counts.
async fn origin(root: PathBuf, expected: String) -> (String, Arc<Seen>) {
    let seen = Arc::new(Seen::default());
    let counts = Arc::clone(&seen);
    let app = axum::Router::new().fallback(
        move |method: Method, uri: Uri, headers: HeaderMap, body: Bytes| {
            let (root, expected, counts) = (root.clone(), expected.clone(), Arc::clone(&counts));
            async move {
                let presented = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok());
                if presented != Some(expected.as_str()) {
                    counts.refused.fetch_add(1, Ordering::SeqCst);
                    return Response::builder()
                        .status(StatusCode::UNAUTHORIZED)
                        .header(WWW_AUTHENTICATE, "Basic realm=\"git\"")
                        .body(Body::empty())
                        .unwrap();
                }
                counts.served.fetch_add(1, Ordering::SeqCst);
                tokio::task::spawn_blocking(move || backend(&root, &method, &uri, &headers, &body))
                    .await
                    .unwrap()
            }
        },
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{address}/"), seen)
}

/// One request through `git http-backend`, run as the CGI program it is.
fn backend(root: &Path, method: &Method, uri: &Uri, headers: &HeaderMap, body: &Bytes) -> Response {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned()
    };
    let mut child = Command::new("git")
        .arg("http-backend")
        .env("GIT_PROJECT_ROOT", root)
        .env("GIT_HTTP_EXPORT_ALL", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("REQUEST_METHOD", method.as_str())
        .env("PATH_INFO", uri.path())
        .env("QUERY_STRING", uri.query().unwrap_or_default())
        .env("CONTENT_TYPE", header(CONTENT_TYPE.as_str()))
        .env("CONTENT_LENGTH", body.len().to_string())
        .env("HTTP_CONTENT_ENCODING", header("content-encoding"))
        .env("GIT_PROTOCOL", header("git-protocol"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(body).unwrap();
    let output = child.wait_with_output().unwrap();
    let end = output
        .stdout
        .windows(HEAD_END.len())
        .position(|window| window == HEAD_END)
        .unwrap();
    let mut response = Response::builder();
    for line in String::from_utf8_lossy(&output.stdout[..end]).lines() {
        let (name, value) = line.split_once(": ").unwrap();
        response = if name == CGI_STATUS {
            response.status(value[..3].parse::<u16>().unwrap())
        } else {
            response.header(name, value)
        };
    }
    let rest = output.stdout[end + HEAD_END.len()..].to_vec();
    response.body(Body::from(rest)).unwrap()
}

/// Checks [`REPOSITORY`] out into `workspace` from `mirrors`, presenting
/// [`TOKEN`].
async fn check_out(
    fixture: &Fixture,
    mirrors: &Mirrors,
    workspace: &Path,
) -> crate::error::Result<Fetched> {
    let request = Request {
        scope: SCOPE,
        checkout: Checkout {
            repository: REPOSITORY,
            owner: OWNER,
            name: NAME,
            base: BASE,
        },
        token: TOKEN,
        workspace: HostWorkspace {
            root: workspace,
            owner: fixture.owner,
        },
    };
    mirrors.check_out(request, &CancellationToken::new()).await
}

#[tokio::test(flavor = "multi_thread")]
async fn the_fetch_presents_the_token_as_basic_credentials() {
    let fixture = Fixture::new();
    let root = fixture.root.path().join(ORIGINS);
    let (url, seen) = origin(root, credentials(TOKEN)).await;
    let mirrors = Mirrors::new(fixture.root.path().join("http"), url);
    let first = fixture.workspace(FIRST_LEASE);
    let second = fixture.workspace("lease_2");

    let cloned = check_out(&fixture, &mirrors, &first).await.unwrap();
    let fetched = check_out(&fixture, &mirrors, &second).await.unwrap();

    assert_eq!((cloned, fetched), (Fetched::Cloned, Fetched::Unchanged));
    assert_eq!(
        seen.refused.load(Ordering::SeqCst),
        0,
        "every request carried it"
    );
    assert!(seen.served.load(Ordering::SeqCst) >= 2, "{seen:?}");
    assert_eq!(
        head(&second.join(NAME), "HEAD"),
        head(&fixture.remote(), BASE)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_token_the_origin_refuses_fails_the_fetch() {
    let fixture = Fixture::new();
    let root = fixture.root.path().join(ORIGINS);
    let (url, seen) = origin(root, credentials("ghs_someOtherToken")).await;
    let mirrors = Mirrors::new(fixture.root.path().join("http"), url);

    let refused = check_out(&fixture, &mirrors, &fixture.workspace(FIRST_LEASE))
        .await
        .unwrap_err();

    assert!(refused.to_string().contains("fetched"), "{refused}");
    assert!(seen.refused.load(Ordering::SeqCst) >= 1);
    assert_eq!(seen.served.load(Ordering::SeqCst), 0, "nothing was served");
}
