//! Names what `admit_one.sql` decided, in refusal order, without a datastore.
//!
//! The statement returns facts and never a verdict, so the order below is one
//! typed function a unit test reads, and the SQL holds no outcome spellings.

use afd_fleet_lifecycle::FleetStatus;

use crate::Repeated;
use crate::admit::Ledger;
use crate::error::{ErrorKind, Result, query};

/// Context a malformed answer row carries.
const CONTEXT: &str = "read the admission answer";

/// The statement's one answer row.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct Answer {
    /// Whether a fleet with the id belongs to the caller's workspace.
    pub(crate) fleet_found: bool,
    /// The fleet's stored status, when found.
    pub(crate) status: Option<String>,
    /// Whether the fleet's head setup carries this build's policy stamp.
    pub(crate) setup_current: bool,
    /// Whether this statement wrote the stored row.
    pub(crate) inserted: bool,
    /// The stored row: this statement's, or the one the key already held.
    pub(crate) id: Option<String>,
    pub(crate) digest: Option<String>,
    pub(crate) fleet: Option<String>,
    pub(crate) created_at: Option<jiff_sqlx::Timestamp>,
    /// The fleet's live run, as `live_run` and `earlier_waiting` read it.
    #[sqlx(flatten)]
    pub(crate) run: RunRow,
}

/// The answer row's live-run columns.
#[derive(Debug, Default, sqlx::FromRow)]
pub(crate) struct RunRow {
    /// The live run's event; `None` when no lease holds the slot unexpired.
    pub(crate) run_event_id: Option<String>,
    pub(crate) run_takes_input: bool,
    pub(crate) same_conversation: bool,
    pub(crate) earlier_waiting: bool,
}

/// What the caller bound: the status a runnable admission needs, and the
/// deployment ceiling it was checked against.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Bound {
    pub(crate) runnable: Option<FleetStatus>,
    pub(crate) ceiling_open: bool,
    pub(crate) limit: u64,
}

impl Answer {
    /// The stored row, when the answer carries one.
    fn stored(&self) -> Option<Repeated> {
        Some(Repeated {
            id: self.id.clone()?,
            digest: self.digest.clone()?,
            fleet: self.fleet.clone()?,
            created_at: self.created_at?.to_jiff(),
        })
    }

    /// The live-run facts, as the ledger hands them to a producer.
    fn run(&self) -> crate::RunFacts {
        crate::RunFacts {
            live_run: self
                .run
                .run_event_id
                .clone()
                .map(|event_id| crate::LiveRun {
                    event_id,
                    takes_input: self.run.run_takes_input,
                    same_conversation: self.run.same_conversation,
                }),
            earlier_waiting: self.run.earlier_waiting,
        }
    }

    /// The fleet's status, refusing a spelling this build does not know.
    fn fleet_status(&self) -> Result<FleetStatus> {
        self.status
            .as_deref()
            .and_then(FleetStatus::parse)
            .ok_or_else(|| {
                query(CONTEXT)(sqlx::Error::ColumnDecode {
                    index: "status".to_owned(),
                    source: "a fleet status this build does not know".into(),
                })
            })
    }
}

/// The outcome, in order: fleet absent, repeat, not runnable, policy,
/// deployment ceiling, accepted.
///
/// # Errors
/// The refusal that applied first, or an unreadable answer: a status this
/// build cannot parse, or open gates that wrote no row.
pub(crate) fn decide(answer: &Answer, bound: Bound) -> Result<Ledger> {
    if !answer.fleet_found {
        return Err(ErrorKind::FleetAbsent.into());
    }
    let stored = answer.stored();
    if let Some(stored) = stored.clone()
        && !answer.inserted
    {
        return Ok(Ledger {
            stored,
            inserted: false,
            run: answer.run(),
        });
    }
    if let Some(required) = bound.runnable {
        let status = answer.fleet_status()?;
        if status != required {
            return Err(ErrorKind::NotRunnable { status }.into());
        }
    }
    if !answer.setup_current {
        return Err(ErrorKind::Policy.into());
    }
    if !bound.ceiling_open {
        return Err(ErrorKind::OverBudget { limit: bound.limit }.into());
    }
    stored
        .map(|stored| Ledger {
            stored,
            inserted: true,
            run: answer.run(),
        })
        .ok_or_else(|| query(CONTEXT)(sqlx::Error::RowNotFound))
}

#[cfg(test)]
mod tests;
