//! The fixed text a mention earns when routing picks no fleet, owed to the
//! thread it was asked in.
//!
//! No model runs. Each kind is fixed text naming the fleets it is about and the
//! next step a person can take. It is owed through the ledger an answer is owed
//! through (`afd_outbound::obligation`), by the channel's resident, under a key
//! derived from the chat event's own id, so a retried delivery owes it once.

use afd_connector::Provider;
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_outbound::obligation::{self, Delivery};

use super::{Notice, Subscriber};
use crate::Ingress;
use crate::error::Result;

/// What a notice's obligation key ends in, so it can never share a key with an
/// answer the same resident owes for an admitted event.
const KEY_SUFFIX: &str = "notice";

/// What separates the fleet names a notice lists.
const NAME_SEPARATOR: &str = ", ";

/// The obligation key a notice for chat event `event_id` in `team` is owed
/// under: `<team>:<event_id>:notice`.
#[must_use]
pub fn notice_key(team: &str, event_id: &str) -> String {
    format!("{team}:{event_id}:{KEY_SUFFIX}")
}

/// The fixed text `notice` answers with.
#[must_use]
pub fn notice_text(notice: &Notice<'_>) -> String {
    match notice {
        Notice::Ambiguous { fleets } => format!(
            "More than one fleet here answers to that name ({}), so I can't tell which you mean. \
             Mention one by its exact name.",
            names(fleets)
        ),
        Notice::Choose { fleets } => format!(
            "Several fleets answer in this channel: {}. Start your message with the name of \
             the one you mean.",
            names(fleets)
        ),
        Notice::AddressIt { fleets } => format!(
            "The fleets attached here answer only when named: {}. Start your message with one \
             of those names.",
            names(fleets)
        ),
        Notice::Paused { fleet } => format!(
            "{} is paused and cannot answer. A workspace member can resume it with \
             `agentsfleet resume {}`.",
            fleet.name,
            fleet.fleet.as_str()
        ),
    }
}

/// The fleets' names, in the order routing listed them.
fn names(fleets: &[&Subscriber]) -> String {
    fleets
        .iter()
        .map(|fleet| fleet.name.as_str())
        .collect::<Vec<_>>()
        .join(NAME_SEPARATOR)
}

/// One notice, as the ledger owes it.
#[derive(Debug, Clone, Copy)]
pub struct NoticeOwed<'a> {
    /// The channel's resident, which owns the notice.
    pub resident: &'a Uuid7,
    /// The workspace whose grant posts it.
    pub workspace: &'a Uuid7,
    /// The connector that carries it back.
    pub provider: Provider,
    /// [`notice_key`] for the mention's event.
    pub key: &'a str,
    /// The thread it is posted in, as the connector's poster reads it.
    pub address: &'a str,
    /// [`notice_text`].
    pub text: &'a str,
}

impl Ingress {
    /// Owes one notice to the mention's thread.
    ///
    /// Answers `true` when this call wrote the obligation, and `false` for a
    /// retried delivery whose notice is already owed.
    ///
    /// # Errors
    /// Reports entropy or an instant that would not mint the row's id, and a
    /// ledger that would not record it.
    pub async fn owe_notice(&self, owed: NoticeOwed<'_>, now: UnixMillis) -> Result<bool> {
        let row = Uuid7::encode(now, self.entropy.uuid_randomness()?)?;
        let mut connection = self.database.acquire().await?;
        let written = obligation::owe(
            connection.as_mut(),
            row.as_str(),
            Delivery {
                fleet_id: owed.resident.as_str(),
                workspace_id: owed.workspace.as_str(),
                provider: owed.provider,
                destination: owed.address,
                event_id: owed.key,
                answer: owed.text,
            },
            now,
        )
        .await?;
        Ok(written)
    }
}

#[cfg(test)]
#[path = "notice_tests.rs"]
mod tests;
