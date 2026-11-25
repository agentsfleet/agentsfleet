//! Resolving one App delivery to the fleets that subscribed to it.
//!
//! # The question this answers, and why it is a different one
//!
//! `/v1/webhooks/{fleet_id}/{source}` is addressed. The URL names the fleet and
//! the trigger, the secret is that fleet's, and there is exactly one place for
//! the delivery to go.
//!
//! An App delivery is not addressed to anything. A provider App is installed
//! once against an ORGANISATION, and every event for that installation arrives
//! at one URL carrying no workspace, no fleet and no principal — only the
//! provider's own account identifier. Everything about where it goes has to be
//! looked up, in this order:
//!
//! ```text
//!   installation.id ──► connector.installs ──► workspace
//!                       (SELECT_INSTALL_WORKSPACE)  │
//!                                                   ▼
//!                       fleet.triggers (app_event) ⋈ fleet.fleets
//!                       (SELECT_APP_SUBSCRIBERS: same workspace, active)
//!                                                       │
//!                                                       ▼
//!                       Binding::read, then
//!                       serves_repository + admits          ──► fan-out
//! ```

use af_core::id::Uuid7;
use af_wire::trigger_verb::TriggerKind;
use afd_fleet_lifecycle::FleetStatus;
use sha2::{Digest as _, Sha256};

use crate::binding::{Binding, StoredTrigger};
use crate::error::{self, Result};
use crate::{Ingress, sql};

/// The context a failed installation lookup reports under.
const CONTEXT_INSTALL: &str = "resolve an App installation";

/// The context a failed subscriber read reports under.
const CONTEXT_SUBSCRIBERS: &str = "resolve App ingress subscribers";

/// Where one App delivery is going.
///
/// Two states rather than a `Vec` a caller has to interpret: an empty vector
/// is a 200 saying nobody subscribed, and [`Self::To`] can never be empty.
#[derive(Debug)]
pub enum Fanout {
    /// The fleets to wake — at least one.
    To(Vec<Binding>),
    /// Nobody subscribed to this repository and event.
    ///
    /// The common case by a wide margin, and not an error: an App receives
    /// every event for every repository in an installation, and a workspace
    /// subscribes a handful of fleets to a handful of them.
    Nobody,
}

impl Ingress {
    /// The workspace a provider's App installation was connected to.
    ///
    /// `Ok(None)` for an installation this daemon has no row for — an App
    /// installed on an organisation that never finished connecting, or one
    /// connected to a workspace since deleted. Answered as a dropped delivery
    /// rather than an error: the sender is a correctly configured provider and
    /// has nothing to fix.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and a row this build cannot
    /// read. The workspace column is a `NOT NULL` foreign key, so a value that
    /// will not parse is a broken invariant rather than a race.
    pub async fn installation_workspace(
        &self,
        provider: &str,
        installation: &str,
    ) -> Result<Option<Uuid7>> {
        let mut connection = self.database.acquire().await?;
        sqlx::query_scalar(sql::SELECT_INSTALL_WORKSPACE)
            .bind(provider)
            .bind(installation)
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_INSTALL))
    }

    /// The fleets in `workspace` that subscribed to this repository and event.
    ///
    /// The relational narrowing is [`sql::SELECT_APP_SUBSCRIBERS`]'s and the
    /// repository and event match is [`Binding`]'s — see that statement's note
    /// on why the split falls there.
    ///
    /// A fleet whose trigger row the parser refuses stops the whole delivery
    /// rather than being skipped. Silently dropping it would
    /// leave a fleet that quietly never runs again after a bad write, which is
    /// the failure an operator cannot see; the raised error is one they can.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a row this build cannot read,
    /// and a stored trigger row the parser refuses.
    pub async fn subscribers(
        &self,
        workspace: &Uuid7,
        provider: &str,
        repository: &str,
        event: &str,
    ) -> Result<Fanout> {
        let mut connection = self.database.acquire().await?;
        let rows: Vec<SubscriberRow> = sqlx::query_as(sql::SELECT_APP_SUBSCRIBERS)
            .bind(workspace)
            .bind(FleetStatus::Active.as_str())
            .bind(TriggerKind::AppEvent.as_str())
            .bind(provider)
            .fetch_all(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_SUBSCRIBERS))?;

        let mut matched = Vec::new();
        for row in &rows {
            let fleet = row.fleet_id.clone();
            let owner = row.workspace_id.clone();

            // `is_runnable` is deliberately not re-asked: the statement bound
            // the status to `active`, and a second check here would be a
            // defensive re-test of a filter one layer up (RULE FN-RS). The
            // per-fleet route DOES ask, because its URL reaches an archived fleet
            // on purpose and owes that sender a reason.
            let subscribed = Binding::read(fleet, owner, &row.status, &row.trigger)?
                .filter(|binding| binding.serves_repository(repository) && binding.admits(event));

            matched.extend(subscribed);
        }

        Ok(fanout(matched))
    }
}

/// What identifies a repeat of this delivery.
///
/// The body's digest, NOT the delivery header the provider sets. That choice is
/// the whole reason this function exists rather than the App path reusing
/// `x-github-delivery` the way the per-fleet route does.
///
/// A provider's signature covers the BODY. It does not cover the headers, so
/// `x-github-delivery` is an unauthenticated value on a public endpoint: anyone
/// holding a captured delivery could resend it byte-for-byte with a fresh
/// delivery id, and a claim keyed on that id would see a new delivery and run
/// every subscribed fleet a second time. The digest cannot be varied without
/// varying the body, and varying the body invalidates the signature — so the
/// claim key is exactly as forgery-resistant as the wall in front of it.
///
/// The cost is that a provider redelivering a genuinely identical body is
/// suppressed even when it MEANT to send it twice. That is the correct trade
/// for this surface: an App delivery is an observation of something that
/// happened once, never a command sent twice on purpose.
#[must_use]
pub fn replay_id(body: &[u8]) -> String {
    hex::encode(Sha256::digest(body))
}

/// [`sql::SELECT_APP_SUBSCRIBERS`]'s row: one active, subscribed fleet.
#[derive(Debug, sqlx::FromRow)]
struct SubscriberRow {
    /// The fleet.
    fleet_id: Uuid7,
    /// Its workspace.
    workspace_id: Uuid7,
    /// The stored status word.
    status: String,
    /// Its App subscription row.
    #[sqlx(flatten)]
    trigger: StoredTrigger,
}

/// The matched set, as the two answers a route can act on.
fn fanout(matched: Vec<Binding>) -> Fanout {
    if matched.is_empty() {
        Fanout::Nobody
    } else {
        Fanout::To(matched)
    }
}

#[cfg(test)]
mod tests;
