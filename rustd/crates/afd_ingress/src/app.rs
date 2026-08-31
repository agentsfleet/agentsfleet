//! Resolving one App delivery to the fleets that subscribed to it.
//!
//! # The question this answers, and why it is a different one
//!
//! `/v1/webhooks/{fleet_id}` is addressed. The URL names the fleet, the secret
//! is that fleet's, and there is exactly one place for the delivery to go.
//!
//! An App delivery is not addressed to anything. A provider App is installed
//! once against an ORGANISATION, and every event for that installation arrives
//! at one URL carrying no workspace, no fleet and no principal — only the
//! provider's own account identifier. Everything about where it goes has to be
//! looked up, in this order:
//!
//! ```text
//!   installation.id ──► core.connector_installs ──► workspace
//!                       (SELECT_INSTALL_WORKSPACE)      │
//!                                                       ▼
//!                       core.fleets ⋈ integration_grants
//!                       (SELECT_APP_SUBSCRIBERS: active, granted)
//!                                                       │
//!                                                       ▼
//!                       Binding::read_for_source, then
//!                       serves_repository + admits          ──► fan-out
//! ```
//!
//! # Why the ceiling is on the MATCHED set
//!
//! `github.zig` asks Postgres for `MAX_FANOUT + 1` rows and refuses the
//! delivery when the sentinel row comes back — the ceiling is enforced on what
//! SQL matched, because SQL did the matching. Here the document half of the
//! match is [`Binding`]'s, so the ceiling is counted here instead. It is the
//! same ceiling over the same set: the number of fleets one delivery wakes.
//!
//! What the ceiling is FOR is worth stating, because it is not a performance
//! guard. One delivery becoming a hundred fleet runs is a hundred model spends
//! from a single HTTP request. A workspace that has wired itself past that has
//! a configuration incident, and a loud refusal surfaces it before the invoice.

use afd_core::id::Uuid7;
use afd_fleet_lifecycle::FleetStatus;
use afd_wire::grant::status;
use sha2::{Digest as _, Sha256};
use sqlx::Row as _;

use crate::binding::Binding;
use crate::error::{self, Result};
use crate::{Ingress, sql};

/// The context a failed installation lookup reports under.
const CONTEXT_INSTALL: &str = "resolve an App installation";

/// The context a failed subscriber read reports under.
const CONTEXT_SUBSCRIBERS: &str = "resolve App ingress subscribers";

/// The most fleets one App delivery may wake.
///
/// `github.zig`'s `MAX_FANOUT`, unchanged. See the module note on what it
/// protects — it is a spend bound, not a latency one.
pub const MAX_FANOUT: usize = 100;

/// Where one App delivery is going.
///
/// Three states rather than a `Vec` a caller has to interpret: an empty vector
/// and a hundred-and-one-entry vector are both "do not fan out", and they are
/// different answers on the wire — one is a 200 saying nobody subscribed, the
/// other is a refusal. Separate variants mean a route cannot answer one while
/// meaning the other, and [`Self::To`] can never be empty.
#[derive(Debug)]
pub enum Fanout {
    /// The fleets to wake — at least one, never more than [`MAX_FANOUT`].
    To(Vec<Binding>),
    /// Nobody subscribed to this repository and event.
    ///
    /// The common case by a wide margin, and not an error: an App receives
    /// every event for every repository in an installation, and a workspace
    /// subscribes a handful of fleets to a handful of them.
    Nobody,
    /// More fleets matched than one delivery may wake.
    TooMany(usize),
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
        let row = sqlx::query(sql::SELECT_INSTALL_WORKSPACE)
            .bind(provider)
            .bind(installation)
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_INSTALL))?;

        row.map(|row| {
            let workspace: String = row.try_get(0).map_err(error::query(CONTEXT_INSTALL))?;
            Uuid7::parse(&workspace)
                .map_err(|_shape| error::row_unreadable(error::COLUMN_WORKSPACE))
        })
        .transpose()
    }

    /// The fleets in `workspace` that subscribed to this repository and event.
    ///
    /// The relational narrowing is [`sql::SELECT_APP_SUBSCRIBERS`]'s and the
    /// document match is [`Binding`]'s — see that statement's note on why the
    /// split falls there.
    ///
    /// A fleet whose row is readable but whose stored document is not stops the
    /// whole delivery rather than being skipped. Silently dropping it would
    /// leave a fleet that quietly never runs again after a bad write, which is
    /// the failure an operator cannot see; the raised error is one they can.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a row this build cannot read,
    /// and a stored document that no longer parses.
    pub async fn subscribers(
        &self,
        workspace: &Uuid7,
        provider: &str,
        repository: &str,
        event: &str,
    ) -> Result<Fanout> {
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(sql::SELECT_APP_SUBSCRIBERS)
            .bind(workspace.as_str())
            .bind(FleetStatus::Active.as_str())
            .bind(provider)
            .bind(status::APPROVED)
            .fetch_all(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_SUBSCRIBERS))?;

        let mut matched = Vec::new();
        for row in &rows {
            let unreadable = error::query(CONTEXT_SUBSCRIBERS);
            let fleet: String = row.try_get(0).map_err(&unreadable)?;
            let owner: String = row.try_get(1).map_err(&unreadable)?;
            let fleet_status: String = row.try_get(2).map_err(&unreadable)?;
            let document: String = row.try_get(3).map_err(&unreadable)?;

            let fleet = Uuid7::parse(&fleet)
                .map_err(|_shape| error::row_unreadable(error::COLUMN_FLEET))?;
            let owner = Uuid7::parse(&owner)
                .map_err(|_shape| error::row_unreadable(error::COLUMN_WORKSPACE))?;

            // `is_runnable` is deliberately not re-asked: the statement bound
            // the status to `active`, and a second check here would be a
            // defensive re-test of a filter one layer up (RULE FN-RS). The
            // per-fleet route DOES ask, because its URL reaches a paused fleet
            // on purpose and owes that sender a reason.
            let subscribed =
                Binding::read_for_source(fleet, owner, &fleet_status, &document, provider)?.filter(
                    |binding| binding.serves_repository(repository) && binding.admits(event),
                );

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
///
/// `github.zig`'s `authenticatedReplayId`, and its name is the argument.
#[must_use]
pub fn replay_id(body: &[u8]) -> String {
    hex::encode(Sha256::digest(body))
}

/// The matched set, as the three answers a route can act on.
fn fanout(matched: Vec<Binding>) -> Fanout {
    match matched.len() {
        0 => Fanout::Nobody,
        count if count > MAX_FANOUT => Fanout::TooMany(count),
        _ => Fanout::To(matched),
    }
}

#[cfg(test)]
mod tests;
