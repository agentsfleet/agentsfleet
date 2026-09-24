//! A chat channel's resident: the fleet that answers a mention no attached
//! fleet takes, materialised on the first one.
//!
//! # The configuration is code, and the skill is prose
//!
//! What a resident may do comes from the `TRIGGER.md` built here — one `api`
//! trigger, no tools, no hosts, a small daily ceiling — and never from its
//! skill, which carries a name and prose only. So nothing a channel's members
//! say, and nothing the skill text says, can widen what the resident reaches
//! (RULE PRI). A person who needs more is told the command that attaches a
//! fleet which has it.
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
use afd_fleet_lifecycle::FleetStatus;
use afd_fleet_runtime::FleetName;
use sqlx::Row as _;

use super::ChannelId;
use crate::error::{self, COLUMN_FLEET, COLUMN_STATUS, Result, row_unreadable};
use crate::{Ingress, sql};

/// The skill every resident runs, before its placeholders are filled.
const SKILL_TEMPLATE: &str = include_str!("resident.md");
/// Where the resident's name goes in [`SKILL_TEMPLATE`].
const PLACEHOLDER_NAME: &str = "{name}";
/// Where the channel's identifier goes in [`SKILL_TEMPLATE`].
const PLACEHOLDER_CHANNEL: &str = "{channel_id}";

/// What every resident's name opens with.
const NAME_PREFIX: &str = "slack-channel-";

/// The daily ceiling a resident is held to, in dollars.
const DAILY_DOLLARS: &str = "1.0";

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resident {
    /// `slack-channel-<team>-<channel>`, lower-cased.
    pub name: FleetName,
    /// The skill, with the name and the channel filled in.
    pub skill_markdown: String,
    /// The policy, built here rather than read from anything a person wrote.
    pub trigger_markdown: String,
    /// [`Self::trigger_markdown`] as the fleet row stores it, which is how a
    /// fleet already holding the name is recognised as this resident.
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
        let skill_markdown = SKILL_TEMPLATE
            .replace(PLACEHOLDER_NAME, name.as_str())
            .replace(PLACEHOLDER_CHANNEL, channel.as_str());
        let trigger_markdown = format!(
            "---\nname: {}\nx-agentsfleet:\n  triggers:\n    - type: api\n  tools: []\n  budget:\n    daily_dollars: {DAILY_DOLLARS}\n---\n",
            name.as_str()
        );
        let config_json = afd_fleet_runtime::parse_trigger(&trigger_markdown)
            .ok()?
            .config_json()
            .to_owned();
        Some(Self {
            name,
            skill_markdown,
            trigger_markdown,
            config_json,
        })
    }
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
        found
            .map(|row| {
                let unreadable = error::query(CONTEXT_RESIDENT);
                let fleet: String = row.try_get(0).map_err(&unreadable)?;
                let status: String = row.try_get(1).map_err(&unreadable)?;
                Ok(BoundResident {
                    fleet: fleet_id(&fleet)?,
                    status: FleetStatus::parse(&status)
                        .ok_or_else(|| row_unreadable(COLUMN_STATUS))?,
                })
            })
            .transpose()
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
        let id = Uuid7::encode(now, self.entropy.uuid_randomness()?)?;
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
            Some(bound) => fleet_id(&bound),
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
                let fleet: String = row.try_get(0).map_err(&unreadable)?;
                let status: String = row.try_get(1).map_err(&unreadable)?;
                Ok(Named::Resident(BoundResident {
                    fleet: fleet_id(&fleet)?,
                    status: FleetStatus::parse(&status)
                        .ok_or_else(|| row_unreadable(COLUMN_STATUS))?,
                }))
            })
            .transpose()
    }
}

/// A fleet id as a row stores it.
fn fleet_id(stored: &str) -> Result<Uuid7> {
    Uuid7::parse(stored).map_err(|_shape| row_unreadable(COLUMN_FLEET))
}

#[cfg(test)]
#[path = "resident_tests.rs"]
mod tests;
