//! What a suite needs to prove a provider call, in this crate and in the
//! crates that embed it (M-TEST-UTIL).
//!
//! An observer it reads back, a scripted model and the measure every
//! scripted model answers with, and a TLS listener a fake provider serves on.

pub mod observed;
#[cfg(feature = "test-util")]
pub mod scripted;
#[cfg(feature = "test-util")]
pub mod tls;

pub use self::observed::{Observed, Scoped, Tally, scoped};

use guffbolt::Request;

/// The input tokens `request` costs in the canonical chat shape: what a
/// scripted model measures with, where a connected provider measures on its
/// own wire.
///
/// # Errors
/// The request cannot be encoded.
pub fn measure(request: Request<'_>) -> crate::Result<u64> {
    let body = crate::request::request(af_wire::inference::InferenceApi::Chat, &request)?;
    let text = serde_json::to_string(&body)?;
    let tokens = af_inference::input_tokens(request.model, &text)
        .saturating_add(body.chat_history.len().saturating_mul(32));
    Ok(u64::try_from(tokens).unwrap_or(u64::MAX))
}
