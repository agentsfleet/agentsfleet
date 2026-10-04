//! How a failure becomes an [`Error`](super::Error): the lift, and a raiser
//! for each refusal that carries data.

use super::{Error, ErrorKind};

// A client reqwest would not build is the one lift: `?` meets it in
// `Network::new` (`docs/RUST_ERROR_STANDARD.md` rule 2).
afd_core::error_lifts!(Error, ErrorKind:
    reqwest::Error => Client,
);

/// The URL does not parse, for `reason`.
pub(crate) fn invalid_url(reason: url::ParseError) -> Error {
    Error::from(ErrorKind::InvalidUrl { reason })
}

/// Header `name` cannot be sent.
pub(crate) fn invalid_header(name: &str) -> Error {
    Error::from(ErrorKind::InvalidHeader {
        name: name.to_owned(),
    })
}

/// The URL is not HTTPS.
pub(crate) fn https_required() -> Error {
    Error::from(ErrorKind::HttpsRequired)
}

/// `method` is not sent here.
pub(crate) fn method_not_allowed(method: &str) -> Error {
    Error::from(ErrorKind::MethodNotAllowed {
        method: method.to_owned(),
    })
}

/// `host` is not in the fleet's network allowlist.
pub(crate) fn host_not_allowed(host: &str) -> Error {
    Error::from(ErrorKind::HostNotAllowed {
        host: host.to_owned(),
    })
}

/// `host` is, or resolves to, an address this runner never reaches.
pub(crate) fn address_not_allowed(host: &str) -> Error {
    Error::from(ErrorKind::AddressNotAllowed {
        host: host.to_owned(),
    })
}

/// `what` stood where no placeholder or header may.
pub(crate) fn placement_not_allowed(what: &str) -> Error {
    Error::from(ErrorKind::PlacementNotAllowed {
        what: what.to_owned(),
    })
}

/// Credential `name` is not sent to `host`.
pub(crate) fn credential_host_not_allowed(name: &str, host: &str) -> Error {
    Error::from(ErrorKind::CredentialHostNotAllowed {
        name: name.to_owned(),
        host: host.to_owned(),
    })
}

/// No rule at `host` admits `method` `path` with this body.
pub(crate) fn request_policy_not_allowed(host: &str, method: &str, path: &str) -> Error {
    Error::from(ErrorKind::RequestPolicyNotAllowed {
        host: host.to_owned(),
        method: method.to_owned(),
        path: path.to_owned(),
    })
}

/// A minted token the masker would not take, for `source`.
pub(crate) fn unmaskable(source: afr_secrets::Error) -> Error {
    Error::from(ErrorKind::Unmaskable { source })
}

/// `host` could not be reached, for `reason`, a fixed phrase with no URL.
pub(crate) fn upstream_unreachable(host: &str, reason: &'static str) -> Error {
    Error::from(ErrorKind::UpstreamUnreachable {
        host: host.to_owned(),
        reason,
    })
}

/// One error of each kind, named, for the suites that read every refusal the
/// way the model does. `Unmaskable` is not among them: no input makes the
/// masker refuse a token, so none can be built here.
///
/// # Panics
/// When reqwest builds a request from a URL that does not parse, which would
/// be a change in reqwest rather than a runtime condition.
#[cfg(any(test, feature = "test-util"))]
#[must_use]
#[expect(
    clippy::expect_used,
    reason = "a sample builder whose own preconditions fail should stop the suite"
)]
pub fn one_of_each_kind() -> Vec<(&'static str, Error)> {
    let unbuilt = reqwest::Client::new()
        .get("not a url")
        .build()
        .expect_err("reqwest refuses a URL that does not parse");
    vec![
        ("client", Error::from(unbuilt)),
        ("invalid url", invalid_url(url::ParseError::EmptyHost)),
        ("invalid header", invalid_header("x")),
        ("https required", https_required()),
        ("method not allowed", method_not_allowed("TRACE")),
        ("host not allowed", host_not_allowed("h.example")),
        ("address not allowed", address_not_allowed("10.0.0.1")),
        ("placement not allowed", placement_not_allowed("w")),
        (
            "credential host not allowed",
            credential_host_not_allowed("n", "h.example"),
        ),
        ("secret not found", Error::secret_not_found("n", "f")),
        (
            "request policy not allowed",
            request_policy_not_allowed("h.example", "POST", "/"),
        ),
        (
            "mint refused",
            Error::mint_refused(afd_core::error_code::GH_MINT_FAILED, "d"),
        ),
        (
            "upstream unreachable",
            upstream_unreachable("h.example", "r"),
        ),
    ]
}
