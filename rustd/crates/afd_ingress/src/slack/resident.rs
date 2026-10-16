//! A chat channel's resident: the fleet that answers a mention no attached
//! fleet takes, materialised on the first one.
//!
//! # The configuration is code, and the instructions are prose
//!
//! What a resident may do comes from the native definition built here — one
//! `api` trigger, no tools, no hosts, a small daily ceiling — and never from
//! its instructions, which are prose only. So nothing a channel's members say,
//! and nothing the instructions say, can widen what the resident reaches
//! (RULE PRI). A person who needs more is told how to set up a fleet that has
//! it.
//!
//! # One resident per channel, however many first mentions race
//!
//! The name is derived from the team and the channel, and a workspace holds at
//! most one fleet per name, so two first mentions installing at once collide on
//! the name and the loser finds the winner by it. The binding row is
//! insert-once on the channel, so both then read back the same fleet.
//!
//! # One workspace's resident
//!
//! A Slack team can move to another workspace, and the binding row stays
//! behind. Every read and write here is scoped to the workspace the mention
//! resolved to, so a moved team's first mention installs and binds a resident
//! of its own instead of running the one it left.
//!
//! # Its memory is the channel's
//!
//! Memory is keyed by fleet. One fleet per channel means every thread in the
//! channel shares one memory, and no other channel's resident can read it.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_core::money::NANOS_PER_USD;
use afd_fleet_lifecycle::FleetStatus;
use afd_fleet_runtime::definition::DEFINITION_SCHEMA_VERSION;
use afd_fleet_runtime::{Definition, FleetName};
use serde_json::json;
use sqlx::Row as _;
use sqlx::postgres::PgRow;

use super::ChannelId;
use crate::error::{self, COLUMN_FLEET, Result, row_unreadable, stored_fleet, stored_status};
use crate::{Ingress, sql};

/// The instructions every resident runs, before its placeholder is filled.
const INSTRUCTIONS_TEMPLATE: &str = include_str!("resident.md");
/// Where the channel's identifier goes in [`INSTRUCTIONS_TEMPLATE`].
const PLACEHOLDER_CHANNEL: &str = "{channel_id}";

/// What every resident's name opens with.
const NAME_PREFIX: &str = "slack-channel-";

/// The resident's display description.
const DESCRIPTION: &str = "Read-only assistant resident in one Slack channel. Answers mentions \
from what it has learned about this channel; never acts unattended.";
/// A one-dollar day, in the exact units every budget reader uses.
const DAILY_NANOS: i64 = NANOS_PER_USD;

/// How a binding row says it names a channel's resident. The table stores the
/// spelling; this is the one place it is written (RULE STS).
pub const KIND_RESIDENT: &str = "resident";

/// Statement names, for the context a query failure carries.
const CONTEXT_RESIDENT: &str = "resolve channel resident";
/// See [`CONTEXT_RESIDENT`].
const CONTEXT_BIND: &str = "bind channel resident";
/// See [`CONTEXT_RESIDENT`].
const CONTEXT_NAMED: &str = "resolve fleet by name";

/// A channel's resident, as an install writes it.
#[derive(Debug, Clone, PartialEq)]
pub struct Resident {
    /// `slack-channel-<team>-<channel>`, lower-cased.
    pub name: FleetName,
    /// The complete native setup, never anything a person wrote.
    pub definition: Definition,
    /// The definition's configuration as the fleet row stores it, which is
    /// how a fleet already holding the name is recognised as this resident.
    /// A fleet a person saved under this name with the same policy reads as
    /// the resident too: the check tells a resident from a fleet with a policy
    /// of its own.
    pub config_json: String,
}

/// What a workspace holds under a resident's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Named {
    /// The resident itself: installed by a concurrent first mention, or left
    /// unbound by a team that moved away and back.
    Resident(BoundResident),
    /// A fleet somebody installed under that name, with a configuration of its
    /// own. Never adopted: the resident's policy is the daemon's to write, and
    /// adopting would hand unaddressed mentions to whatever that fleet may do.
    Other,
}

impl Resident {
    /// The resident of `channel` in the chat team `team`.
    ///
    /// `None` when the team's identifier cannot form a fleet name, which a
    /// Slack team identifier always can: it is upper-case letters and digits.
    #[must_use]
    pub fn for_channel(team: &str, channel: &ChannelId) -> Option<Self> {
        let name = FleetName::parse(
            &format!("{NAME_PREFIX}{team}-{}", channel.as_str()).to_ascii_lowercase(),
        )
        .ok()?;
        let definition = Definition::parse(&definition(&name, channel).to_string()).ok()?;
        let config_json = serde_json::to_string(definition.config()).ok()?;
        Some(Self {
            name,
            definition,
            config_json,
        })
    }
}

/// The resident's setup: its name, prose for `channel`, and the fixed policy.
fn definition(name: &FleetName, channel: &ChannelId) -> serde_json::Value {
    json!({
        "schema_version": DEFINITION_SCHEMA_VERSION,
        "description": DESCRIPTION,
        "instructions_md": INSTRUCTIONS_TEMPLATE.replace(PLACEHOLDER_CHANNEL, channel.as_str()),
        "config": {
            "name": name.as_str(),
            "x-agentsfleet": {
                // No trigger: ingress addresses a resident directly, by the
                // channel's binding row, never by matching what it declares.
                "triggers": [],
                "tools": [],
                "budget": {"daily_nanos": DAILY_NANOS.to_string()},
            },
        },
    })
}

/// A channel's resident, as its binding reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundResident {
    /// The resident fleet.
    pub fleet: Uuid7,
    /// Whether it can run, which the binding alone does not say.
    pub status: FleetStatus,
}

impl Ingress {
    /// The fleet bound as `channel`'s resident in `team` on `provider`, if one
    /// is bound in `workspace`.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and a row this build cannot
    /// read.
    pub async fn resident(
        &self,
        workspace: &Uuid7,
        provider: &str,
        team: &str,
        channel: &ChannelId,
    ) -> Result<Option<BoundResident>> {
        let mut connection = self.database.acquire().await?;
        let found = sqlx::query(sql::SELECT_RESIDENT)
            .bind(provider)
            .bind(team)
            .bind(channel.as_str())
            .bind(KIND_RESIDENT)
            .bind(workspace.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_RESIDENT))?;
        found.map(|row| bound(&row, CONTEXT_RESIDENT)).transpose()
    }

    /// Binds `fleet` as `channel`'s resident unless `workspace` already has
    /// one bound, and answers whichever fleet is bound there.
    ///
    /// # Errors
    /// Reports entropy or an instant that would not mint the binding's id, a
    /// datastore that would not answer, and a row this build cannot read.
    pub async fn bind_resident(
        &self,
        workspace: &Uuid7,
        provider: &str,
        team: &str,
        channel: &ChannelId,
        fleet: &Uuid7,
        now: UnixMillis,
    ) -> Result<Uuid7> {
        let id = self.entropy.uuid7(now)?;
        let mut connection = self.database.acquire().await?;
        let bound: Option<String> = sqlx::query_scalar(sql::INSERT_RESIDENT)
            .bind(id.as_str())
            .bind(provider)
            .bind(team)
            .bind(channel.as_str())
            .bind(fleet.as_str())
            .bind(KIND_RESIDENT)
            .bind(now.as_millis())
            .bind(workspace.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_BIND))?;
        drop(connection);
        match bound {
            Some(bound) => stored_fleet(&bound),
            // The insert waited on a concurrent one and did nothing, and its
            // own read was taken before that commit; a fresh statement sees it.
            None => self
                .resident(workspace, provider, team, channel)
                .await?
                .map(|bound| bound.fleet)
                .ok_or_else(|| row_unreadable(COLUMN_FLEET)),
        }
    }

    /// What `workspace` holds under `resident`'s name, if anything: read after
    /// an install lost the name, to tell the resident a concurrent first
    /// mention installed from a fleet somebody else named that way.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and a row this build cannot
    /// read.
    pub async fn resident_named(
        &self,
        workspace: &Uuid7,
        resident: &Resident,
    ) -> Result<Option<Named>> {
        let mut connection = self.database.acquire().await?;
        let found = sqlx::query(sql::SELECT_RESIDENT_NAMED)
            .bind(workspace.as_str())
            .bind(resident.name.as_str())
            .bind(&resident.config_json)
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_NAMED))?;
        found
            .map(|row| {
                let unreadable = error::query(CONTEXT_NAMED);
                let is_resident: bool = row.try_get(2).map_err(&unreadable)?;
                if !is_resident {
                    return Ok(Named::Other);
                }
                bound(&row, CONTEXT_NAMED).map(Named::Resident)
            })
            .transpose()
    }
}

/// The resident a row's first two columns name: its fleet and that fleet's
/// status, the shape both [`SELECT_RESIDENT`](sql::SELECT_RESIDENT) and
/// [`SELECT_RESIDENT_NAMED`](sql::SELECT_RESIDENT_NAMED) lead with.
fn bound(row: &PgRow, context: &'static str) -> Result<BoundResident> {
    let unreadable = error::query(context);
    let fleet: String = row.try_get(0).map_err(&unreadable)?;
    let status: String = row.try_get(1).map_err(&unreadable)?;
    Ok(BoundResident {
        fleet: stored_fleet(&fleet)?,
        status: stored_status(&status)?,
    })
}

#[cfg(test)]
#[path = "resident_tests.rs"]
mod tests;
