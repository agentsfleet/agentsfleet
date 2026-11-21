//! The ledger is staged on the caller's transaction, with no queue append inside it.

use super::super::{Creations, stored::Stored};
use super::Prepared;
use crate::{
    Result,
    types::{InitialStart, StartOutcome, StartState},
};
use af_wire::turn::TurnKind;
use afd_turns::{IncomingTurn, Key, Producer, Reply};
use sqlx::PgConnection;

impl Creations {
    pub(in crate::creation) async fn prepare_initial<'a>(
        &self,
        row: &'a Stored,
        key: &'a str,
        payload: Option<&'a str>,
    ) -> Result<Prepared<'a>> {
        // System setup has no person to attribute a run to and always stores
        // `Start::None`, so only a person's Save writes the turn.
        let (Some(payload), Some(actor)) = (payload, &row.accepted.context.creator_id) else {
            return Ok(Prepared::None);
        };
        let turn = IncomingTurn {
            producer: Producer::Steer,
            key: Key::Repeated(key),
            fleet: &row.id,
            workspace: &row.workspace_id,
            actor: actor.as_str(),
            event_type: TurnKind::Chat,
            request_json: payload,
            reply: Reply::None,
        };
        match self.turns.prepare(turn).await {
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
            Prepared::Ready(prepared) => match self.turns.stage(connection, prepared).await {
                Ok(admitted) => StartOutcome::RunNow {
                    state: StartState::Pending,
                    turn_id: Some(admitted.stored.id.into()),
                    error_code: None,
                },
                Err(refused) if refused.is_over_capacity() => blocked(refused.code().to_string()),
                Err(source) => return Err(source.into()),
            },
        };
        Ok(initial)
    }
}

const fn blocked(code: String) -> StartOutcome {
    StartOutcome::RunNow {
        state: StartState::Blocked,
        turn_id: None,
        error_code: Some(code),
    }
}
