//! Whether a tenant-supplied endpoint may be dialed, and by which provider.
//!
//! The URL is hostile input. A tenant who can set it can aim the runner's
//! egress at a loopback admin port, the cloud metadata service, or an internal
//! RFC1918 host — Server-Side Request Forgery. This module refuses those at the
//! RESOLVE boundary, before a lease exists, so a blocked endpoint never reaches
//! the engine or the egress allowlist.
//!
//! # Two rules, and they are one function
//!
//! The Zig splits them across two files: `base_url_guard.validate` answers
//! whether a URL is safe, and `secret_probe.validateSecretEndpoint` answers
//! whether this PROVIDER may carry one at all. Both are pure, both are always
//! called together, and the second is meaningless without the first — a named
//! provider that smuggles a `base_url` widens the egress allowlist without
//! going through the compatible path, which is a bypass rather than a typo.
//!
//! [`resolve`] is the pair, stated once. [`validate`] stays a separate function
//! rather than being inlined into it, because the egress allowlist needs the bare
//! HOST — which is the one thing the pairing rule discards — and that caller
//! lands in the sibling slice that builds the execution policy.
//!
//! # The parse is `url`'s, not thirty hand-written lines
//!
//! `base_url_guard.zig` scans for `://`, finds the first `/?#`, takes the last
//! `@`, and looks for a `:` unless the authority opens with `[` — a URL parser,
//! written by hand, in the one place where disagreeing with the client that
//! will actually dial the address is a security hole. This uses `url` 2.5.8,
//! already in the lock, and gets a TYPED [`Host`] out of it: an IP literal
//! arrives as an `Ipv4Addr` or `Ipv6Addr` and goes straight to the classifier
//! with no string round trip, and bracket stripping, zone ids, percent-encoded
//! authorities and userinfo are all handled by something maintained.
//!
//! The one thing kept by hand is how an IPv6 host is RENDERED back. `url` hands
//! it over unbracketed and `execution_policy.zig::hostFromUrl` produces the
//! bracketed form, and this value travels to a stock Zig runner as its
//! egress-allowlist entry. Normalisation is otherwise safe here, which was
//! checked rather than assumed: the runner compares allowlist entries with
//! `std.ascii.eqlIgnoreCase` at all three of its matching sites, so a
//! lower-cased host still matches.
//!
//! # Three verdicts differ from the hand-written guard, and all three are it
//!
//! Every one is a case where the Zig disagrees with what an HTTP client would
//! actually dial, which is the only thing this guard is for — a verdict about a
//! host nobody will connect to protects nothing.
//!
//! - `https:///just/a/path` — the Zig calls it malformed. A client following
//!   WHATWG skips the extra slash and dials `just`, and so does this. The SSRF
//!   check still runs on that host, so `https:///169.254.169.254` is refused
//!   exactly as the two-slash spelling is.
//! - `https://256.1.1.1/v1` — the Zig's `parseIpv4` fails, concludes "not a
//!   literal", and passes it through as a NAME. WHATWG reads a four-part
//!   all-numeric host as an IPv4 attempt and refuses the out-of-range octet, so
//!   this refuses it too. The safe direction, and the one a client takes.
//! - A schemeless host reports `InvalidScheme` rather than the parser's own
//!   "relative URL" — the diagnosis an operator can act on, and the Zig's.

/// The provider id that opts a credential into a custom OpenAI-compatible
/// endpoint.
///
/// The credential JSON's own `provider` value, and deliberately distinct from
/// the `custom:<url>` name the runner is handed on the wire — one names the
/// SHAPE of the credential, the other names the dial.
pub const OPENAI_COMPATIBLE: &str = "openai-compatible";

/// Why an endpoint was refused.
///
/// Carried as a reason rather than collapsed into a bool because the rejection
/// LOG names it, and an operator reading "blocked host" acts differently from
/// one reading "not https". Only the reason and the host are ever logged; the
/// `api_key` sitting beside the URL in the same credential is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    /// The scheme is not `https` — plain http, another scheme, or none at all.
    InvalidScheme,
    /// The host is an IP literal in an SSRF-unsafe range.
    BlockedHost,
    /// There is no parseable authority to check.
    Malformed,
    /// A named provider carried an endpoint, which only the compatible
    /// provider may do.
    NotPermitted,
    /// The compatible provider carried no endpoint, which it must.
    Required,
}

impl Rejection {
    /// The word this rejection is logged under.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidScheme => "invalid_scheme",
            Self::BlockedHost => "blocked_host",
            Self::Malformed => "malformed",
            Self::NotPermitted => "not_permitted",
            Self::Required => "required",
        }
    }
}

/// A custom endpoint that passed every check, and the host it resolved to.
///
/// The two travel together because they are one decision. The host is what the
/// egress allowlist admits and the URL is what the run dials, so a shape that
/// carried only the URL would leave every consumer re-parsing it to recover the
/// host — and a second parse is a second chance to disagree with the one that
/// did the SSRF check. `resolve` computed this host to make its decision; it
/// hands it over rather than discarding it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Endpoint<'a> {
    /// The URL the run dials, borrowed from the stored credential.
    pub url: &'a str,
    /// The bare host the egress allowlist admits.
    pub host: Box<str>,
}

/// The endpoint `provider` may dial, given what its credential declared.
///
/// `Ok(None)` is a named provider with no endpoint, which is the ordinary case:
/// it dials a built-in host and has nothing to validate. `Ok(Some(endpoint))`
/// is a compatible provider whose endpoint passed every check.
///
/// # Errors
/// Refuses a named provider that carries an endpoint, a compatible provider
/// that carries none, and any endpoint [`validate`] rejects.
pub(super) fn resolve<'a>(
    provider: &str,
    base_url: Option<&'a str>,
) -> Result<Option<Endpoint<'a>>, Rejection> {
    if provider != OPENAI_COMPATIBLE {
        return base_url.map_or(Ok(None), |_smuggled| Err(Rejection::NotPermitted));
    }
    let url = base_url.ok_or(Rejection::Required)?;
    validate(url).map(|host| Some(Endpoint { url, host }))
}

/// The bare host of a safe `https` endpoint.
///
/// Order matters and is the Zig's: scheme first because it is the cheapest
/// check and an `http` URL is refused whatever its host, then the SSRF
/// classification of whatever the parser resolved.
///
/// Owned rather than borrowed from the input, because the parser normalises and
/// the result is no longer a slice of what came in. That is the honest
/// signature: a borrow would have implied the bytes are the caller's.
///
/// # Errors
/// Refuses a URL that does not parse, a non-`https` scheme, an authority with
/// no host, and a host that is an SSRF-unsafe address.
mod url;

pub(crate) use self::url::validate;
