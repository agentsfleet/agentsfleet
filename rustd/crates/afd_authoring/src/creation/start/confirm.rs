//! Accepted means a durable ledger row, successful append and saved receipt.

use super::super::{ACTIVE, Creations};
use crate::{
    Access, Result,
    types::{StartOutcome, StartState},
};
use afd_admission::Producer;
use afd_core::id::Uuid7;

/// An admitted initial run whose append or receipt could not be confirmed yet.
const EVENT_UNCONFIRMED: &str = "fleet_initial_run_unconfirmed";

impl Creations {
    pub(in crate::creation) async fn confirm_initial(
        &self,
        access: Access<'_>,
        fleet: &Uuid7,
    ) -> Result<()> {
        let (mut row, _) = self.snapshot(access, fleet).await?;
        if row.status != ACTIVE {
            return Ok(());
        }
        let StartOutcome::RunNow {
            state: StartState::Pending,
            event_id: Some(expected),
            ..
        } = &row.initial_start.outcome
        else {
            return Ok(());
        };
        let confirmation = match self
            .admissions
            .confirm(Producer::Steer, &super::key(fleet))
            .await
        {
            Ok(Some(confirmation)) => confirmation,
            Ok(None) => return Ok(()),
            Err(unconfirmed) => {
                // The run is durably admitted; an unsaved receipt leaves it
                // pending exactly as a refused append does, and a same-key
                // retry or the replay sweep confirms that same event.
                let (event, error_code) = (EVENT_UNCONFIRMED, unconfirmed.code());
                tracing::warn!(event, fleet_id = %fleet, error_code = error_code.as_str());
                return Ok(());
            }
        };
        if !confirmation.accepted || confirmation.event_id != *expected {
            return Ok(());
        }
        row.initial_start.outcome = StartOutcome::RunNow {
            state: StartState::Accepted,
            event_id: Some(confirmation.event_id),
            error_code: None,
        };
        let mut connection = self.setup.drafts.database.acquire().await?;
        super::store(&mut connection, access, fleet, &row.initial_start).await
    }
}
