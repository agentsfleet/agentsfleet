//! Writing a new account's tenant back to the identity provider.
//!
//! The port of `auth/clerk_backend.zig`, and the write side of
//! [`crate::provider`]'s read. Both carry the same credential to the same API
//! base; they are separate modules because the Zig kept them separate, for the
//! reason that survives the port: a claim read decides whether a request
//! proceeds, and this decides nothing at all.
//!
//! # Why a failure here is not a failed signup
//!
//! Signup is two writes — a tenant row, then this. The row is already
//! committed when this runs, and the account exists whether or not the
//! provider hears about it. So the caller logs the outcome and answers 200
//! regardless: a Clerk outage that turned signup into a 500 would refuse
//! accounts it had already created, and the provider would retry the delivery
//! into a duplicate it cannot make.
//!
//! What that costs is real and is the reason [`MetadataUnwritten`] separates
//! its three cases: until the write lands the person's next session token
//! carries no `tenant_id`, so every call they make is refused for want of a
//! tenant. An operator repairs it from the Clerk dashboard, and the log line
//! is how they learn they have to.

use afd_auth::principal::Subject;

use crate::error::MetadataUnwritten;
use crate::provider::{JSON_MEDIA_TYPE, ProviderSecret};

/// The merge payload, as the provider reads it.
///
/// A serialised struct rather than a hand-built JSON string: the field names
/// ARE the wire names, so the shape cannot drift from the keys the provider
/// merges on. The provider deep-merges, so sibling keys this daemon knows
/// nothing about — anything an operator or a future dashboard set — survive,
/// and no read-then-write is needed to preserve them.
#[derive(Debug, serde::Serialize)]
struct MetadataMerge<'a> {
    public_metadata: PublicMetadata<'a>,
}

/// The two keys this daemon owns inside that object.
#[derive(Debug, serde::Serialize)]
struct PublicMetadata<'a> {
    tenant_id: &'a str,
    scopes: &'a str,
}

/// Writes account metadata to the identity provider's backend API.
#[derive(Debug)]
pub struct ProviderMetadata {
    client: reqwest::Client,
    api_base: Box<str>,
    secret: ProviderSecret,
}

impl ProviderMetadata {
    /// Builds a metadata writer against `api_base`.
    ///
    /// # Errors
    /// [`MetadataUnwritten::Unreachable`] when a client cannot be constructed.
    pub fn new(
        api_base: impl Into<Box<str>>,
        secret: ProviderSecret,
        timeout: std::time::Duration,
    ) -> Result<Self, MetadataUnwritten> {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            // The provider is reached directly, for the reason the read gives:
            // an environment proxy silently rerouting the call that decides
            // what a person may do is not a convenience worth having.
            .no_proxy()
            .build()
            .map_err(|_unbuildable| MetadataUnwritten::Unreachable)?;
        Ok(Self {
            client,
            api_base: api_base.into(),
            secret,
        })
    }

    /// Merges a new account's tenant and owner grant into the subject's
    /// `public_metadata`.
    ///
    /// # Errors
    /// [`MetadataUnwritten`] — whichever of the three the attempt produced.
    /// Every one of them is the caller's to log and swallow, never to answer
    /// the delivery with.
    pub async fn write_signup(
        &self,
        subject: &Subject,
        tenant_id: &str,
        scopes: &str,
    ) -> Result<(), MetadataUnwritten> {
        let url = format!("{}/users/{}/metadata", self.api_base, subject.as_str());
        // Serialised here rather than through reqwest's `json` helper: this
        // workspace builds reqwest without that feature, and the read next
        // door already spells its own media-type header for the same reason.
        // Two `&str` fields cannot fail to serialise; the arm exists because
        // `expect` in a request path is a panic waiting for a refactor.
        let payload = serde_json::to_vec(&MetadataMerge {
            public_metadata: PublicMetadata { tenant_id, scopes },
        })
        .map_err(|_unserialisable| MetadataUnwritten::Unreachable)?;

        let response = self
            .client
            .patch(&url)
            .bearer_auth(self.secret.expose())
            .header(reqwest::header::CONTENT_TYPE, JSON_MEDIA_TYPE)
            .body(payload)
            .send()
            .await
            .inspect_err(|failure| {
                // Hoisted out of the macro for the reason the read hoists it:
                // `tracing`'s `log` feature is on across this workspace, so a
                // call inside an event field compiles twice.
                //
                // The URL is NOT logged: it carries the subject, and a line
                // naming whose account was being repaired is a record of who
                // signed up and when.
                let cause = failure.to_string();
                tracing::warn!(cause, event = "metadata_writeback_failed");
            })
            // `inspect_err` observes, `map_err` maps — split so the cause stays
            // out of the error VALUE, which carries no source (RULE ERR-RS).
            .map_err(|_logged| MetadataUnwritten::Unreachable)?;

        let code = response.status().as_u16();
        if let Some(refusal) = Self::classify(code) {
            tracing::warn!(code, event = "metadata_writeback_rejected");
            return Err(refusal);
        }
        tracing::debug!(code, event = "metadata_writeback_ok");
        Ok(())
    }

    /// Maps a response status onto the three outcomes an operator acts on
    /// differently.
    ///
    /// `clerk_backend.zig::mapStatus` draws the same lines, and it is a
    /// separate function here for the same reason the read's is: every branch
    /// is provable without standing up a listener.
    #[must_use]
    pub const fn classify(status: u16) -> Option<MetadataUnwritten> {
        match status {
            200..=299 => None,
            401 | 403 => Some(MetadataUnwritten::Unauthorized),
            404 => Some(MetadataUnwritten::UnknownSubject),
            _ => Some(MetadataUnwritten::Unreachable),
        }
    }
}

#[cfg(test)]
#[path = "metadata/tests.rs"]
mod tests;
