//! The fixed text a mention earns when routing picks no fleet, owed to the
//! thread it was asked in.
//!
//! No model runs. Each kind is fixed text naming the fleets it is about and the
//! next step a person can take. It is owed through the ledger an answer is owed
//! through (`afd_outbound::obligation`), by a stable channel notice owner, under
//! its own minted id. A provider's retry of the same mention owes it again.

use af_core::id::Uuid7;
use afd_connector::Provider;
use afd_outbound::obligation::{self, Delivery};
use jiff::Timestamp;

use super::{Notice, Subscriber};
use crate::Ingress;
use crate::error::Result;

/// What separates the fleet names a notice lists.
const NAME_SEPARATOR: &str = ", ";

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
    /// The stable channel carrier, which owns only this notice.
    pub owner: &'a Uuid7,
    /// The workspace whose grant posts it.
    pub workspace: &'a Uuid7,
    /// The connector that carries it back.
    pub provider: Provider,
    /// The thread it is posted in, as the connector's poster reads it.
    pub address: &'a str,
    /// [`notice_text`].
    pub text: &'a str,
}

impl Ingress {
    /// Owes one notice to the mention's thread, and answers the id it is owed
    /// under. `now` mints that id; the ledger stamps the row on the database
    /// clock.
    ///
    /// # Errors
    /// Reports entropy or an instant that would not mint the row's id, and a
    /// ledger that would not record it.
    pub async fn owe_notice(&self, owed: NoticeOwed<'_>, now: Timestamp) -> Result<Uuid7> {
        let row = self.entropy.uuid7(now)?;
        // Unpinged: the notice is claimable at once, and the outbound claim
        // pass takes it within one pass.
        let token = self.entropy.uuid7(now)?;
        let mut connection = self.database.acquire().await?;
        obligation::owe(
            connection.as_mut(),
            &row,
            token.as_str(),
            Delivery {
                fleet_id: owed.owner,
                workspace_id: owed.workspace,
                provider: owed.provider,
                destination: owed.address,
                event_id: &row,
                answer: owed.text,
            },
        )
        .await?;
        Ok(row)
    }
}

#[cfg(test)]
#[path = "notice_tests.rs"]
mod tests;
