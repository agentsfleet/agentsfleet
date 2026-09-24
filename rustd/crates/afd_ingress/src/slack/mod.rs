//! A chat mention's fleets: who in a workspace attached to the channel it
//! arrived in.
//!
//! The GitHub App read's sibling, split the same way (`sql.rs` on
//! `SELECT_APP_SUBSCRIBERS`): the statement asks only the relational question —
//! which fleets are in the workspace and alive — and the document question,
//! whether a fleet's `mention` trigger names THIS channel, is [`subscribed`],
//! a pure function a test reaches with no datastore.

use afd_core::id::Uuid7;
use afd_fleet_lifecycle::FleetStatus;
use afd_fleet_runtime::config::{Access, FleetConfig, Trigger};
use sqlx::Row as _;

use crate::error::{self, COLUMN_FLEET, COLUMN_STATUS, Result, row_unreadable};
use crate::{Ingress, sql};

mod admit;
mod message;
mod notice;
mod resident;
mod route;

pub use self::admit::MentionAdmission;
pub use self::message::{
    Composed, MESSAGE_CAP, THREAD_CAP, THREAD_HEADING, THREAD_UNAVAILABLE, compose,
};
pub use self::notice::{NoticeOwed, notice_key, notice_text};
pub use self::resident::{KIND_RESIDENT, Resident};
pub use self::route::{Notice, Route, route};
/// The channel type every mention method takes, and the name a resident is
/// found by, re-exported so a caller of this module needs no dependency on the
/// document crate for them.
pub use afd_fleet_runtime::FleetName;
pub use afd_fleet_runtime::config::ChannelId;

/// Statement name, for the context a query failure carries.
const CONTEXT_MENTION_SUBSCRIBERS: &str = "resolve mention subscribers";

/// The statuses a subscriber can be read in.
///
/// Paused and stopped fleets are read, not skipped: a person who addresses one
/// by name is owed a notice saying so, and a fleet the read silently dropped
/// would earn "no such fleet" instead. Installing and killed fleets are not
/// attached to anything yet, or any longer.
const READABLE: [FleetStatus; 3] = [
    FleetStatus::Active,
    FleetStatus::Paused,
    FleetStatus::Stopped,
];

/// One fleet attached to a channel, as routing reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subscriber {
    /// The fleet.
    pub fleet: Uuid7,
    /// The name a person addresses it by.
    pub name: String,
    /// Whether it can run now; a paused or stopped fleet earns a notice.
    pub runnable: bool,
    /// Whether it takes only mentions addressed to it by name. A fleet that
    /// can write to a repository never receives an unaddressed mention.
    pub addressed_only: bool,
}

impl Ingress {
    /// The fleets in `workspace` whose `mention` trigger names `channel` on
    /// `provider`, in fleet-id order.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a row this build cannot read,
    /// and a stored document that no longer parses.
    pub async fn mention_subscribers(
        &self,
        workspace: &Uuid7,
        provider: &str,
        channel: &ChannelId,
    ) -> Result<Vec<Subscriber>> {
        let statuses: Vec<&str> = READABLE.iter().map(|status| status.as_str()).collect();
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(sql::SELECT_MENTION_CANDIDATES)
            .bind(workspace.as_str())
            .bind(&statuses)
            .fetch_all(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_MENTION_SUBSCRIBERS))?;
        drop(connection);

        rows.iter()
            .map(|row| {
                let unreadable = error::query(CONTEXT_MENTION_SUBSCRIBERS);
                let fleet: String = row.try_get(0).map_err(&unreadable)?;
                let status: String = row.try_get(1).map_err(&unreadable)?;
                let document: String = row.try_get(2).map_err(&unreadable)?;
                let fleet = Uuid7::parse(&fleet).map_err(|_shape| row_unreadable(COLUMN_FLEET))?;
                subscribed(fleet, &status, &document, provider, channel)
            })
            .filter_map(Result::transpose)
            .collect()
    }
}

/// Whether one stored fleet is attached to `channel` on `provider`.
///
/// `None` for a fleet whose document declares no `mention` trigger, or one
/// naming another provider or another channel. The channel is compared by its
/// identifier, which a rename does not change; the provider ignoring case, for
/// the reason the webhook source is (`binding.rs`).
///
/// # Errors
/// A status this build cannot name, and a stored document that no longer
/// parses — both this deployment's incidents rather than a sender's.
pub(crate) fn subscribed(
    fleet: Uuid7,
    stored_status: &str,
    document: &str,
    provider: &str,
    channel: &ChannelId,
) -> Result<Option<Subscriber>> {
    let status = FleetStatus::parse(stored_status).ok_or_else(|| row_unreadable(COLUMN_STATUS))?;
    let config = FleetConfig::stored(document)?;
    let attached = config.triggers().iter().any(|trigger| match trigger {
        Trigger::Mention(mention) => {
            mention.source.eq_ignore_ascii_case(provider) && &mention.channel == channel
        }
        Trigger::Webhook(_) | Trigger::Cron(_) | Trigger::Api => false,
    });
    Ok(attached.then(|| Subscriber {
        fleet,
        name: config.name().as_str().to_owned(),
        runnable: status == FleetStatus::Active,
        addressed_only: config
            .repository_binding()
            .is_some_and(|binding| binding.access() == Access::Write),
    }))
}

#[cfg(test)]
mod tests;
