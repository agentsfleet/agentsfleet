//! The grant an install writes already answered.
//!
//! # Why there is no card here
//!
//! A person installing a fleet has already answered the only question a grant
//! card asks. They chose the fleet, the bundle names the integration, and the
//! fleet's own binding names the repositories and the access level — so a
//! second per-install question adds no fact a person could act on, and a
//! per-EVENT question adds one card per model turn, which is the treadmill
//! this verb exists to end.
//!
//! What still stops a fleet is unchanged and deliberate: `budget.daily_dollars`
//! bounds the money, `agentsfleet grant revoke` is the manual stop, and the
//! App installation bounds which repositories a token can ever reach.
//!
//! # A re-install never un-revokes
//!
//! [`grant_sql::GRANT_AT_INSTALL`] conflicts onto `DO NOTHING`, so re-installing
//! a fleet whose grant a person revoked leaves the revocation standing and this
//! verb reports [`Requested::Denied`]. A revoke that the next install undid
//! would be a stop button with a timer on it.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_wire::grant::status;
use sqlx::Row as _;

use super::{Requested, Wanted};
use crate::grant::IntegrationGrants;
use crate::{Result, error, grant_sql};

/// Statement name, for the context a query failure carries.
const CONTEXT_INSTALL: &str = "grant.install";

/// A grant landed approved and nobody was asked.
const EVENT_AUTO_APPROVED: &str = "grant_auto_approved";

/// An install found a grant a person had already answered, and left it alone.
const EVENT_ALREADY_ANSWERED: &str = "grant_install_already_answered";

impl IntegrationGrants {
    /// Write the grant this install authorises, and raise no card.
    ///
    /// Takes no workspace identifier, unlike [`IntegrationGrants::request`]:
    /// nothing here writes into an inbox, so there is no card that could land
    /// in a workspace that does not own the fleet it names.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and an identifier that could
    /// not be minted. A grant that was already revoked is
    /// [`Requested::Denied`], not an error.
    pub async fn grant_at_install(
        &self,
        fleet: &Uuid7,
        wanted: Wanted<'_>,
        now: UnixMillis,
    ) -> Result<Requested> {
        let grant_id = self.mint(now)?;
        let mut connection = self.database().acquire().await?;
        let row = sqlx::query(grant_sql::GRANT_AT_INSTALL)
            .bind(grant_id.as_str())
            .bind(fleet.as_str())
            .bind(wanted.service)
            .bind(status::APPROVED)
            .bind(wanted.origin.reason())
            .bind(now.as_millis())
            .fetch_one(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_INSTALL))?;

        let standing: String = row.try_get(0).map_err(error::query(CONTEXT_INSTALL))?;
        let outcome = settle(&standing);
        report(outcome, fleet, wanted.service, &standing);
        Ok(outcome)
    }
}

/// The status that survived the write, as the answer a caller acts on.
///
/// An unrecognised spelling is [`Requested::Pending`] for the reason
/// [`super::settle`] gives it: waiting is the fail-safe direction, and an
/// unknown status read as a person's yes would mint against a grant nobody
/// granted.
fn settle(standing: &str) -> Requested {
    match standing {
        status::APPROVED => Requested::Approved,
        status::REVOKED => Requested::Denied,
        _ => Requested::Pending,
    }
}

/// Says what the install did, in the lines an operator counts.
fn report(outcome: Requested, fleet: &Uuid7, service: &str, standing: &str) {
    let fleet_id = fleet.as_str();
    match outcome {
        Requested::Approved => tracing::info!(
            event = EVENT_AUTO_APPROVED,
            fleet_id,
            service,
            "the install authorised this integration; no card was raised"
        ),
        Requested::Denied | Requested::Pending | Requested::Raised => tracing::info!(
            event = EVENT_ALREADY_ANSWERED,
            fleet_id,
            service,
            status = standing,
            "a grant already stood for this service; the install left it alone"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::settle;
    use crate::request::Requested;
    use afd_wire::grant::status;

    #[test]
    fn m202_001_an_install_reports_the_status_that_survived() {
        // The three the table can hold. `DO NOTHING` means the install may
        // find any of them, and the caller's next move differs for each.
        assert_eq!(settle(status::APPROVED), Requested::Approved);
        assert_eq!(settle(status::REVOKED), Requested::Denied);
        assert_eq!(settle(status::PENDING), Requested::Pending);
    }

    #[test]
    fn m202_001_an_unknown_status_is_never_read_as_a_yes() {
        // The fail-safe direction, and the one that matters: a spelling this
        // build has no arm for must not admit a mint. Pending refuses; denied
        // would also refuse but would tell an operator a person said no.
        for unknown in ["", "APPROVED", "granted", "approved "] {
            assert_eq!(settle(unknown), Requested::Pending, "{unknown:?}");
        }
    }
}
