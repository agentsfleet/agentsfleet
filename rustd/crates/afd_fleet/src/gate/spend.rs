//! Spending a human's answer: may THIS lease mint a write-scoped token?
//!
//! The sibling read in [`super::grants`] answers whether a lease may author a
//! branch, and folds every refusal into `None` because its caller has one thing
//! to say. This one is asked by a runner that must be TOLD which refusal it
//! met — an unapproved gate, a reach that drifted, and an exhausted allowance
//! are three remedies and three registry codes — so the verdict is a type with
//! three ways to say no.
//!
//! # Confirm and spend are one decision, in two statements
//!
//! The row is read `FOR UPDATE` and the spend commits in the same transaction,
//! so two concurrent mints on one approval cannot both read the same
//! `spend_count`. The `UPDATE` then restates the conditions the read checked:
//! if the row moved between them it updates nothing, and zero affected rows is
//! the exhausted answer rather than a spend nobody counted.
//!
//! # Why the drift check reads a row and not the config
//!
//! Gate rules and the repository binding both ride `config_json`, which a
//! `fleet:write` PATCH can change under the same scope that wakes the fleet. So
//! "what the human approved" is read from the `stated_binding` this daemon
//! WROTE when it raised the card, never from anything editable — and a fleet
//! that added a repository since the answer is refused as drift rather than
//! writing to one nobody was asked about.

use afd_core::id::Uuid7;
use afd_fleet_runtime::config::RepositoryBinding;
use sqlx::{Acquire as _, Row as _};

use crate::error::{Result, query};
use crate::gate::decision::Status;
use crate::gate::detail::KIND_REPOSITORY_WRITE;
use crate::gate::store::Gates;
use crate::sql;

/// Statement name, for the context a query failure carries.
const CONTEXT_SPEND: &str = "write gate spend";

/// The event a diagnostic names when a write mint is refused.
const EVENT_WRITE_REFUSED: &str = "write_mint_refused";

/// Whether this lease may mint a write-scoped token, and if not, why.
///
/// Three refusals rather than one, because a runner acts differently on each:
/// an unapproved gate waits for a human, drift needs the fleet's declaration
/// and the human's answer brought back into agreement, and an exhausted
/// allowance is a run that has already asked as often as it was permitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteApproval {
    /// Approved, unchanged, and one request has just been spent against it.
    Approved,
    /// No repository-write gate for this event, one not approved, or one
    /// answered after its own deadline had passed.
    Unapproved,
    /// Approved for a reach the fleet no longer declares.
    BindingDrift,
    /// The approved allowance is used up.
    Exhausted,
}

/// The gate row, as the locking read projects it.
///
/// Every field is `Option` where the column is nullable, so a row that never
/// carried a value cannot be read as one that carries zero — the distinction
/// between "no allowance was recorded" and "the allowance is spent".
#[derive(Debug)]
struct Locked {
    /// The gate, for the spend to name.
    id: String,
    /// What a human answered.
    status: String,
    /// The reach they were shown.
    stated_binding: Option<String>,
    /// When the question would have lapsed.
    timeout_at: i64,
    /// When the answer landed. `None` is a gate nobody has answered.
    answered_at: Option<i64>,
    /// How many requests have been spent.
    spend_count: Option<i64>,
    /// How many were permitted.
    spend_ceiling: Option<i64>,
}

impl Locked {
    /// Reads the row the locking statement returned.
    fn read(row: &sqlx::postgres::PgRow) -> Result<Self> {
        Ok(Self {
            id: row.try_get(0).map_err(query(CONTEXT_SPEND))?,
            status: row.try_get(1).map_err(query(CONTEXT_SPEND))?,
            stated_binding: row.try_get(2).map_err(query(CONTEXT_SPEND))?,
            timeout_at: row.try_get(3).map_err(query(CONTEXT_SPEND))?,
            answered_at: row.try_get(4).map_err(query(CONTEXT_SPEND))?,
            spend_count: row.try_get(5).map_err(query(CONTEXT_SPEND))?,
            spend_ceiling: row.try_get(6).map_err(query(CONTEXT_SPEND))?,
        })
    }

    /// The gate this row may be spent from, or the refusal it earns.
    ///
    /// Pure, and separate from the transaction for that reason: every refusal
    /// this verb can produce is decided here, so all of them are provable
    /// against a row literal rather than against a seeded datastore.
    ///
    /// The order is the order things become false in. Status first because an
    /// unanswered gate has nothing else worth reading; the deadline next
    /// because an answer that arrived too late is not an answer; the reach
    /// after that because a stale approval must not be spent even when the
    /// allowance is intact; and the allowance last, since it is the only check
    /// whose failure is temporary.
    fn examine(&self, declared: &RepositoryBinding) -> std::result::Result<&str, WriteApproval> {
        if self.status != Status::Approved.as_str() {
            return Err(WriteApproval::Unapproved);
        }
        // An answer stamped after the card lapsed is a human answering a
        // question that had already expired. `None` is a row that says it is
        // approved and records no answer, which is a row this daemon did not
        // write the way it writes them.
        match self.answered_at {
            Some(answered) if answered <= self.timeout_at => {}
            _lapsed_or_unstamped => return Err(WriteApproval::Unapproved),
        }
        // An unrecorded reach authorises nothing: there is nothing to compare
        // the fleet's current declaration against, and unknown reach must never
        // be the permissive branch.
        let Some(stated) = self.stated_binding.as_deref() else {
            return Err(WriteApproval::BindingDrift);
        };
        if !declared.matches_recorded(stated) {
            return Err(WriteApproval::BindingDrift);
        }
        // Both `None` means the row was raised without an allowance at all —
        // not an allowance of zero, and not one to spend.
        let (Some(spent), Some(ceiling)) = (self.spend_count, self.spend_ceiling) else {
            return Err(WriteApproval::Unapproved);
        };
        if spent >= ceiling {
            return Err(WriteApproval::Exhausted);
        }
        Ok(&self.id)
    }
}

impl Gates {
    /// Confirms and spends one write-mint request against this event's gate.
    ///
    /// Runs BEFORE any vault read and before any provider is dialled, so a
    /// refused mint has touched no credential bytes. A datastore that will not
    /// answer fails the request rather than the check — never open.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. Every refusal is a
    /// [`WriteApproval`], not an error: a fleet without an approval is not a
    /// fault, and the runner is told which one it met.
    pub async fn reserve_write_approval(
        &self,
        fleet_id: &Uuid7,
        event_id: &str,
        declared: &RepositoryBinding,
    ) -> Result<WriteApproval> {
        let mut connection = self.database.acquire().await?;
        // The transaction is what makes the lock mean anything: it is held from
        // the read to the commit, and a drop rolls back — there is no reset to
        // forget and no path that leaves the row locked.
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_SPEND))?;

        let row = sqlx::query(sql::gate::LOCK_WRITE_GATE_FOR_MINT)
            .bind(fleet_id.as_str())
            .bind(event_id)
            .bind(KIND_REPOSITORY_WRITE)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(query(CONTEXT_SPEND))?;
        // No gate at all reads exactly as an unapproved one: in both cases no
        // human has said yes to this event.
        let Some(row) = row else {
            return Ok(refused(fleet_id, event_id, WriteApproval::Unapproved));
        };

        let locked = Locked::read(&row)?;
        let gate_id = match locked.examine(declared) {
            Ok(gate_id) => gate_id,
            Err(refusal) => return Ok(refused(fleet_id, event_id, refusal)),
        };

        let spent = sqlx::query(sql::gate::SPEND_WRITE_GATE_FOR_MINT)
            .bind(gate_id)
            .bind(Status::Approved.as_str())
            .execute(&mut *transaction)
            .await
            .map_err(query(CONTEXT_SPEND))?;
        if spent.rows_affected() == 0 {
            // The row moved between the read and the write. Under the lock that
            // is only possible for a row this transaction never really held,
            // and the safe reading of "the update matched nothing" is that
            // there was nothing left to spend.
            return Ok(refused(fleet_id, event_id, WriteApproval::Exhausted));
        }
        transaction.commit().await.map_err(query(CONTEXT_SPEND))?;
        Ok(WriteApproval::Approved)
    }
}

/// Records a refusal and hands it back.
///
/// One diagnostic for all four, carrying which one it was: an operator asking
/// why a run cannot write wants the answer in one line, and four log sites that
/// have to stay in agreement is how one of them ends up saying something else.
/// No token, no binding and no credential bytes appear here.
fn refused(fleet_id: &Uuid7, event_id: &str, verdict: WriteApproval) -> WriteApproval {
    tracing::warn!(
        event = EVENT_WRITE_REFUSED,
        fleet_id = fleet_id.as_str(),
        event_id,
        ?verdict,
        "a write-scoped mint was refused"
    );
    verdict
}

#[cfg(test)]
mod tests;
