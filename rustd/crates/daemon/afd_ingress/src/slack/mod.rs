//! A chat mention's fleets: who in a workspace subscribed to the channel it
//! arrived in.
//!
//! The GitHub App read's sibling (`sql.rs` on `SELECT_APP_SUBSCRIBERS`): the
//! statement selects the fleets whose `mention` row names THIS channel, and
//! [`subscribed`] reads each one's status and approved GitHub reach, a pure
//! function a test reaches with no datastore.

use af_core::id::Uuid7;
use af_fleet_runtime::config::{Access, RepositoryBinding};
use af_wire::trigger_verb::TriggerKind;
use afd_api_wire::grant::status;
use afd_fleet_lifecycle::FleetStatus;

use crate::error::{self, Result};
use crate::{Ingress, sql};

mod admit;
mod message;
mod notice;
mod notice_owner;
mod route;

pub use self::admit::MentionTurn;
pub use self::message::{Composed, THREAD_HEADING, THREAD_UNAVAILABLE, compose};
pub use self::notice::{NoticeOwed, notice_text};
pub use self::notice_owner::NoticeOwner;
pub use self::route::{Notice, Route, route};
/// The channel type used by mention routing and notice ownership.
pub(crate) use af_fleet_runtime::config::ChannelId;

/// Statement name, for the context a query failure carries.
const CONTEXT_MENTION_SUBSCRIBERS: &str = "resolve mention subscribers";

/// The statuses a subscriber can be read in.
///
/// Archived fleets are read, not skipped: a person who addresses one by name
/// is owed a notice saying so, and a fleet the read silently dropped would earn
/// "no such fleet" instead.
const READABLE: [FleetStatus; 2] = [FleetStatus::Active, FleetStatus::Archived];

/// The grant a fleet's GitHub scope is approved under, as the policy build and
/// the token mint name it.
const SERVICE_GITHUB: &str = "github";

/// One fleet attached to a channel, as routing reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subscriber {
    /// The fleet.
    pub fleet: Uuid7,
    /// The name a person addresses it by.
    pub name: String,
    /// Whether it can run now; an archived fleet earns a notice.
    pub runnable: bool,
    /// Whether it takes only mentions addressed to it by name. A fleet that
    /// can write to a repository never receives an unaddressed mention.
    pub addressed_only: bool,
}

impl Ingress {
    /// The fleets in `workspace` whose `mention` row names `channel` on
    /// `provider`, in fleet-id order.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a row this build cannot read,
    /// and a stored GitHub scope the parser refuses.
    pub async fn mention_subscribers(
        &self,
        workspace: &Uuid7,
        provider: &str,
        channel: &ChannelId,
    ) -> Result<Vec<Subscriber>> {
        let statuses: Vec<&str> = READABLE.iter().map(|status| status.as_str()).collect();
        let mut connection = self.database.acquire().await?;
        let rows: Vec<CandidateRow> = sqlx::query_as(sql::SELECT_MENTION_CANDIDATES)
            .bind(workspace)
            .bind(&statuses)
            .bind(TriggerKind::Mention.as_str())
            .bind(provider.to_ascii_lowercase())
            .bind(channel.as_str())
            .bind(SERVICE_GITHUB)
            .bind(status::APPROVED)
            .bind(afd_api_wire::grant::grant_type::INTEGRATION)
            .fetch_all(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_MENTION_SUBSCRIBERS))?;
        drop(connection);

        rows.iter()
            .map(|row| {
                subscribed(
                    row.fleet_id.clone(),
                    &row.name,
                    &row.status,
                    row.github_scope.as_deref(),
                )
            })
            .collect()
    }
}

/// [`sql::SELECT_MENTION_CANDIDATES`]'s row: one fleet subscribed to the channel.
#[derive(Debug, sqlx::FromRow)]
struct CandidateRow {
    /// The fleet.
    fleet_id: Uuid7,
    /// Its saved name: the one the dashboard and the command line show.
    name: String,
    /// The stored status word.
    status: String,
    /// The GitHub scope a person approved for it, as JSON text, or `None`.
    github_scope: Option<String>,
}

/// One subscribed fleet as routing reads it.
///
/// A fleet whose approved GitHub scope reaches write takes only mentions
/// addressed to it by name: an unaddressed mention in a busy channel must not
/// steer a fleet that can push to a repository.
///
/// # Errors
/// A status this build cannot name, and a stored scope the parser refuses —
/// both this deployment's incidents rather than a sender's.
pub(crate) fn subscribed(
    fleet: Uuid7,
    name: &str,
    stored_status: &str,
    github_scope: Option<&str>,
) -> Result<Subscriber> {
    let status = error::stored_status(stored_status)?;
    let addressed_only = github_scope
        .map(RepositoryBinding::parse)
        .transpose()?
        .is_some_and(|binding| binding.access() == Access::Write);
    Ok(Subscriber {
        fleet,
        name: name.to_owned(),
        runnable: status.is_runnable(),
        addressed_only,
    })
}

#[cfg(test)]
mod tests;
