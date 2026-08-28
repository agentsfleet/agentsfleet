//! The operator's side of an integration grant: read the fleet's, take one back.
//!
//! The port of `integration_grants/workspace.zig`.
//!
//! # Why this is here and not in `afd_gate`
//!
//! `afd_gate::gate::grants` reads the same table and asks a different question.
//! A runner asks "may this fleet mint against this service", once per lease,
//! and the only answer it can act on is `approved` — absent, pending and
//! revoked are one word to it. An operator browses every row a fleet holds,
//! including the ones the runner is blind to, and answers them.
//!
//! That is the split [`crate::Inbox`] already makes over
//! `core.fleet_approval_gates`, and it is the same split for the same reason.
//! What the two sides share is one column's vocabulary, in
//! [`afd_wire::grant::status`], and nothing else — a second spelling of
//! `revoked` in either crate is a row one plane writes that the other stops
//! matching.
//!
//! This crate was already the grant table's writer before this module existed:
//! `sql::RESOLVE_GATE` moves the grant in the same statement that answers the
//! gate, because a crash between two statements would leave a gate saying yes
//! over a grant that never heard. The revoke below is the other direction of
//! that same authority.
//!
//! # Ownership is answered in SQL, twice, on purpose
//!
//! Whether the CALLER may act in the workspace is decided at the edge by a
//! layer. Whether the FLEET is in that workspace is decided here, because this
//! is the crate that can enforce it rather than trust it — and the revoke's own
//! statement re-answers it a second time in its join. See
//! [`sql::REVOKE_GRANT`] for why the redundancy stays.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_wire::grant::status;
use sqlx::Row as _;

use crate::sql;
use crate::{Result, error};

/// Statement names, for the context a query failure carries.
const CONTEXT_SCOPE: &str = "grant.fleet.scope";
const CONTEXT_LIST: &str = "grant.list";
const CONTEXT_REVOKE: &str = "grant.revoke";

/// One grant as the operator's list shows it.
///
/// Owned rather than borrowed: the rows outlive the connection they were read
/// through, and the HTTP edge borrows FROM these when it renders the wire shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantRow {
    /// The grant's own row id — what a revoke addresses it by.
    pub id: String,
    /// The third party the grant is about.
    pub service: String,
    /// Where the decision stands, from [`afd_wire::grant::status`].
    pub status: String,
    /// When the grant was raised.
    pub created_at: i64,
    /// When a person approved it, if one has.
    pub approved_at: Option<i64>,
    /// When a person took that back, if one has.
    pub revoked_at: Option<i64>,
    /// Why the row exists at all — `requested_reason` on the column.
    pub reason: String,
}

/// What a revoke did.
///
/// Three outcomes and not a `bool`, because two of them are refusals carrying
/// DIFFERENT registry codes and a caller cannot derive one from the other: a
/// fleet this workspace does not hold is not the same incident as a grant id
/// that names nothing, and an operator's remedy differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Revocation {
    /// The grant was usable and now is not.
    Revoked,
    /// No such grant on that fleet, or it was already revoked.
    ///
    /// One arm for both, matching the sentence the caller is told: the grant is
    /// unusable either way, and telling them apart would only invite a retry.
    GrantAbsent,
    /// The workspace holds no fleet by that id.
    FleetAbsent,
}

/// A workspace's view of the grants its fleets hold.
///
/// Postgres and nothing else. The gate inbox beside it needs a queue as well —
/// an approval CONTINUES the run it blocked — where a grant decision continues
/// nothing: revoking withdraws a standing permission that the next mint will
/// consult, so there is no parked event waiting on this answer.
#[derive(Debug, Clone)]
pub struct IntegrationGrants {
    database: Db,
}

impl IntegrationGrants {
    /// A grant surface over `database`.
    #[must_use]
    pub const fn new(database: Db) -> Self {
        Self { database }
    }

    /// Every grant `fleet` holds, newest first.
    ///
    /// `Ok(None)` means `workspace` holds no fleet by that id — which covers a
    /// fleet that never existed and one that is another workspace's, because
    /// those must be one answer to a caller probing identifiers. It is
    /// deliberately NOT the same as an empty list: a fleet declaring no
    /// mintable credential holds no grants and still exists.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn page(&self, workspace: &Uuid7, fleet: &Uuid7) -> Result<Option<Vec<GrantRow>>> {
        let mut connection = self.database.acquire().await?;
        if !holds(&mut connection, workspace, fleet).await? {
            return Ok(None);
        }

        let rows = sqlx::query(sql::SELECT_FLEET_GRANTS)
            .bind(fleet.as_str())
            .fetch_all(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_LIST))?;

        rows.iter()
            .map(read_grant)
            .collect::<Result<Vec<_>>>()
            .map(Some)
    }

    /// Takes one grant back.
    ///
    /// Idempotent in effect and honest in report: a second revoke changes no
    /// row — `status != 'revoked'` in the statement's predicate — and answers
    /// [`Revocation::GrantAbsent`], so a caller learns their request was
    /// already satisfied rather than being told it landed twice.
    ///
    /// The approval a revoked grant once had is left standing. `approved_at`
    /// is not cleared, so the row still records that a person said yes before
    /// somebody took it back — a history a cleared column would erase.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn revoke(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        grant: &Uuid7,
        now: UnixMillis,
    ) -> Result<Revocation> {
        let mut connection = self.database.acquire().await?;
        if !holds(&mut connection, workspace, fleet).await? {
            return Ok(Revocation::FleetAbsent);
        }

        let revoked = sqlx::query(sql::REVOKE_GRANT)
            .bind(status::REVOKED)
            .bind(now.as_millis())
            .bind(grant.as_str())
            .bind(fleet.as_str())
            .bind(workspace.as_str())
            .fetch_optional(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_REVOKE))?;

        Ok(match revoked {
            Some(_row) => Revocation::Revoked,
            None => Revocation::GrantAbsent,
        })
    }
}

/// Whether `workspace` holds `fleet` at all.
///
/// Both verbs open with it, on the connection they already hold, because both
/// must tell "no such fleet here" from their own absent row — and the two
/// carry different registry codes at the edge. A free function rather than a
/// method: it needs the connection and the two identifiers, and nothing else
/// the store owns.
///
/// The Zig reads `core.fleets.workspace_id` and compares it in the handler.
/// Asking the predicate instead is the same authorization decided in the place
/// that can enforce it, and it carries no row back to be compared wrongly.
async fn holds(
    connection: &mut sqlx::PgConnection,
    workspace: &Uuid7,
    fleet: &Uuid7,
) -> Result<bool> {
    sqlx::query(sql::SELECT_FLEET_IN_WORKSPACE)
        .bind(fleet.as_str())
        .bind(workspace.as_str())
        .fetch_optional(connection)
        .await
        .map(|row| row.is_some())
        .map_err(error::query(CONTEXT_SCOPE))
}

/// Reads one grant row, through one error context.
///
/// Positional, matching the statement's projection: naming the columns here as
/// well would be a second list to keep in step with the `SELECT`, and the two
/// drifting is a decode failure at runtime rather than a build failure.
fn read_grant(row: &sqlx::postgres::PgRow) -> Result<GrantRow> {
    let unreadable = error::query(CONTEXT_LIST);
    Ok(GrantRow {
        id: row.try_get(0).map_err(&unreadable)?,
        service: row.try_get(1).map_err(&unreadable)?,
        status: row.try_get(2).map_err(&unreadable)?,
        created_at: row.try_get(3).map_err(&unreadable)?,
        approved_at: row.try_get(4).map_err(&unreadable)?,
        revoked_at: row.try_get(5).map_err(&unreadable)?,
        reason: row.try_get(6).map_err(&unreadable)?,
    })
}

#[cfg(test)]
mod tests {
    use super::Revocation;

    #[test]
    fn a_revoke_reports_three_outcomes_that_no_two_of_which_collapse() {
        // The variants exist to carry two DIFFERENT registry codes out of one
        // verb, so an equality that held between any pair would be the bug:
        // the edge would answer one code where the store meant the other.
        assert_ne!(Revocation::Revoked, Revocation::GrantAbsent);
        assert_ne!(Revocation::Revoked, Revocation::FleetAbsent);
        assert_ne!(Revocation::GrantAbsent, Revocation::FleetAbsent);
    }
}
