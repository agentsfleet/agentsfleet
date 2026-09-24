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
//! # Its memory is the channel's
//!
//! Memory is keyed by fleet. One fleet per channel means every thread in the
//! channel shares one memory, and no other channel's resident can read it.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_fleet_runtime::FleetName;

use super::ChannelId;
use crate::error::{self, COLUMN_FLEET, Result, row_unreadable};
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
        Some(Self {
            name,
            skill_markdown,
            trigger_markdown,
        })
    }
}

impl Ingress {
    /// The fleet bound as `channel`'s resident in `team` on `provider`, if one
    /// is.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and a row this build cannot
    /// read.
    pub async fn resident(
        &self,
        provider: &str,
        team: &str,
        channel: &ChannelId,
    ) -> Result<Option<Uuid7>> {
        let mut connection = self.database.acquire().await?;
        let found: Option<String> = sqlx::query_scalar(sql::SELECT_RESIDENT)
            .bind(provider)
            .bind(team)
            .bind(channel.as_str())
            .bind(KIND_RESIDENT)
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_RESIDENT))?;
        found.as_deref().map(fleet_id).transpose()
    }

    /// Binds `fleet` as `channel`'s resident unless one already is, and
    /// answers whichever fleet is bound.
    ///
    /// # Errors
    /// Reports entropy or an instant that would not mint the binding's id, a
    /// datastore that would not answer, and a row this build cannot read.
    pub async fn bind_resident(
        &self,
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
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_BIND))?;
        drop(connection);
        match bound {
            Some(bound) => fleet_id(&bound),
            // The insert waited on a concurrent one and did nothing, and its
            // own read was taken before that commit; a fresh statement sees it.
            None => self
                .resident(provider, team, channel)
                .await?
                .ok_or_else(|| row_unreadable(COLUMN_FLEET)),
        }
    }

    /// The fleet `workspace` holds under `name`, if any: the resident a
    /// concurrent first mention installed, found after this one's install lost
    /// the race for the name.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and a row this build cannot
    /// read.
    pub async fn fleet_named(&self, workspace: &Uuid7, name: &FleetName) -> Result<Option<Uuid7>> {
        let mut connection = self.database.acquire().await?;
        let found: Option<String> = sqlx::query_scalar(sql::SELECT_FLEET_NAMED)
            .bind(workspace.as_str())
            .bind(name.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_NAMED))?;
        found.as_deref().map(fleet_id).transpose()
    }
}

/// A fleet id as a row stores it.
fn fleet_id(stored: &str) -> Result<Uuid7> {
    Uuid7::parse(stored).map_err(|_shape| row_unreadable(COLUMN_FLEET))
}

#[cfg(test)]
#[path = "resident_tests.rs"]
mod tests;
