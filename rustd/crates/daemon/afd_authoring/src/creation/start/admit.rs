//! The ledger is staged on the caller's transaction, with no queue append inside it.

use super::super::{Creations, stored::Stored};
use super::Prepared;
use crate::{
    Result,
    types::{InitialStart, StartOutcome, StartState},
};
use afd_admission::{Admission, Key, Producer, Reply};
use afd_wire::event::EventType;
use sqlx::PgConnection;

impl Creations {
    pub(in crate::creation) async fn prepare_initial<'a>(
        &self,
        row: &'a Stored,
        key: &'a str,
        payload: Option<&'a str>,
    ) -> Result<Prepared<'a>> {
        // System setup has no person to attribute a run to and always stores
        // `Start::None`, so only a person's Save reaches the admission.
        let (Some(payload), Some(actor)) = (payload, &row.accepted.context.creator_id) else {
            return Ok(Prepared::None);
        };
        let admission = Admission {
            producer: Producer::Steer,
            key: Key::Repeated(key),
            fleet: row.id.as_str(),
            workspace: row.workspace_id.as_str(),
            actor: actor.as_str(),
            event_type: EventType::Chat,
            request_json: payload,
            reply: Reply::None,
        };
        match self.admissions.prepare(admission).await {
            Ok(prepared) => Ok(Prepared::Ready(prepared)),
            Err(refused) if refused.is_over_capacity() => {
                Ok(Prepared::Blocked(refused.code().to_string()))
            }
            Err(source) => Err(source.into()),
        }
    }

    pub(in crate::creation) async fn stage_initial(
        &self,
        connection: &mut PgConnection,
        mut initial: InitialStart,
        prepared: Prepared<'_>,
    ) -> Result<InitialStart> {
        initial.outcome = match prepared {
            Prepared::None => StartOutcome::None,
            Prepared::Blocked(code) => blocked(code),
            Prepared::Ready(prepared) => match self.admissions.stage(connection, prepared).await {
                Ok(admitted) => StartOutcome::RunNow {
                    state: StartState::Pending,
                    event_id: Some(admitted.stored.id),
                    error_code: None,
                },
                Err(refused) if refused.is_over_capacity() => blocked(refused.code().to_string()),
                Err(source) => return Err(source.into()),
            },
        };
        Ok(initial)
    }
}

fn blocked(code: String) -> StartOutcome {
    StartOutcome::RunNow {
        state: StartState::Blocked,
        event_id: None,
        error_code: Some(code),
    }
}
