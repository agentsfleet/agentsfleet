//! Validate the final credential representation before a caller records dispatch intent.

use crate::{Result, header_key};
use afd_wire::inference::InferenceApi;
use http::HeaderValue;
use rig_core::wire::Body;

/// Checks header syntax and Gemini's query-to-header transformation without sending.
///
/// # Errors
/// Refuses malformed credentials without including their bytes in the error.
pub fn validate_key(api: InferenceApi, key: &str) -> Result<()> {
    HeaderValue::from_str(key)?;
    if api == InferenceApi::Gemini {
        let mut request = http::Request::new(Body::empty());
        *request.uri_mut() = header_key::key_probe(key)?;
        header_key::move_key(&mut request)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
