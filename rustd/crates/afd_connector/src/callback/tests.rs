//! The dashboard URLs a connect round-trip and an invite are built from.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use super::{Dashboard, connected_url, relay_uri};
use crate::provider::Provider;
use afd_core::id::Uuid7;

/// The dashboard base a suite builds URLs under.
const DASHBOARD: &str = "https://app.example.test";

/// A workspace identifier the destination is built for.
const WORKSPACE: &str = "01920000-0000-7000-8000-000000000001";

/// The suite's dashboard, parsed as boot parses it.
fn dashboard(base: &str) -> Dashboard {
    Dashboard::parse(base).expect("a URL base")
}

/// The relay spells the route the dashboard actually mounts.
///
/// A vendor matches the registered callback URL literally, so the literal is
/// what has to be pinned: `path_segments_mut` encodes a slash inside a segment,
/// and a single `"api/connectors"` entry came out as `api%2Fconnectors`.
#[test]
fn the_relay_spells_the_route_the_dashboard_mounts() {
    assert_eq!(
        relay_uri(&dashboard(DASHBOARD), Provider::GitHub),
        format!("{DASHBOARD}/api/connectors/github/callback"),
    );
    for provider in Provider::ALL.iter().copied() {
        let minted = relay_uri(&dashboard(DASHBOARD), provider);
        assert!(
            !minted.contains('%'),
            "`{provider}` relay carries percent-encoding: {minted}",
        );
    }
}

/// A trailing slash on the configured base does not double.
///
/// An operator writes `https://app.example.test/` as readily as without,
/// and `{s}{s}` concatenation turns that into `//api/connectors/...` — a
/// different path to the provider, and therefore a different redirect URI
/// from the one the code was minted against.
#[test]
fn a_trailing_slash_on_the_base_does_not_become_a_double_slash() {
    assert_eq!(
        relay_uri(&dashboard("https://app.example.test/"), Provider::Slack),
        relay_uri(&dashboard(DASHBOARD), Provider::Slack),
    );
}

/// A sub-path on the base survives, and each page segment is one segment.
#[test]
fn a_page_keeps_the_base_sub_path_and_encodes_each_segment_as_one() {
    let page = dashboard("https://app.example.test/dash/").page(["invites", "a/b"]);
    assert_eq!(page.as_str(), "https://app.example.test/dash/invites/a%2Fb");
}

/// A base no page can hang off is refused when it is read, so no request
/// ever builds half a URL from it.
#[test]
fn a_base_that_is_not_a_url_is_refused_when_it_is_read() {
    for base in ["", "not a url", "/relative", "mailto:ops@example.test"] {
        assert_eq!(Dashboard::parse(base), None, "`{base}` is no base");
    }
}

/// Boot promises an absolute http(s) URL: another scheme is no page a browser
/// opens, credentials would ride every link, and a query or a fragment would
/// sit after every path segment a page appends.
#[test]
fn a_base_that_is_not_a_bare_http_url_is_refused_when_it_is_read() {
    for base in [
        "ftp://app.example.test",
        "file:///srv/dashboard",
        "https://u:p@app.example.test",
        "https://u@app.example.test",
        "https://:p@app.example.test",
        "https://app.example.test/?q=1",
        "https://app.example.test/#f",
    ] {
        assert_eq!(Dashboard::parse(base), None, "`{base}` is no base");
    }
    for base in ["http://localhost:3000", "https://app.example.test/dash/"] {
        assert!(Dashboard::parse(base).is_some(), "`{base}` is a base");
    }
}

/// The base reads back as the URL it parsed to.
#[test]
fn a_base_reads_back_as_the_url_it_parsed_to() {
    assert_eq!(
        dashboard(DASHBOARD).as_str(),
        "https://app.example.test/",
        "the parsed form, which the device-flow surface trims"
    );
}

/// The destination names the workspace the connect landed in.
#[test]
fn a_finished_connect_lands_on_its_own_workspaces_page() {
    let workspace = Uuid7::parse(WORKSPACE).expect("a canonical identifier");

    assert_eq!(
        connected_url(&dashboard(DASHBOARD), &workspace),
        "https://app.example.test/w/01920000-0000-7000-8000-000000000001/integrations",
    );
}
