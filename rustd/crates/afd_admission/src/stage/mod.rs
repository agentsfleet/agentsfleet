//! Activation can commit its fleet transition and admission in one Postgres transaction.

mod insert;
mod policy;

use crate::{Admission, Admissions, Admitted, Key, Result};
use afd_core::{
    clock::{self, UnixMillis},
    id::Uuid7,
};
use sqlx::PgConnection;

/// An immutable admission whose external budget reads completed before taking locks.
#[derive(Debug)]
pub struct PreparedAdmission<'a> {
    admission: Admission<'a>,
    row_id: Uuid7,
    now: UnixMillis,
    estimate: u64,
}

impl Admissions {
    /// Reads queue and deployment ceilings before a caller opens its transaction.
    ///
    /// # Errors
    /// Preserves capacity, entropy and datastore failures without recording acceptance.
    pub async fn prepare<'a>(&self, admission: Admission<'a>) -> Result<PreparedAdmission<'a>> {
        self.refuse_over_fleet_budget(admission.fleet).await?;
        let now = clock::now();
        Ok(PreparedAdmission {
            admission,
            row_id: self.entropy.uuid7(now)?,
            estimate: self.deployment_estimate(now).await?,
            now,
        })
    }

    /// Records the prepared admission on the caller's transaction, without queue I/O.
    ///
    /// The caller must commit before confirming delivery. A rollback removes both
    /// the admission and its surrounding activation. `confirm` appends that same
    /// producer identity and answers accepted only after saving its receipt.
    ///
    /// # Errors
    /// Preserves a failed statement or the deployment ceiling's refusal.
    pub async fn stage(
        &self,
        connection: &mut PgConnection,
        prepared: PreparedAdmission<'_>,
    ) -> Result<Admitted> {
        let key = match prepared.admission.key {
            Key::Repeated(key) => key,
            Key::Unrepeatable => prepared.row_id.as_str(),
        };
        let ledger = self
            .insert_on(
                connection,
                &prepared.row_id,
                &prepared.admission,
                key,
                &prepared.admission.payload_digest(),
                prepared.now,
                prepared.estimate,
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
