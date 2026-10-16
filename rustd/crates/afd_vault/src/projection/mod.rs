//! What a stored credential IS, read off its body and never off its name.
//!
//! Pure over a parsed JSON object: no pool, no key, no allocation the caller
//! does not keep. Its classification rules are what the dashboard's
//! `SECRET_KIND` union is written against, so they are a wire fact.
//!
//! # There is no field for the key, and that is the guarantee
//!
//! [`Projection`] carries a `has_key` BOOLEAN and nothing that could hold the
//! key itself. The four `meta_*` columns behind it inherit the same property —
//! the table has no column a key would fit in — so a careless future projection
//! cannot leak one, because there is nowhere to put it. That is
//! `M-STRONG-TYPES-GUARD` applied to a confidentiality invariant: the compiler
//! refuses, rather than a reviewer noticing.
//!
//! # Why `model` is not projected
//!
//! `schema/300_vault_secrets.sql` declares four `meta_*` columns and `model` is
//! not among them, so the only way to answer it on a list is to decrypt every
//! row — which is precisely what spec Invariant 3 forbids. It is optional in
//! `SecretSummary` and in the dashboard's `Secret` union, and no client reads
//! it, so the never-decrypt guarantee wins and the field is omitted. Recorded
//! as a declared divergence rather than closed with a schema change this
//! milestone's gate table says it does not make.

use serde_json::{Map, Value};

/// The `provider` field, which classification keys on.
const FIELD_PROVIDER: &str = "provider";

/// The `base_url` field, carried only by a custom endpoint.
const FIELD_BASE_URL: &str = "base_url";

/// The `api_key` field, tested for presence and never read out.
const FIELD_API_KEY: &str = "api_key";

/// The provider id that opts a credential into a custom OpenAI-compatible
/// endpoint.
///
/// `base_url` is meaningful exactly when the provider equals this. The runner
/// uses the distinct `custom:<url>` wire name when dialing, never this id, so
/// the constant belongs beside the classification that is its only reader
/// (RULE UFS) — and is crate-private until a second reader exists. §2's model
/// registry is the one that will want it; exporting it before then would be
/// publishing a name on the strength of a guess.
pub(crate) const OPENAI_COMPATIBLE_PROVIDER: &str = "openai-compatible";

/// The scheme separator a URL's authority begins after.
const SCHEME_SEPARATOR: &str = "://";

/// What a stored credential is, derived from its `provider` field.
///
/// The stored spelling is the wire value and is kept verbatim in the dashboard's
/// `SECRET_KIND` union (the cross-runtime half of RULE UFS), so a rename here is
/// a wire break there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A named provider's key — anthropic, openai, and the rest.
    ProviderKey,
    /// An OpenAI-compatible endpoint the operator supplied a URL for.
    CustomEndpoint,
    /// Anything else, including a body this daemon could not describe.
    CustomSecret,
}

impl Kind {
    /// The stored spelling — the bytes in `meta_kind`, and on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProviderKey => "provider_key",
            Self::CustomEndpoint => "custom_endpoint",
            Self::CustomSecret => "custom_secret",
        }
    }

    /// The kind a stored spelling names, if this daemon knows it.
    ///
    /// `None` rather than a default, so the DEGRADE decision is the read path's
    /// to make and to explain — see [`crate::read`]. A `parse` that silently
    /// answered `CustomSecret` would hide a newer daemon's spelling at the one
    /// place it is worth seeing in a log.
    ///
    /// Crate-private: the only caller is [`crate::read`], and a stored spelling
    /// is a thing this crate reads OUT of a column rather than something a
    /// caller hands in.
    #[must_use]
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        [Self::ProviderKey, Self::CustomEndpoint, Self::CustomSecret]
            .into_iter()
            .find(|kind| kind.as_str() == raw)
    }
}

/// The non-secret descriptors one credential is listed by.
///
/// Crate-private. A caller outside sees [`crate::SecretSummary`], which is what
/// a LIST answers with; this is the write path's intermediate, produced by
/// [`crate::SecretBody`] and consumed by the statement beside it. Exporting it
/// would offer callers a projection they have no way to obtain and no use for.
///
/// Owned rather than borrowed from the parse: a body is at most four kilobytes,
/// so owning the two short strings costs less than the lifetime a borrow would
/// thread through the write path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Projection {
    /// What the credential is.
    pub kind: Kind,
    /// The provider label, for the two kinds that carry one.
    pub provider: Option<Box<str>>,
    /// The custom endpoint, when it is one this daemon may display.
    pub base_url: Option<Box<str>>,
    /// Whether a non-empty `api_key` is stored. Never the key.
    pub has_key: bool,
}

impl Projection {
    /// The projection for `body`.
    ///
    /// Crate-visible: the only way to obtain one outside this crate is to build
    /// a [`crate::SecretBody`], which produces the projection and the plaintext
    /// TOGETHER from one parse. That is what makes "the `meta_*` columns
    /// describe the ciphertext beside them" a property of the type rather than
    /// of the statement — a caller has no projection of its own to pass in.
    pub(crate) fn of(body: &Map<String, Value>) -> Self {
        let kind = classify(body);
        let has_key = has_non_empty_api_key(body);
        // Bound once per arm rather than built into a mutable default: each kind
        // carries exactly the descriptors it has, and an arm that forgot one
        // would be a missing field rather than a stale value.
        match kind {
            // An opaque secret carries no descriptors — but it may still hold a
            // key, and the tenant Models page reports presence for it the same
            // way, so `has_key` is computed for every kind.
            Kind::CustomSecret => Self {
                kind,
                provider: None,
                base_url: None,
                has_key,
            },
            Kind::ProviderKey => Self {
                kind,
                provider: owned_string(body, FIELD_PROVIDER),
                base_url: None,
                has_key,
            },
            Kind::CustomEndpoint => Self {
                kind,
                provider: owned_string(body, FIELD_PROVIDER),
                base_url: displayable_base_url(body),
                has_key,
            },
        }
    }
}

/// Classifies by the `provider` field, never by the operator-chosen name.
///
/// A missing or non-string provider is an opaque `custom_secret`; the
/// openai-compatible id is a `custom_endpoint`; any other provider id is a
/// `provider_key`. A custom secret that happens to carry a string `provider`
/// misfiles as a provider key — the accepted edge, recorded in the spec's
/// Product Clarity and preserved here rather than quietly corrected.
fn classify(body: &Map<String, Value>) -> Kind {
    match body.get(FIELD_PROVIDER).and_then(Value::as_str) {
        None => Kind::CustomSecret,
        Some(OPENAI_COMPATIBLE_PROVIDER) => Kind::CustomEndpoint,
        Some(_named) => Kind::ProviderKey,
    }
}

/// Whether the body carries a non-empty `api_key` string.
///
/// The value is compared against zero length and then dropped — it is never
/// returned, logged, or copied. One function at one moment over one parse,
/// because two producers is how a stored projection drifts from the body it
/// describes.
fn has_non_empty_api_key(body: &Map<String, Value>) -> bool {
    body.get(FIELD_API_KEY)
        .and_then(Value::as_str)
        .is_some_and(|key| !key.is_empty())
}

/// The `base_url` as it may be stored IN PLAINTEXT and shown to callers, or
/// nothing when it may not be.
///
/// The projection moved this value out of the AES-GCM envelope and into a
/// column, on the reasoning that every projected field is metadata any
/// authorized caller already sees. That holds for a scheme, host, port and
/// path. It does not hold for `https://user:pw@host/v1`: a body this crate is
/// handed can carry a password inside its URL, and promoting that string
/// verbatim turns a key-protected secret into one any database reader can
/// `SELECT`.
///
/// Omitted rather than rewritten. A misconfigured credential showing no
/// endpoint is a better outcome than a column showing the password, and
/// stripping the userinfo would invent a URL the operator never wrote.
fn displayable_base_url(body: &Map<String, Value>) -> Option<Box<str>> {
    let url = body.get(FIELD_BASE_URL).and_then(Value::as_str)?;
    (!raw_authority_has_userinfo(url) && !parsed_userinfo(url)).then(|| url.into())
}

/// Whether the authority, split the plain way, carries a `@`.
///
/// Only the AUTHORITY is examined. A `@` after it is an ordinary path or query
/// byte, and dropping those URLs would hide legitimate endpoints for no gain.
fn raw_authority_has_userinfo(url: &str) -> bool {
    let authority = match url.split_once(SCHEME_SEPARATOR) {
        None => url,
        Some((_scheme, rest)) => rest.split(['/', '?', '#']).next().unwrap_or(rest),
    };
    authority.contains('@')
}

/// Whether the URL a client would actually dial carries a user or a password.
///
/// The parse finds what the split above cannot: `https:///u:p@h/v1` has an
/// empty authority to the split, and a user `u` with password `p` to every
/// WHATWG parser. Either check firing hides the URL.
fn parsed_userinfo(url: &str) -> bool {
    url::Url::parse(url)
        .is_ok_and(|parsed| !parsed.username().is_empty() || parsed.password().is_some())
}

/// One string field of the body, owned, or nothing when it is absent or not a
/// string.
fn owned_string(body: &Map<String, Value>, field: &str) -> Option<Box<str>> {
    body.get(field).and_then(Value::as_str).map(Box::from)
}

#[cfg(test)]
mod tests;
