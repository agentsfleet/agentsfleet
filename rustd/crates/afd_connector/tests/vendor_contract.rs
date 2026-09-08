//! The vendor answers in the format this daemon parses — asked of the real
//! endpoints, with no credentials and nothing redeemed.
//!
//! # Why this suite exists
//!
//! `crate::complete` reads every provider's answer with `serde_json`. GitHub's
//! token endpoint answers `application/x-www-form-urlencoded` unless a caller
//! asks for JSON, so a daemon that sends no `Accept` redeems the code, gets the
//! grant issued, and then cannot read it — `UZ-CONN-006` over a connection the
//! provider considers made. That is what shipped, and 409 integration tests
//! held green over it, because the fake provider answered JSON whatever the
//! request asked for. A stub cannot hold this property. Only the vendor can.
//!
//! # Why junk credentials prove anything
//!
//! The format is chosen from the request's `Accept` before the credentials are
//! judged: GitHub answers `{"error":"Not Found"}` as JSON to a bogus client id
//! and `Not Found` as `text/plain` to the same request without the header. So a
//! refusal is exactly as good a witness as a grant, and it needs no secret, no
//! valid code, and redeems nothing. Nothing here can create or spend a grant.
//!
//! # Why it sends the daemon's own request
//!
//! Through [`Exchange::probe_request`], not a hand-rolled `reqwest` call. A
//! test that rebuilt the request would assert its own spelling of the headers,
//! which is the failure mode this suite exists to close.

use afd_connector::exchange::Exchange;
use afd_connector::provider::Provider;
use afd_connector::registry::Archetype;
use afd_connector::oauth;

/// Values that cannot redeem anything, and do not need to.
const NO_CLIENT: &str = "vendor-contract-probe-not-a-client";
const NO_SECRET: &str = "vendor-contract-probe-not-a-secret";
const NO_CODE: &str = "vendor-contract-probe-not-a-code";
const NO_REDIRECT: &str = "https://example.invalid/vendor-contract-probe";

/// The media type [`afd_connector::complete`] can read.
const JSON_MEDIA_TYPE: &str = "application/json";
const HEADER_CONTENT_TYPE: &str = "content-type";
/// A vendor call that hangs is an environment fact, not a contract failure —
/// the suite says which by timing out rather than blocking the lane.
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// Where this provider's grant is redeemed, whichever archetype it runs.
fn token_endpoint(provider: Provider) -> &'static str {
    match provider.archetype() {
        Archetype::Oauth2(flow) => flow.token_endpoint,
        Archetype::AppInstall(flow) => flow.token_endpoint,
    }
}

/// Every shipped provider answers the daemon's exchange request in JSON.
///
/// Walks [`Provider::ALL`] rather than naming five endpoints, so a sixth
/// provider cannot land without this proof covering it.
#[tokio::test]
#[ignore = "reaches the providers' live token endpoints: make test-integration-rustd"]
async fn every_provider_answers_the_exchange_in_the_format_the_daemon_parses() {
    let client = reqwest::Client::builder()
        .timeout(PROBE_TIMEOUT)
        .build()
        .expect("a client");
    let exchange = Exchange::new(client);
    let form = oauth::exchange_form(NO_CLIENT, NO_SECRET, NO_CODE, NO_REDIRECT);

    for &provider in Provider::ALL {
        let endpoint = token_endpoint(provider);
        let answer = exchange
            .probe_request(endpoint, &form)
            .send()
            .await
            .unwrap_or_else(|source| {
                panic!("{} at {endpoint} was unreachable: {source}", provider.id())
            });

        let media_type = answer
            .headers()
            .get(HEADER_CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();

        assert!(
            media_type.starts_with(JSON_MEDIA_TYPE),
            "{} answered `{media_type}` at {endpoint}; `complete` reads every \
             answer with serde_json, so anything else is a grant the daemon \
             cannot read. Check the `Accept` header the exchange sends.",
            provider.id(),
        );
    }
}
