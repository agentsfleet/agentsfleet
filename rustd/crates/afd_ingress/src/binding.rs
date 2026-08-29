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
//! # Why the FIRST webhook trigger, still
//!
//! `serve_webhook_lookup.zig` takes `LIMIT 1` over the trigger array and says
//! why in its own header: the `{source}` URL segment that would let one fleet
//! carry two webhook sources "lands with the install + list response slice",
//! and until then the URL carries `fleet_id` alone. The rule is unchanged and
//! the reason is unchanged; what moved is where it is written, from a
//! sub-select into [`Binding::resolve`], where a test can reach it.
//!
//! `Trigger::parse_set` already refuses two webhook triggers that share one
//! source, so "the first" is unambiguous whenever a fleet declares more than
//! one — they differ by source, and the URL cannot yet name which.
//!
//! # Where the URL DOES name the source
//!
//! `/v1/ingress/{provider}` carries it, so [`Binding::read_for_source`] selects
//! on it instead of taking the first. Both readers exist because the two
//! surfaces genuinely know different things, not because one is a fallback for
//! the other: a fleet declaring Slack and GitHub is measured on its GitHub
//! trigger when GitHub sent the delivery, and on whichever comes first when the
//! URL says only which fleet.

use afd_core::id::Uuid7;
use afd_fleet_lifecycle::FleetStatus;
use afd_fleet_runtime::config::{FleetConfig, Trigger, Webhook, WebhookSignature};
use afd_webhook::Scheme;

use crate::error::{COLUMN_STATUS, Result, row_unreadable};

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
}

impl Binding {
    /// Reads one from a row's three columns.
    ///
    /// `Ok(None)` when the fleet declares no webhook trigger at all: a delivery
    /// to a fleet nobody configured for webhooks is indistinguishable from one
    /// to a fleet that does not exist, and both answer `UZ-WH-001`. Telling
    /// them apart would confirm a fleet id to whoever guessed it.
    ///
    /// # Errors
    /// Reports a stored document that no longer parses, and a status this build
    /// cannot name. Both are this deployment's incidents rather than a sender's.
    pub(crate) fn read(
        fleet: Uuid7,
        workspace: Uuid7,
        stored_status: &str,
        document: &str,
    ) -> Result<Option<Self>> {
        Self::select(fleet, workspace, stored_status, document, None)
    }

    /// Reads the trigger a named provider sends to, rather than the first one.
    ///
    /// The App ingress difference. `/v1/webhooks/{fleet_id}` is addressed to a
    /// fleet and the URL cannot yet name WHICH of its webhook triggers, so
    /// [`Self::read`] takes the first and says why. `/v1/ingress/{provider}`
    /// carries the provider in the path, so it can and must say: a fleet
    /// declaring Slack before GitHub would otherwise be measured on its Slack
    /// trigger — a wrong allow-list and a wrong repository set, silently.
    ///
    /// `Trigger::parse_set` already refuses two webhook triggers sharing one
    /// source, so at most one trigger can match and "the first matching" is the
    /// only one.
    ///
    /// # Errors
    /// As [`Self::read`].
    pub(crate) fn read_for_source(
        fleet: Uuid7,
        workspace: Uuid7,
        stored_status: &str,
        document: &str,
        source: &str,
    ) -> Result<Option<Self>> {
        Self::select(fleet, workspace, stored_status, document, Some(source))
    }

    /// Both readers, over the trigger `source` selects.
    fn select(
        fleet: Uuid7,
        workspace: Uuid7,
        stored_status: &str,
        document: &str,
        source: Option<&str>,
    ) -> Result<Option<Self>> {
        let status =
            FleetStatus::parse(stored_status).ok_or_else(|| row_unreadable(COLUMN_STATUS))?;
        let config = FleetConfig::stored(document)?;

        Ok(webhook_trigger(&config, source).map(|trigger| Self {
            fleet,
            workspace,
            status,
            trigger: trigger.clone(),
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
    /// nothing for a fleet somebody paused on purpose — the rework that retired
    /// `UZ-WH-003` (`error_entries.zig:135`). The steer ingress, where a person
    /// is waiting for an answer, refuses loudly with `UZ-AGT-012` instead.
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
    /// `credential_name ?? source`, which is the Zig's rule verbatim: the
    /// override exists so two fleets on one provider can hold different
    /// secrets, and its absence means the provider's own name IS the key.
    #[must_use]
    pub fn credential_name(&self) -> &str {
        self.trigger
            .credential_name
            .as_deref()
            .unwrap_or(&self.trigger.source)
    }

    /// The scheme this delivery is verified under, when the daemon ships one.
    ///
    /// `None` is a source no scheme is declared for, which the ingress answers
    /// as [`afd_webhook::Refusal::Unconfigured`] — never as a pass. The Zig
    /// carries the same fail-closed note on the same branch: *"always populate
    /// the scheme when the provider is recognized, so the middleware fails
    /// closed with UZ-WH-020"*.
    #[must_use]
    pub fn scheme(&self) -> Option<Scheme> {
        Scheme::for_source(self.source())
    }

    /// The Svix `secret_ref`, for the fleet whose trigger declares one.
    ///
    /// A separate vault key from [`Self::credential_name`] and a separate
    /// SHAPE: this one resolves to a raw `whsec_…` string where the HMAC family
    /// resolves to a JSON object. Two readers, because they are two stored
    /// forms — see [`crate::secret`].
    #[must_use]
    pub fn signature(&self) -> Option<&WebhookSignature> {
        self.trigger.signature.as_ref()
    }

    /// Whether this trigger's allow-list admits `event`.
    ///
    /// An absent list fires on every event, which is `github_filter.zig`'s rule
    /// and the shape [`Webhook::events`] already documents.
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
    /// no event list is every event. That asymmetry is deliberate and it is
    /// `SELECT_APP_INGRESS_TARGETS`'s, written out — its repository clause is an
    /// `EXISTS` over `COALESCE(trigger->'repositories', '[]')`, which matches
    /// nothing when the key is absent, while its event clause is
    /// `NOT (trigger ? 'events') OR …`, which matches everything.
    ///
    /// The reason the two differ: one App delivery is offered to every fleet in
    /// the workspace, so a fleet that named no repository has not opted in to
    /// anything and must not be woken by another team's repository. A fleet
    /// reached at `/v1/webhooks/{fleet_id}` was addressed on purpose, so its
    /// silence about events means "all of them".
    ///
    /// Compared case-insensitively, as the SQL's `lower(…) = lower(…)` does:
    /// GitHub treats `Owner/Repo` and `owner/repo` as one repository, and a
    /// subscription that missed on case would fail in a way an author could
    /// stare straight at without seeing.
    #[must_use]
    pub fn serves_repository(&self, repository: &str) -> bool {
        self.trigger.repositories.as_deref().is_some_and(|allowed| {
            allowed
                .iter()
                .any(|named| named.eq_ignore_ascii_case(repository))
        })
    }
}

/// The first webhook trigger a document declares, or the first from `source`.
///
/// `None` for `source` is "whichever comes first", which is what a URL carrying
/// only a fleet id can ask for.
fn webhook_trigger<'c>(config: &'c FleetConfig, source: Option<&str>) -> Option<&'c Webhook> {
    config.triggers().iter().find_map(|trigger| match trigger {
        Trigger::Webhook(hook) => match source {
            // Case-insensitive for the reason `serves_repository` is: the
            // provider names itself in the URL and an author names it in the
            // document, and neither spelling is authoritative over the other.
            Some(named) if !hook.source.eq_ignore_ascii_case(named) => None,
            Some(_) | None => Some(hook),
        },
        Trigger::Cron(_) | Trigger::Api => None,
    })
}

#[cfg(test)]
mod tests;
