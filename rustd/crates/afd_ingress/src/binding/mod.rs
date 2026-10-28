//! What one fleet's row says about receiving a signed delivery.
//!
//! # Parsed once, at the boundary
//!
//! A [`Binding`] can only hold a fleet that HAS a webhook trigger. Resolution
//! either produces one or answers `Ok(None)`, so nothing downstream re-asks
//! "does this fleet take webhooks" and nothing downstream can forget to. The
//! status, the vault key and the scheme are settled here too, which is why the
//! verifier and the appender below take a `&Binding` rather than four loose
//! strings a call site could pair wrongly.
//!
//! # Where the URL names the source
//!
//! `/v1/webhooks/{fleet_id}/{source}` and `/v1/ingress/{provider}` carry it,
//! so [`Binding::read_for_source`] selects the trigger declared for that
//! source. `Trigger::parse_set` refuses two webhook triggers on one source, so
//! the match is unambiguous.
//!
use afd_core::id::Uuid7;
use afd_fleet_lifecycle::FleetStatus;
use afd_fleet_runtime::config::{FleetConfig, Trigger, Webhook, WebhookVerifier};
use afd_webhook::{Family, Scheme};

use crate::error::{self, Result};

/// A fleet that takes signed deliveries, and everything one is checked against.
#[derive(Debug, Clone)]
pub struct Binding {
    /// The fleet the delivery is addressed to.
    fleet: Uuid7,
    /// The workspace whose vault holds the signing secret.
    ///
    /// Read from the row rather than taken from the request, because the
    /// request has no principal to carry one — see [`crate::sql`].
    workspace: Uuid7,
    /// What the fleet's row says it is doing.
    status: FleetStatus,
    /// The webhook trigger this delivery is measured against.
    trigger: Webhook,
    /// Resolved at construction, so every binding names a usable recipe.
    family: Family,
}

impl Binding {
    /// Reads the trigger addressed by the source in the delivery URL.
    ///
    /// `Ok(None)` means this fleet declares no trigger for that source.
    ///
    /// # Errors
    /// Reports an unreadable stored document or status.
    pub(crate) fn read_for_source(
        fleet: Uuid7,
        workspace: Uuid7,
        stored_status: &str,
        document: &str,
        source: &str,
    ) -> Result<Option<Self>> {
        Self::select(fleet, workspace, stored_status, document, source)
    }

    /// Exposes the production reader to sibling-crate tests.
    ///
    /// # Errors
    /// Reports an unreadable stored document or status.
    #[cfg(feature = "test-util")]
    pub fn stored(
        fleet: Uuid7,
        workspace: Uuid7,
        stored_status: &str,
        document: &str,
        source: &str,
    ) -> Result<Option<Self>> {
        Self::select(fleet, workspace, stored_status, document, source)
    }

    /// Both readers, over the trigger `source` selects.
    fn select(
        fleet: Uuid7,
        workspace: Uuid7,
        stored_status: &str,
        document: &str,
        source: &str,
    ) -> Result<Option<Self>> {
        let status = error::stored_status(stored_status)?;
        let config = FleetConfig::stored(document)?;

        Ok(webhook_trigger(&config, source).and_then(|trigger| {
            trigger
                .verifier
                .as_ref()
                .map(WebhookVerifier::family)
                .or_else(|| Family::for_source(&trigger.source))
                .map(|family| Self {
                    fleet,
                    workspace,
                    status,
                    trigger: trigger.clone(),
                    family,
                })
        }))
    }

    /// The fleet the delivery wakes.
    #[must_use]
    pub const fn fleet(&self) -> &Uuid7 {
        &self.fleet
    }

    /// The workspace whose vault holds the signing secret.
    #[must_use]
    pub const fn workspace(&self) -> &Uuid7 {
        &self.workspace
    }

    /// Whether this fleet will take new work.
    ///
    /// `false` is not a refusal on this surface. A webhook to a paused fleet is
    /// answered 200 with an ignore reason, because a sender's retry queue adds
    /// nothing for a fleet somebody paused on purpose. The steer ingress, where
    /// a person is waiting for an answer, refuses loudly with `UZ-AGT-012`
    /// instead.
    #[must_use]
    pub const fn is_runnable(&self) -> bool {
        self.status.is_runnable()
    }

    /// The provider a delivery must be signed by.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.trigger.source
    }

    /// The vault key holding this fleet's shared secret.
    ///
    /// `credential_name ?? source`: the override exists so two fleets on one
    /// provider can hold different secrets, and its absence means the
    /// provider's own name IS the key.
    #[must_use]
    pub fn credential_name(&self) -> &str {
        self.trigger
            .credential_name
            .as_deref()
            .unwrap_or(&self.trigger.source)
    }

    /// The stored declaration or registry source selects the recipe.
    #[must_use]
    pub const fn family(&self) -> Family {
        self.family
    }

    /// The signature header selected by the stored trigger.
    #[must_use]
    pub fn signature_header(&self) -> Option<&str> {
        self.trigger
            .verifier
            .as_ref()
            .and_then(WebhookVerifier::header)
            .or_else(|| Scheme::for_source(self.source()).map(Scheme::signature_header))
    }

    /// Whether this trigger's allow-list admits `event`.
    ///
    /// An absent list fires on every event, which is the shape
    /// [`Webhook::events`] already documents.
    ///
    /// There is deliberately no empty-list arm. The schema bounds the list at
    /// `min = 1`, so `Some([])` is a state [`FleetConfig::stored`] refuses
    /// before this is ever called — and a branch here guessing what an empty
    /// list "meant" would be a second, softer answer to a question the parser
    /// already answered by rejecting the document. Interior code that trusts
    /// its types needs no defensive re-check (RULE FN-RS).
    #[must_use]
    pub fn admits(&self, event: &str) -> bool {
        self.trigger
            .events
            .as_deref()
            .is_none_or(|allowed| allowed.iter().any(|kind| &**kind == event))
    }

    /// Whether this trigger subscribes to `repository`.
    ///
    /// The App-ingress counterpart to [`Self::admits`], and it answers the
    /// OPPOSITE way when the list is absent: no list is no subscription, where
    /// no event list is every event. That asymmetry is deliberate.
    ///
    /// The reason the two differ: one App delivery is offered to every fleet in
    /// the workspace, so a fleet that named no repository has not opted in to
    /// anything and must not be woken by another team's repository. A fleet
    /// reached at `/v1/webhooks/{fleet_id}/{source}` was addressed on purpose,
    /// so its silence about events means "all of them".
    ///
    /// Compared case-insensitively: GitHub treats `Owner/Repo` and
    /// `owner/repo` as one repository, and a subscription that missed on case
    /// would fail in a way an author could stare straight at without seeing.
    #[must_use]
    pub fn serves_repository(&self, repository: &str) -> bool {
        self.trigger.repositories.as_deref().is_some_and(|allowed| {
            allowed
                .iter()
                .any(|named| named.eq_ignore_ascii_case(repository))
        })
    }
}

/// Only the trigger addressed by this source can receive the delivery.
fn webhook_trigger<'c>(config: &'c FleetConfig, source: &str) -> Option<&'c Webhook> {
    config.triggers().iter().find_map(|trigger| match trigger {
        Trigger::Webhook(hook) if hook.source.eq_ignore_ascii_case(source) => Some(hook),
        Trigger::Webhook(_) | Trigger::Cron(_) | Trigger::Mention(_) => None,
    })
}

#[cfg(test)]
mod tests;
