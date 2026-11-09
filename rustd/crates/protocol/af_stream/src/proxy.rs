//! Explicit HTTP CONNECT routing preserves the daemon's end-to-end TLS identity.

use std::fmt;

use af_core::env::{EnvSource, optional};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use http::{
    Request, Uri,
    header::{HOST, HeaderValue, PROXY_AUTHORIZATION},
};
use hyper_util::{client::proxy::matcher::Matcher, rt::TokioIo};
use tokio::net::TcpStream;
use tokio_util::task::AbortOnDropHandle;
use zeroize::Zeroizing;

use crate::{Endpoint, Outgoing, Result, error::ErrorKind};

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Proxy {
    uri: Uri,
    authorization: Option<Zeroizing<String>>,
}

impl fmt::Debug for Proxy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Proxy")
            .field("uri", &self.uri)
            .finish_non_exhaustive()
    }
}

pub(crate) fn resolve(
    env: &(impl EnvSource + ?Sized),
    endpoint: &Endpoint,
) -> Result<Option<Proxy>> {
    let raw = optional(env, "https_proxy")
        .or_else(|| optional(env, "HTTPS_PROXY"))
        .or_else(|| optional(env, "all_proxy"))
        .or_else(|| optional(env, "ALL_PROXY"));
    let Some(raw) = raw.map(Zeroizing::new) else {
        return Ok(None);
    };
    // Validate first: Matcher ignores malformed proxy URLs, which must never cause a direct dial.
    let proxy = Proxy::try_from(&**raw)?;
    let matcher = Matcher::builder().https(proxy.uri.to_string());
    let bypass = optional(env, "no_proxy").or_else(|| optional(env, "NO_PROXY"));
    let matcher = match bypass {
        Some(bypass) => {
            let checked = garde::Unvalidated::new(Address { value: &bypass }).validate()?;
            matcher.no(checked.value).build()
        }
        None => matcher.build(),
    };
    Ok(matcher.intercept(endpoint.uri()).map(|_| proxy))
}

impl TryFrom<&str> for Proxy {
    type Error = crate::Error;
    fn try_from(raw: &str) -> Result<Self> {
        let checked = garde::Unvalidated::new(Address { value: raw }).validate()?;
        let url = url::Url::parse(checked.value).map_err(|source| input("HTTPS_PROXY", source))?;
        if url.scheme() != "http"
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(ErrorKind::ProxyPolicy {
                setting: "HTTPS_PROXY",
            }
            .into());
        }
        // Only the authority without user information reaches Matcher, Debug or the socket.
        let authority = &url[url::Position::BeforeHost..url::Position::AfterPort];
        let uri = format!("http://{authority}/").parse()?;
        Ok(Self {
            uri,
            authorization: authorization(&url)?,
        })
    }
}

fn authorization(url: &url::Url) -> Result<Option<Zeroizing<String>>> {
    if url.username().is_empty() && url.password().is_none() {
        return Ok(None);
    }
    let mut credentials =
        Zeroizing::new(percent_encoding::percent_decode_str(url.username()).collect::<Vec<_>>());
    let password = Zeroizing::new(
        percent_encoding::percent_decode_str(url.password().unwrap_or_default())
            .collect::<Vec<_>>(),
    );
    credentials.push(b':');
    credentials.extend_from_slice(&password);
    let encoded = Zeroizing::new(STANDARD.encode(&credentials));
    Ok(Some(Zeroizing::new(format!("Basic {}", &*encoded))))
}

#[derive(garde::Validate)]
struct Address<'a> {
    #[garde(length(bytes, min = 1, max = 4096))]
    value: &'a str,
}

fn input(
    setting: &'static str,
    source: impl std::error::Error + Send + Sync + 'static,
) -> crate::Error {
    ErrorKind::ProxyInput {
        setting,
        source: Box::new(source),
    }
    .into()
}

/// Header construction is a synchronous conversion; socket I/O stays in `socket`.
impl TryFrom<&Endpoint> for Request<Outgoing> {
    type Error = crate::Error;
    fn try_from(endpoint: &Endpoint) -> Result<Self> {
        let authority = if endpoint.host().contains(':') {
            format!("[{}]:{}", endpoint.host(), endpoint.port())
        } else {
            format!("{}:{}", endpoint.host(), endpoint.port())
        };
        let mut request = Request::connect(authority.as_str()).header(HOST, authority.as_str());
        if let Some(authorization) = endpoint
            .proxy
            .as_ref()
            .and_then(|proxy| proxy.authorization.as_ref())
        {
            let mut value = HeaderValue::from_str(authorization)?;
            value.set_sensitive(true);
            request = request.header(PROXY_AUTHORIZATION, value);
        }
        Ok(request.body(Outgoing::new(http_body_util::Empty::<bytes::Bytes>::new()))?)
    }
}

pub(crate) async fn socket(endpoint: &Endpoint) -> Result<TcpStream> {
    let Some(proxy) = &endpoint.proxy else {
        return Ok(match endpoint.dial {
            Some(address) => TcpStream::connect(address).await?,
            None => TcpStream::connect((endpoint.host(), endpoint.port())).await?,
        });
    };
    let host = proxy
        .uri
        .host()
        .ok_or(ErrorKind::ProxyPolicy {
            setting: "HTTPS_PROXY",
        })?
        .trim_matches(['[', ']']);
    let socket = TcpStream::connect((host, proxy.uri.port_u16().unwrap_or(80))).await?;
    socket.set_nodelay(true)?;
    let (mut sender, connection) = hyper::client::conn::http1::Builder::new()
        .max_buf_size(crate::HEADER_BYTES as usize)
        .handshake(TokioIo::new(socket))
        .await?;
    let driver = AbortOnDropHandle::new(tokio::spawn(connection.with_upgrades()));
    let mut response = sender.send_request(Request::try_from(endpoint)?).await?;
    if response.status() != http::StatusCode::OK {
        return Err(ErrorKind::ProxyRefused {
            status: response.status(),
        }
        .into());
    }
    let upgraded = hyper::upgrade::on(&mut response).await?;
    driver.await??;
    let parts = upgraded
        .downcast::<TokioIo<TcpStream>>()
        .map_err(|_| ErrorKind::Protocol)?;
    // No TLS ClientHello has been sent, so preloaded server bytes are a protocol violation.
    if !parts.read_buf.is_empty() {
        return Err(ErrorKind::Protocol.into());
    }
    Ok(parts.io.into_inner())
}

#[cfg(test)]
mod tests;
