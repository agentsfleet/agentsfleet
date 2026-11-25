//! What one fleet's trigger row says about receiving a signed delivery.
//!
//! # Parsed once, at the boundary
//!
//! A [`Binding`] can only hold a fleet that HAS a trigger row for the
//! delivery's source. Resolution either produces one or answers `Ok(None)`, so
//! nothing downstream re-asks "does this fleet take webhooks" and nothing
//! downstream can forget to. The status, the vault key and the scheme are
//! settled here too, which is why the verifier and the appender below take a
//! `&Binding` rather than four loose strings a call site could pair wrongly.
//!
//! # One parser for the add and the read
//!
//! A run's `trigger_add` is read by `af_fleet_runtime`'s trigger parser before
//! the row is written, and [`StoredTrigger`] reads the row back through the
//! same parser. A row the parser would refuse is a row nothing in this daemon
//! writes, so it is this deployment's incident, never a sender's.
//!
//! # Where the URL names the source
//!
//! `/v1/webhooks/{fleet_id}/{source}` and `/v1/ingress/{provider}` carry it,
//! and the statement selects the row for that source. `fleet.triggers` holds
//! one row per (fleet, `trigger_type`, source), so the match is unambiguous.

use af_core::id::Uuid7;
use af_fleet_runtime::config::{Trigger, Webhook, WebhookVerifier, parse_triggers};
use af_webhook::{Family, Scheme};
use afd_fleet_lifecycle::FleetStatus;
use serde_json::{Map, Value, json};

use crate::error::{self, Result};

/// One trigger row's columns a delivery is measured against, as the ingress
/// statements select them from `fleet.triggers`.
#[derive(Debug, Clone, Default, PartialEq, Eq, sqlx::FromRow)]
pub struct StoredTrigger {
    /// A webhook's source slug, or `github` for an App subscription.
    pub source: String,
    /// The event names that wake the fleet; `None` wakes it on every event.
    pub events: Option<Vec<String>>,
    /// The repositories whose deliveries wake the fleet, as `owner/name`.
    pub repositories: Option<Vec<String>>,
    /// The signing recipe of a source outside the built-in registry.
    pub verifier_scheme: Option<String>,
    /// The header that recipe reads its signature from.
    pub verifier_header: Option<String>,
    /// The workspace vault key of the signing secret; the source when absent.
    pub secret_name: Option<String>,
}

impl StoredTrigger {
    /// The row read back through the trigger parser its add went through.
    ///
    /// # Errors
    /// A row the parser refuses: one nothing in this daemon writes.
    fn webhook(&self) -> Result<Option<Webhook>> {
        Ok(parse_triggers(&self.document().to_string())?
            .into_iter()
            .find_map(|trigger| match trigger {
                Trigger::Webhook(hook) => Some(hook),
                Trigger::Cron(_) | Trigger::Mention(_) => None,
            }))
    }

    /// The row as a trigger list of one, in the parser's own keys.
    fn document(&self) -> Value {
        let mut trigger = Map::new();
        trigger.insert("type".to_owned(), json!("webhook"));
        trigger.insert("source".to_owned(), json!(self.source));
        if let Some(events) = &self.events {
            trigger.insert("events".to_owned(), json!(events));
        }
        if let Some(repositories) = &self.repositories {
            trigger.insert("repositories".to_owned(), json!(repositories));
        }
        if let Some(secret) = &self.secret_name {
            trigger.insert("credential_name".to_owned(), json!(secret));
        }
        if let Some(scheme) = &self.verifier_scheme {
            let mut recipe = Map::new();
            recipe.insert("scheme".to_owned(), json!(scheme));
            if let Some(header) = &self.verifier_header {
                recipe.insert("header".to_owned(), json!(header));
            }
            trigger.insert("verifier".to_owned(), Value::Object(recipe));
        }
        json!([trigger])
    }
}

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
    /// The trigger row this delivery is measured against.
    trigger: Webhook,
    /// Resolved at construction, so every binding names a usable recipe.
    family: Family,
}

impl Binding {
    /// Reads the trigger row a statement selected for the delivery's source.
    ///
    /// `Ok(None)` means the row names no recipe this daemon can verify.
    ///
    /// # Errors
    /// Reports an unreadable stored row or status.
    pub(crate) fn read(
        fleet: Uuid7,
        workspace: Uuid7,
        stored_status: &str,
        row: &StoredTrigger,
    ) -> Result<Option<Self>> {
        let status = error::stored_status(stored_status)?;
        Ok(row.webhook()?.and_then(|trigger| {
            trigger
                .verifier
                .as_ref()
                .map(WebhookVerifier::family)
                .or_else(|| Family::for_source(&trigger.source))
                .map(|family| Self {
                    fleet,
                    workspace,
                    status,
                    trigger,
                    family,
                })
        }))
    }

    /// Exposes the production reader to sibling-crate tests.
    ///
    /// # Errors
    /// Reports an unreadable stored row or status.
    #[cfg(feature = "test-util")]
    pub fn stored(
        fleet: Uuid7,
        workspace: Uuid7,
        stored_status: &str,
        row: &StoredTrigger,
    ) -> Result<Option<Self>> {
        Self::read(fleet, workspace, stored_status, row)
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
    /// `false` is not a refusal on this surface. A webhook to an archived fleet
    /// is answered 200 with an ignore reason, because a sender's retry queue
    /// adds nothing for a fleet that will never run again. The steer ingress, where
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
    /// `secret_name ?? source`: a row names its own key so two fleets on one
    /// provider can hold different secrets, and its absence means the
    /// provider's own name IS the key.
    #[must_use]
    pub fn credential_name(&self) -> &str {
        self.trigger
            .credential_name
            .as_deref()
            .unwrap_or(&self.trigger.source)
    }

    /// The stored recipe or the registry source selects the verifier.
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
    /// An absent list fires on every event. There is deliberately no
    /// empty-list arm: the parser bounds the list at `min = 1`, so `Some([])`
    /// is a row [`StoredTrigger`] refuses before this is ever called.
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
    /// no event list is every event. One App delivery is offered to every
    /// fleet in the workspace, so a fleet that named no repository has not
    /// opted in to anything and must not be woken by another team's
    /// repository.
    ///
    /// Compared case-insensitively: GitHub treats `Owner/Repo` and
    /// `owner/repo` as one repository.
    #[must_use]
    pub fn serves_repository(&self, repository: &str) -> bool {
        self.trigger.repositories.as_deref().is_some_and(|allowed| {
            allowed
                .iter()
                .any(|named| named.eq_ignore_ascii_case(repository))
        })
    }
}

#[cfg(test)]
mod tests;
