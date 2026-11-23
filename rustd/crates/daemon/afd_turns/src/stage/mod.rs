//! Activation can commit its fleet transition and turn in one Postgres transaction.

pub(crate) mod insert;
mod verdict;

use crate::{Admitted, IncomingTurn, Key, Result, Turns};
use af_core::{clock, id::Uuid7};
use sqlx::PgConnection;

/// An immutable turn whose external budget reads completed before taking locks.
#[derive(Debug)]
pub struct PreparedTurn<'a> {
    turn: IncomingTurn<'a>,
    row_id: Uuid7,
    estimate: u64,
}

impl Turns {
    /// Reads queue and deployment ceilings before a caller opens its transaction.
    ///
    /// # Errors
    /// Preserves capacity, entropy and datastore failures without recording acceptance.
    pub async fn prepare<'a>(&self, turn: IncomingTurn<'a>) -> Result<PreparedTurn<'a>> {
        let now = clock::now();
        Ok(PreparedTurn {
            turn,
            row_id: self.entropy.uuid7(now)?,
            estimate: self.deployment_estimate(now).await,
        })
    }

    /// Records the prepared turn on the caller's transaction, without queue I/O.
    ///
    /// The caller must commit before confirming acceptance. A rollback removes both
    /// the turn and its surrounding activation. `confirm` reads the saved
    /// producer identity and rings the wake subject after the commit.
    ///
    /// # Errors
    /// Preserves a failed statement, a fleet outside the caller's workspace,
    /// a setup needing review, or the deployment ceiling's refusal.
    pub async fn stage(
        &self,
        connection: &mut PgConnection,
        prepared: PreparedTurn<'_>,
    ) -> Result<Admitted> {
        let key = match prepared.turn.key {
            Key::Repeated(key) => key,
            Key::Unrepeatable => prepared.row_id.as_str(),
        };
        let ledger = self
            .insert_on(
                connection,
                &prepared.row_id,
                &prepared.turn,
                key,
                &prepared.turn.payload_digest(),
                insert::Gates {
                    estimate: prepared.estimate,
                    runnable: None,
                },
            )
            .await?;
        if ledger.inserted {
            // A later rollback may overcount until the next sample. Counting
            // here keeps concurrent staged inserts from undercounting capacity.
            self.ceiling.admitted();
        }
        Ok(Admitted {
            replayed: !ledger.inserted,
            stored: ledger.stored,
        })
    }
}
