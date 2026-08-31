//! Building the callback the external scheduler is told to post to.
//!
//! # Why a query or a fragment is refused rather than stripped
//!
//! The destination rides RAW inside the provider's own request path. A `?` in
//! it would be read as the PROVIDER request's query and a `#` as its fragment,
//! registering a callback silently truncated at that byte — one this daemon
//! never meant and cannot serve. `constants.zig` refuses at construction for
//! the same reason, and refusing beats stripping because a deployment whose
//! configured URL carries a query is misconfigured, and quietly registering a
//! different URL hides that until the first fire never arrives.
//!
//! # The percent-encoded cases are the whole reason this is parsed
//!
//! The check is asked of the PARSED url, not of the string. A `#` that is
//! percent-encoded inside a path is not a fragment, and a substring search
//! would refuse a destination that is perfectly good. Both directions are
//! wrong, and both are covered below.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_cron::qstash::{INGRESS_PATH, InvalidDestination, MAX_DESTINATION_BYTES, destination_url};

/// A deployment's API URL, as an operator would configure it.
const API: &str = "https://api.example.com";

#[test]
fn a_plain_origin_becomes_the_ingress_path_beneath_it() {
    assert_eq!(
        destination_url(API).expect("a plain origin is usable"),
        format!("{API}{INGRESS_PATH}")
    );
}

/// A configured URL written with a trailing slash is the same deployment.
///
/// Left alone it would produce a double slash, and a scheduler posting to
/// `//v1/...` reaches a path this router does not serve — a misconfiguration
/// that only shows up at the first fire, hours later.
#[test]
fn a_trailing_slash_does_not_become_a_double_one() {
    let destination = destination_url(&format!("{API}/")).expect("a trailing slash is usable");

    assert_eq!(destination, format!("{API}{INGRESS_PATH}"));
    assert!(!destination.contains("//v1"), "got {destination}");
}

#[test]
fn a_url_carrying_a_query_is_refused_rather_than_stripped() {
    assert_eq!(
        destination_url(&format!("{API}/?tenant=acme")),
        Err(InvalidDestination::NotAPlainOrigin)
    );
}

#[test]
fn a_url_carrying_a_fragment_is_refused_rather_than_stripped() {
    assert_eq!(
        destination_url(&format!("{API}/#section")),
        Err(InvalidDestination::NotAPlainOrigin)
    );
}

/// An escaped `#` is a path byte, not a fragment, and must not be refused.
///
/// This is the direction a substring search gets wrong the OTHER way: it would
/// see a `#`, call the destination unusable, and refuse a deployment whose URL
/// is entirely valid.
#[test]
fn a_percent_encoded_hash_in_the_path_is_not_a_fragment() {
    let destination =
        destination_url(&format!("{API}/base%23one")).expect("an escaped hash is a path byte");

    assert!(destination.ends_with(INGRESS_PATH), "got {destination}");
    assert!(
        destination.contains("%23"),
        "the escape survives: {destination}"
    );
}

#[test]
fn a_value_that_is_not_a_url_at_all_is_unusable() {
    for candidate in [
        "",
        "   ",
        "api.example.com",
        "not a url",
        "://missing-scheme",
    ] {
        assert_eq!(
            destination_url(candidate),
            Err(InvalidDestination::Unusable),
            "`{candidate}` is not a URL this daemon can register a callback on"
        );
    }
}

/// A destination past the cap is refused, not registered and truncated.
#[test]
fn a_destination_over_the_cap_is_refused() {
    let long = format!("{API}/{}", "a".repeat(MAX_DESTINATION_BYTES));

    assert_eq!(
        destination_url(&long),
        Err(InvalidDestination::Unusable),
        "a destination longer than the cap must not be registered"
    );
}

/// The cap is measured on the FINAL destination, not on the configured URL.
///
/// A URL that fits on its own but not once the ingress path is appended is the
/// case a check on the input alone would let through.
#[test]
fn the_cap_is_measured_after_the_path_is_appended() {
    let filler = MAX_DESTINATION_BYTES - API.len() - INGRESS_PATH.len();
    let just_over = format!("{API}/{}", "a".repeat(filler));

    assert!(
        just_over.len() <= MAX_DESTINATION_BYTES,
        "the configured URL itself is within the cap: {}",
        just_over.len()
    );
    assert_eq!(
        destination_url(&just_over),
        Err(InvalidDestination::Unusable),
        "it is only over once the ingress path is on it"
    );
}

/// The ingress path is half of the claim a fire token is verified against.
///
/// `cron/constants.zig`'s `ingress_path`, kept byte-for-byte. A divergence
/// would make every schedule registered by one daemon unverifiable by the
/// other — the failure would not appear until a fire arrived and was refused
/// as being for somebody else's daemon.
#[test]
fn the_ingress_path_is_the_one_the_router_serves() {
    assert_eq!(INGRESS_PATH, "/v1/ingress/qstash/schedules");
    assert!(
        INGRESS_PATH.starts_with('/') && !INGRESS_PATH.ends_with('/'),
        "the path is appended directly to an origin: {INGRESS_PATH}"
    );
}
