//! The outbound default of a standalone harness: HTTPS only, to the hosts an
//! allowlist names, the way nullclaw's `http_request` admits a request
//! (`nullclaw/src/tools/http_request.zig`), and never to a private address.
//!
//! A host is allowed when it is a listed host or one of its subdomains. A
//! host that is a private address, or a name that resolves to one, is
//! refused whatever the allowlist says, at every redirect's hop too
//! ([`super::guard`]). A credential is written as `${secrets.NAME.FIELD}` in
//! the `Authorization` header of a request that may carry one, and put in
//! place as it is sent; every secret's value is masked out of what comes back.

use std::collections::BTreeMap;
use std::fmt;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use reqwest::dns::Resolve;
use reqwest::header::{CONTENT_TYPE, LOCATION};
use reqwest::{Client, Method, Url};

use super::guard::{self, SystemLookup};
use super::{
    Admitted, Outbound, OutboundFuture, Placement, RESPONSE_MAX_BYTES, Redirects, Refused, Request,
    Response, client,
};
use crate::tools::code;
/// The most redirects a request that follows them follows.
const REDIRECTS_MAX: u8 = 3;
/// How long one request may take, end to end.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// The methods a request may use.
const METHODS: [Method; 7] = [
    Method::GET,
    Method::HEAD,
    Method::POST,
    Method::PUT,
    Method::PATCH,
    Method::DELETE,
    Method::OPTIONS,
];
/// The scheme every request uses.
const HTTPS: &str = "https";
/// The header a credential may stand in.
const AUTHORIZATION: &str = "authorization";
/// What a masked secret reads as.
const MASKED: &str = "[masked]";

/// The hosts a request may go to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Allowlist {
    /// Any host.
    Any,
    /// These hosts, and their subdomains.
    Hosts(Vec<String>),
}

impl Allowlist {
    /// Whether `host` is allowed.
    #[must_use]
    pub fn allows(&self, host: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Hosts(hosts) => {
                let host = host.to_ascii_lowercase();
                hosts.iter().any(|allowed| {
                    let allowed = allowed.to_ascii_lowercase();
                    host == allowed || host.ends_with(&format!(".{allowed}"))
                })
            }
        }
    }
}

/// HTTPS requests from this host, to the hosts an allowlist names.
#[derive(Clone)]
pub struct LocalOutbound {
    client: Client,
    /// How a name is looked up, by this client and by an MCP client it
    /// admits ([`Outbound::lookup`]).
    lookup: Arc<dyn Resolve>,
    allowlist: Allowlist,
    /// Each secret's fields, by the secret's name.
    secrets: BTreeMap<String, BTreeMap<String, String>>,
}

impl fmt::Debug for LocalOutbound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalOutbound")
            .field("allowlist", &self.allowlist)
            .finish_non_exhaustive()
    }
}

impl LocalOutbound {
    /// Requests to the hosts `allowlist` names, each name looked up by this
    /// host's resolver.
    ///
    /// # Errors
    /// The TLS backend refused the client's configuration.
    pub fn new(allowlist: Allowlist) -> reqwest::Result<Self> {
        Self::resolving(allowlist, Arc::new(SystemLookup))
    }

    /// The same, each name looked up through `lookup`, such as a resolver of
    /// the embedder's own. A name any of whose addresses is private is
    /// refused whatever `lookup` answers.
    ///
    /// # Errors
    /// As [`LocalOutbound::new`].
    pub fn resolving(allowlist: Allowlist, lookup: Arc<dyn Resolve>) -> reqwest::Result<Self> {
        let client = client(REQUEST_TIMEOUT, Arc::clone(&lookup)).build()?;
        Ok(Self {
            client,
            lookup,
            allowlist,
            secrets: BTreeMap::new(),
        })
    }

    /// The same, holding `value` as `field` of the secret `name`.
    #[must_use]
    pub fn with_secret(
        mut self,
        name: impl Into<String>,
        field: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.secrets
            .entry(name.into())
            .or_default()
            .insert(field.into(), value.into());
        self
    }

    /// `url`, or the refusal: it parses, is HTTPS, names no private address
    /// and names an allowed host. A name is judged by the client's resolver
    /// as it connects.
    fn admitted(&self, url: &str) -> Result<Url, Refused> {
        let parsed = Url::parse(url).map_err(|failure| {
            Refused::new(
                code::INVALID_ARGUMENTS,
                format!("{url} is not a URL: {failure}"),
            )
        })?;
        if parsed.scheme() != HTTPS {
            return Err(Refused::new(
                code::HTTPS_REQUIRED,
                format!("{url} is not https"),
            ));
        }
        guard::literal_admitted(&parsed)?;
        let host = parsed.host_str().unwrap_or_default();
        if !self.allowlist.allows(host) {
            return Err(Refused::new(
                code::HOST_NOT_ALLOWED,
                format!("{host} is not an allowed host"),
            ));
        }
        Ok(parsed)
    }

    /// `value` with every `${secrets.NAME.FIELD}` this hook holds put in
    /// place.
    fn placed(&self, value: &str) -> String {
        let mut placed = value.to_owned();
        for (name, fields) in &self.secrets {
            for (field, secret) in fields {
                placed = placed.replace(&format!("${{secrets.{name}.{field}}}"), secret);
            }
        }
        placed
    }

    /// `text` with every secret's value masked.
    fn masked(&self, text: String) -> String {
        self.secrets
            .values()
            .flat_map(BTreeMap::values)
            .filter(|secret| !secret.is_empty())
            .fold(text, |text, secret| text.replace(secret.as_str(), MASKED))
    }

    async fn sent(&self, request: Request) -> Result<Response, Refused> {
        let method = METHODS
            .iter()
            .find(|method| method.as_str().eq_ignore_ascii_case(&request.method))
            .cloned()
            .ok_or_else(|| {
                Refused::new(
                    code::METHOD_NOT_ALLOWED,
                    format!("{} is not a method this tool sends", request.method),
                )
            })?;
        let mut url = self.admitted(&request.url)?;
        let mut headers = request.headers;
        let mut followed = 0;
        loop {
            let mut outbound = self.client.request(method.clone(), url.clone());
            for (name, value) in &headers {
                let value = if request.placement == Placement::Authorization
                    && name.eq_ignore_ascii_case(AUTHORIZATION)
                {
                    self.placed(value)
                } else {
                    value.clone()
                };
                outbound = outbound.header(name, value);
            }
            if let Some(body) = &request.body {
                outbound = outbound.body(body.clone());
            }
            let response = outbound.send().await.map_err(unreached)?;
            let status = response.status();
            let location = response
                .headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let follow = request.redirects == Redirects::Follow
                && status.is_redirection()
                && followed < REDIRECTS_MAX;
            if let (true, Some(target)) = (follow, location.as_deref()) {
                let next = url.join(target).map_err(|failure| {
                    Refused::new(code::INVALID_ARGUMENTS, failure.to_string())
                })?;
                let next = self.admitted(next.as_str())?;
                // A request's own headers never follow it to another origin.
                if next.origin() != url.origin() {
                    headers.clear();
                }
                url = next;
                followed += 1;
                continue;
            }
            return Ok(self.read(response, location, followed).await);
        }
    }

    /// The answer, at most [`RESPONSE_MAX_BYTES`] of its body, masked.
    async fn read(
        &self,
        mut response: reqwest::Response,
        location: Option<String>,
        followed_redirects: u8,
    ) -> Response {
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let mut bytes = Vec::new();
        let mut truncated = false;
        while let Ok(Some(chunk)) = response.chunk().await {
            let room = RESPONSE_MAX_BYTES.saturating_sub(bytes.len());
            if chunk.len() > room {
                bytes.extend_from_slice(chunk.get(..room).unwrap_or_default());
                truncated = true;
                break;
            }
            bytes.extend_from_slice(&chunk);
        }
        let body = String::from_utf8_lossy(&bytes);
        let lossy = matches!(body, std::borrow::Cow::Owned(_));
        Response {
            status,
            location,
            content_type: content_type.map(|value| self.masked(value)),
            body: self.masked(body.into_owned()),
            truncated,
            lossy,
            followed_redirects,
        }
    }
}

/// Why a request that was sent got no answer: the guard refused its name,
/// or the network failed it, in words that carry no URL.
fn unreached(failure: reqwest::Error) -> Refused {
    guard::denied(&failure).cloned().map_or_else(
        || {
            Refused::new(
                code::UPSTREAM_UNREACHABLE,
                failure.without_url().to_string(),
            )
        },
        Refused::from,
    )
}

impl Outbound for LocalOutbound {
    fn send<'a>(&'a self, _tool: &'a str, request: Request) -> OutboundFuture<'a, Response> {
        Box::pin(self.sent(request))
    }

    fn mask(&self, text: String) -> Pin<Box<dyn Future<Output = String> + Send + '_>> {
        Box::pin(std::future::ready(self.masked(text)))
    }

    fn secret<'a>(
        &'a self,
        _tool: &'a str,
        name: &'a str,
        field: &'a str,
    ) -> OutboundFuture<'a, String> {
        let found = self
            .secrets
            .get(name)
            .and_then(|fields| fields.get(field))
            .cloned()
            .ok_or_else(|| {
                Refused::new(
                    code::SECRET_NOT_FOUND,
                    format!("no secret {name} with a field {field}"),
                )
            });
        Box::pin(std::future::ready(found))
    }

    fn connect<'a>(&'a self, _tool: &'a str, url: &'a str) -> OutboundFuture<'a, Admitted> {
        let admitted = self.admitted(url).map(|url| Admitted {
            url: url.into(),
            headers: Vec::new(),
        });
        Box::pin(std::future::ready(admitted))
    }

    fn lookup(&self) -> Arc<dyn Resolve> {
        Arc::clone(&self.lookup)
    }
}

#[cfg(test)]
#[path = "local_tests/mod.rs"]
mod tests;
