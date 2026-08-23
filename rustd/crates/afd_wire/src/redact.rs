//! Hand-written `Debug` for every wire type that carries a secret.
//!
//! One module rather than an impl beside each type, so "are secrets redacted?"
//! is a question with ONE place to look. A derived `Debug` on any of these would
//! put a provider key, a tenant's whole secret map, a minted credential, or a
//! runner's bearer token into the first log line that formats a lease — and the
//! Zig source says of the mint reply, verbatim, that it "is secret (VLT) — never
//! logged, never echoed into a frame".
//!
//! Redaction is on `Debug` ONLY. `Serialize` still emits the real value, because
//! these types exist to put it on the wire; the round-trip fixtures would fail
//! immediately otherwise. `tests/redaction.rs` proves both halves.
//!
//! Adding a secret-bearing field to a type below means extending its impl here.
//! `missing_debug_implementations` is denied workspace-wide, so a type cannot
//! simply drop `Debug` to dodge the question.

use std::fmt::{self, Debug, Formatter};

use crate::credentials::MintCredentialResponse;
use crate::policy::ExecutionPolicy;
use crate::runner::RegisterResponse;

/// What a redacted field renders as. One spelling, so a log grep for leaked
/// credentials has exactly one negative to look for.
const REDACTED: &str = "<redacted>";

/// Renders a secret as its length only.
///
/// The length is safe and occasionally decisive — "the key is 0 bytes" is the
/// difference between a misconfigured credential and a rejected one, and reading
/// it off a log beats reproducing the request.
fn redacted(secret: &str) -> String {
    format!("{REDACTED} ({} bytes)", secret.len())
}

impl Debug for ExecutionPolicy<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExecutionPolicy")
            .field("network_policy", &self.network_policy)
            .field("tools", &self.tools)
            // The whole map is the tenant's secrets; even its KEYS name the
            // integrations a tenant has connected, so only the count is shown.
            .field(
                "secrets_map",
                &self.secrets_map.as_ref().map(|value| match value {
                    serde_json::Value::Object(map) => {
                        format!("{REDACTED} ({} entries)", map.len())
                    }
                    _ => REDACTED.to_owned(),
                }),
            )
            .field("mintable", &self.mintable)
            .field("provider", &self.provider)
            .field("api_key", &redacted(&self.api_key))
            .field("inference_host", &self.inference_host)
            .field("base_url", &self.base_url)
            .field("repository_binding", &self.repository_binding)
            .field("http_origin_policies", &self.http_origin_policies)
            .field("context", &self.context)
            .finish()
    }
}

impl Debug for RegisterResponse<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegisterResponse")
            .field("runner_id", &self.runner_id)
            // Revealed once at enrollment and never re-readable; the daemon
            // stores only its hash, so a log line is the one place it could
            // survive in plaintext.
            .field("runner_token", &redacted(&self.runner_token))
            .field("assigned_policy", &self.assigned_policy)
            .finish()
    }
}

impl Debug for MintCredentialResponse<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("MintCredentialResponse")
            .field("token", &redacted(&self.token))
            .field("expires_at_ms", &self.expires_at_ms)
            .finish()
    }
}
