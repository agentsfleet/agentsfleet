//! A Gemini API key travels in a header, never in the request line.
//!
//! rig-core 0.44's `generateContent` wire appends the key to the URL as
//! `?key=…` (`providers/gemini/mod.rs:91`). A URL is the part of a request
//! proxies, access logs and error pages keep, so every Gemini request this
//! crate builds passes through [`HeaderKey`] first: it lifts the `key`
//! parameter out of the URI and sends it as `x-goog-api-key`, the header
//! Google accepts on the same endpoints and rig itself uses for its
//! Interactions family (`mod.rs:102`). The wrapper is applied by
//! [`crate::Configured::with_transport`] whenever the saved API is
//! [`afd_wire::inference::InferenceApi::Gemini`], so no caller can hand rig a
//! transport that sees the key in the URL.

use http::Uri;
use http::header::{HeaderName, HeaderValue};
use http::uri::PathAndQuery;
use rig_core::ProviderError;
use rig_core::driver::{Exchange, Opening, Transport};
use rig_core::providers::gemini::completion::GenerateContent;
use rig_core::wire::{Body, Encoded, WireFrame};
use url::form_urlencoded;

use crate::Result;

/// The query parameter rig's `generateContent` wire carries the key in.
const KEY_PARAMETER: &str = "key";

/// The header Google authenticates a Gemini API request by.
pub(crate) const KEY_HEADER: HeaderName = HeaderName::from_static("x-goog-api-key");

/// Wraps the transport a Gemini model sends through.
#[derive(Clone)]
pub(crate) struct HeaderKey<T>(pub(crate) T);

/// Reproduces rig's raw query interpolation without allocating a completion body.
pub(crate) fn key_probe(key: &str) -> Result<Uri> {
    Ok(format!("/?{KEY_PARAMETER}={key}").parse()?)
}

impl<T: Transport<GenerateContent>> Transport<GenerateContent> for HeaderKey<T> {
    fn send(&self, mut payload: Encoded, exchange: Exchange) -> Opening<WireFrame> {
        match move_key(&mut payload.request) {
            Ok(()) => self.0.send(payload, exchange),
            Err(refusal) => Opening::failed(ProviderError::from(refusal)),
        }
    }
}

/// Lifts every `key` parameter out of `request`'s URI into [`KEY_HEADER`],
/// marked sensitive so a header dump redacts it. A URI without one is left
/// as it is.
///
/// # Errors
/// A key that is not a valid header value, or a rebuilt URI that does not
/// parse; the request is not sent either way.
pub(crate) fn move_key(request: &mut http::Request<Body>) -> Result<()> {
    let Some(query) = request.uri().query() else {
        return Ok(());
    };
    let (keys, kept): (Vec<_>, Vec<_>) = form_urlencoded::parse(query.as_bytes())
        .partition(|(name, _)| name.as_ref() == KEY_PARAMETER);
    let Some((_, key)) = keys.into_iter().last() else {
        return Ok(());
    };
    let mut value = HeaderValue::from_str(&key)?;
    value.set_sensitive(true);
    let path = request.uri().path();
    let rebuilt = if kept.is_empty() {
        path.to_owned()
    } else {
        let query = form_urlencoded::Serializer::new(String::new())
            .extend_pairs(kept)
            .finish();
        format!("{path}?{query}")
    };
    let mut parts = request.uri().clone().into_parts();
    parts.path_and_query = Some(PathAndQuery::try_from(rebuilt)?);
    *request.uri_mut() = Uri::from_parts(parts)?;
    request.headers_mut().insert(KEY_HEADER, value);
    Ok(())
}

#[cfg(test)]
mod tests;
