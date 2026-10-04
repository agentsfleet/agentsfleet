//! Whether one request may leave, decided before any connection exists.
//!
//! One pipeline, each step refusing with what it found: the method; where
//! each placeholder stands; HTTPS; the host in the allowlist; an address
//! literal outside the private ranges; each credential bound to this host; the
//! host's origin rules; and `read_only`. The steps are the Zig runner's
//! (`policy_http_request.zig`), stricter where a host serving many tenants
//! needs it: a misplaced placeholder refuses under every policy, not only
//! under `read_only`, and an allowlisted host still never reaches a private
//! address (the resolver in `network.rs` holds that line for names).

use std::borrow::Cow;
use std::fmt;
use std::net::IpAddr;

use afd_core::net::is_blocked;
use afd_wire::policy::{ExecutionPolicy, HttpOriginPolicy, Mintable, NetworkPolicy};
use afr_secrets::{FIELD_HOST, StaticSecrets};
use reqwest::header::{AUTHORIZATION, HOST};
use reqwest::{Method, Url};
use url::Host;

use crate::error::{Error, Result, raise};
use crate::origin;
use crate::placeholder::{self, SecretRef};

/// Every method a tool may send.
const METHODS: [Method; 7] = [
    Method::GET,
    Method::HEAD,
    Method::POST,
    Method::PUT,
    Method::PATCH,
    Method::DELETE,
    Method::OPTIONS,
];

/// The methods `read_only` admits everywhere.
const READS: [Method; 2] = [Method::GET, Method::HEAD];

/// The only scheme sent.
const HTTPS: &str = "https";

/// The field a minted credential's token is named by.
pub(crate) const FIELD_TOKEN: &str = "token";

/// What a placeholder in the URL is called when it is refused.
const IN_URL: &str = "a placeholder in the URL, or credentials in its userinfo";
/// What a placeholder in the body is called when it is refused.
const IN_BODY: &str = "a placeholder in the body";

/// Where a request's placeholders may stand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// As the `Authorization` header's value, and as the URL's whole host.
    Authorization,
    /// Nowhere: the request carries no credential the model named.
    Nowhere,
}

/// What a tool asks to send, as the model wrote it.
pub struct Draft {
    /// The method's name.
    pub method: String,
    /// The URL, its host possibly `${secrets.NAME.host}`.
    pub url: String,
    /// Headers, in order.
    pub headers: Vec<(String, String)>,
    /// The body.
    pub body: Option<String>,
    /// Where placeholders may stand.
    pub placement: Placement,
}

impl fmt::Debug for Draft {
    /// The method and the placement only: a tool may build a body holding a
    /// credential it read for itself.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Draft")
            .field("method", &self.method)
            .field("placement", &self.placement)
            .finish_non_exhaustive()
    }
}

/// A request the policy admits, its placeholders not yet resolved.
pub(crate) struct Admitted {
    pub(crate) method: Method,
    pub(crate) url: Url,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Option<String>,
}

/// One lease's network policy, read where each request consults it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Admission<'p> {
    network: &'p NetworkPolicy<'p>,
    origins: &'p [HttpOriginPolicy<'p>],
    mintable: &'p [Mintable<'p>],
    statics: StaticSecrets<'p>,
}

impl<'p> Admission<'p> {
    /// The admission `policy` describes.
    pub(crate) fn new(policy: &'p ExecutionPolicy<'p>) -> Self {
        Self {
            network: &policy.network_policy,
            origins: &policy.http_origin_policies,
            mintable: &policy.mintable,
            statics: StaticSecrets::new(policy.secrets_map.as_ref()),
        }
    }

    /// The lease's static credentials.
    pub(crate) const fn statics(self) -> StaticSecrets<'p> {
        self.statics
    }

    /// Whether `name` is minted rather than read from `secrets_map`.
    pub(crate) fn mints(self, name: &str) -> Option<&'p Mintable<'p>> {
        self.mintable.iter().find(|mintable| mintable.name == name)
    }

    /// `draft`, admitted, or the first rule it breaks.
    pub(crate) fn admit(self, draft: Draft) -> Result<Admitted> {
        let method = sendable(&draft.method)?;
        let url = self.locate(&draft.url, draft.placement)?;
        let host = url.host_str().unwrap_or_default();
        let credentials = placed(&draft)?;
        self.listed(host)?;
        reachable(&url, host)?;
        credentials
            .into_iter()
            .try_for_each(|secret| self.bound(secret, host))?;
        let matched = self.origin_admits(&method, &url, draft.body.as_deref())?;
        self.read_only_admits(&method, &url, matched)?;
        Ok(Admitted {
            method,
            url,
            headers: draft.headers,
            body: draft.body,
        })
    }

    /// The URL `text` names, its host placeholder put in place.
    fn locate(self, text: &str, placement: Placement) -> Result<Url> {
        let named = match placement {
            Placement::Authorization => placeholder::host_url(text),
            Placement::Nowhere => None,
        };
        let host = named
            .map(|(name, _rest)| {
                self.statics
                    .host(name)
                    .ok_or_else(|| Error::secret_not_found(name, FIELD_HOST))
            })
            .transpose()?;
        let resolved = match (named, host) {
            (Some((_name, rest)), Some(host)) => Cow::Owned(format!("{HTTPS}://{host}{rest}")),
            _ => Cow::Borrowed(text),
        };
        if placeholder::mentions(&resolved) {
            return Err(raise::placement_not_allowed(IN_URL));
        }
        let url = Url::parse(&resolved).map_err(raise::invalid_url)?;
        let host_kept = host.is_none_or(|host| {
            url.host_str()
                .is_some_and(|parsed| parsed.eq_ignore_ascii_case(host))
        });
        if url.scheme() != HTTPS {
            Err(raise::https_required())
        } else if !url.username().is_empty() || url.password().is_some() || !host_kept {
            Err(raise::placement_not_allowed(IN_URL))
        } else {
            Ok(url)
        }
    }

    fn listed(self, host: &str) -> Result<()> {
        self.network
            .allow
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(host))
            .then_some(())
            .ok_or_else(|| raise::host_not_allowed(host))
    }

    /// Whether `secret` may be sent to `host`: a minted one where the host's
    /// origin policy names it, a static one only to its own `host` field.
    fn bound(self, secret: SecretRef<'_>, host: &str) -> Result<()> {
        let sent_here = match self.mints(secret.name) {
            Some(_minted) if secret.field != FIELD_TOKEN => {
                return Err(Error::secret_not_found(secret.name, secret.field));
            }
            Some(_minted) => self.origin(host).is_some_and(|origin| {
                origin
                    .credential_names
                    .iter()
                    .any(|name| name == secret.name)
            }),
            None => {
                self.statics
                    .field(secret.name, secret.field)
                    .ok_or_else(|| Error::secret_not_found(secret.name, secret.field))?;
                self.statics
                    .host(secret.name)
                    .is_some_and(|bound| bound.eq_ignore_ascii_case(host))
            }
        };
        sent_here
            .then_some(())
            .ok_or_else(|| raise::credential_host_not_allowed(secret.name, host))
    }

    fn origin(self, host: &str) -> Option<&'p HttpOriginPolicy<'p>> {
        self.origins
            .iter()
            .find(|origin| origin.host.eq_ignore_ascii_case(host))
    }

    /// Whether a rule admitted the request: `false` for a host with no rules,
    /// a refusal for a host whose rules admit nothing of this shape.
    fn origin_admits(self, method: &Method, url: &Url, body: Option<&str>) -> Result<bool> {
        let host = url.host_str().unwrap_or_default();
        match self.origin(host) {
            None => Ok(false),
            Some(origin) if origin::admits(origin, method, url, body) => Ok(true),
            Some(_refusing) => Err(raise::request_policy_not_allowed(
                host,
                method.as_str(),
                url.path(),
            )),
        }
    }

    /// Under `read_only`: reads anywhere, and a `POST` only where a rule
    /// admitted it or under a listed `read_post_paths` prefix.
    fn read_only_admits(self, method: &Method, url: &Url, matched: bool) -> Result<()> {
        let admitted = !self.network.read_only
            || READS.contains(method)
            || (*method == Method::POST && (matched || self.read_post(url)));
        admitted
            .then_some(())
            .ok_or_else(|| raise::method_not_allowed(method.as_str()))
    }

    /// Whether `url` is a listed query path, ending there or at its query.
    fn read_post(self, url: &Url) -> bool {
        self.network.read_post_paths.iter().any(|prefix| {
            url.as_str()
                .strip_prefix(prefix.as_ref())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('?'))
        })
    }
}

/// The method `text` names, when a tool may send it.
fn sendable(text: &str) -> Result<Method> {
    Method::from_bytes(text.to_ascii_uppercase().as_bytes())
        .ok()
        .filter(|method| METHODS.contains(method))
        .ok_or_else(|| raise::method_not_allowed(text))
}

/// The placeholders `draft`'s headers carry, each where it may stand.
fn placed(draft: &Draft) -> Result<Vec<SecretRef<'_>>> {
    if draft.body.as_deref().is_some_and(placeholder::mentions) {
        return Err(raise::placement_not_allowed(IN_BODY));
    }
    let carried = draft
        .headers
        .iter()
        .map(|(name, value)| header_secrets(name, value, draft.placement))
        .collect::<Result<Vec<_>>>()?;
    Ok(carried.into_iter().flatten().collect())
}

fn header_secrets<'d>(
    name: &str,
    value: &'d str,
    placement: Placement,
) -> Result<Vec<SecretRef<'d>>> {
    let carries =
        name.eq_ignore_ascii_case(AUTHORIZATION.as_str()) && placement == Placement::Authorization;
    let reserved = name.eq_ignore_ascii_case(HOST.as_str()) || placeholder::mentions(name);
    match placeholder::parse(value) {
        Some(found) if !reserved && (found.is_empty() || carries) => Ok(found),
        _misplaced => Err(raise::placement_not_allowed(&format!(
            "the {name} header as written"
        ))),
    }
}

/// Whether an address literal stays outside the private ranges; a name is
/// checked by the resolver, after it resolves.
fn reachable(url: &Url, host: &str) -> Result<()> {
    let address: Option<IpAddr> = match url.host() {
        Some(Host::Ipv4(address)) => Some(address.into()),
        Some(Host::Ipv6(address)) => Some(address.into()),
        Some(Host::Domain(_)) | None => None,
    };
    if address.is_some_and(is_blocked) {
        Err(raise::address_not_allowed(host))
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "admission/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "admission/credential_tests.rs"]
mod credential_tests;
