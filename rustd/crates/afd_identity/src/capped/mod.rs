//! Reading a response body this daemon does not trust the size of.
//!
//! The key-set fetch and the claim read both stream a provider's answer into
//! memory under a cap. They differ only in the cap, the first allocation, the
//! event a refusal logs and the error it answers with, so the loop lives here
//! once and each caller keeps those four.
//!
//! Streamed rather than `bytes()`: a `Content-Length` is the server's claim
//! about itself, and this daemon bounds what it actually reads. No
//! content-encoding is ever negotiated (see the `reqwest` note in the workspace
//! manifest), so these bytes are the decoded bytes and a cap bounds what it says
//! it bounds.

/// Why a capped read stopped short of a body.
///
/// Two cases because a caller logs them differently: an oversized answer is
/// the server misbehaving and earns a warning naming the cap, while a broken
/// transport has already been seen by whatever logged the request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CapBreak {
    /// The connection failed before the body ended.
    Transport,
    /// The body grew past the cap; nothing past it was kept.
    TooLarge,
}

/// Reads `response` to its end, refusing once the body would exceed `cap`.
///
/// Refuses rather than truncates: a key set or a user document cut at an
/// arbitrary byte is not a smaller answer, it is a wrong one. `initial` is the
/// first allocation only, sized for the common answer.
pub(crate) async fn read_capped(
    mut response: reqwest::Response,
    cap: usize,
    initial: usize,
) -> Result<Vec<u8>, CapBreak> {
    let mut body = Vec::with_capacity(initial);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_transport| CapBreak::Transport)?
    {
        if body.len().saturating_add(chunk.len()) > cap {
            return Err(CapBreak::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
mod tests;
