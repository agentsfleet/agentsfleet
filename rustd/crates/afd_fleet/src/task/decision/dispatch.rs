//! One fresh authority check and one native credential-issuance commit.

use std::borrow::Cow;

use afd_crypto::secret::SecretString;
use afd_wire::task::{
    DecisionCallState, DecisionDispatch, DecisionReservationInput, DecisionReservationResult,
};
use sqlx::PgConnection;
use zeroize::Zeroizing;

use super::{
    ReservedDecision, budget,
    fund::Coverage,
    replay,
    row::{self, Call, Lookup, Payload},
    sql, verify,
};
use crate::{
    Result,
    error::query,
    task::{TaskLease, TaskRefusal, Tasks, transaction},
};

enum Emission {
    Dispatch(ReservedDecision),
    Existing(Box<Call>),
}

pub(super) struct Issuance<'a> {
    pub tasks: &'a Tasks,
    pub lease: TaskLease<'a>,
    pub input: &'a garde::Valid<DecisionReservationInput>,
    pub call: &'a Call,
    pub coverage: Coverage,
    pub key: &'a SecretString,
}

impl Issuance<'_> {
    pub(super) async fn commit(&self) -> Result<ReservedDecision> {
        let mut connection = transaction::acquire(self.tasks.leases.pool()).await?;
        let result = self.on(&mut connection).await;
        transaction::recycle(connection);
        match result? {
            Emission::Dispatch(packet) => Ok(packet),
            Emission::Existing(call) => replay::response(self.tasks, *call).await,
        }
    }

    async fn on(&self, connection: &mut PgConnection) -> Result<Emission> {
        let mut tx = transaction::begin(connection).await?;
        let current = verify::on(&mut tx, self.lease, self.input, self.call).await?;
        budget::check_on(
            &mut tx,
            &current.current,
            &current.context,
            current.task.as_ref(),
            false,
        )
        .await?;
        let locked = row::find(&mut tx, Lookup::Id(&self.call.id))
            .await?
            .ok_or(TaskRefusal::NotFound)?;
        if locked.state()? != DecisionCallState::Reserved || locked.issued || locked.accounted {
            return Ok(Emission::Existing(Box::new(locked)));
        }
        let required =
            super::credit::required_on(&mut tx, self.lease.lease_id, self.coverage.headroom)
                .await?;
        if required > self.coverage.required {
            return Err(TaskRefusal::DecisionUnavailable.into());
        }
        let payload = row::payload(&mut tx, &locked.id)
            .await?
            .ok_or(TaskRefusal::DecisionUnavailable)?;
        issue_on(&mut tx, &locked).await?;
        let packet = encode(locked, payload, self.key)?;
        tx.commit()
            .await
            .map_err(query("commit funded physical decision issuance"))?;
        tracing::info!(
            event = "dt_decision_issued",
            decision_call_id = self.call.id.as_str(),
            assessment_id = self.call.assessment_id.as_str(),
            attempt = self.call.attempt_number
        );
        Ok(Emission::Dispatch(packet))
    }
}

async fn issue_on(connection: &mut PgConnection, call: &Call) -> Result<()> {
    sqlx::query_scalar::<_, String>(sql::ISSUE)
        .bind(&call.id)
        .bind(DecisionCallState::Issued.as_str())
        .bind(DecisionCallState::Reserved.as_str())
        .bind(crate::lease::sql::LEASE_STATUS_ACTIVE)
        .bind(crate::lease::sql::FLEET_STATUS_ACTIVE)
        .bind(&call.profile_snapshot.0.profile.model_id)
        .bind(&call.profile_snapshot.0.revision_id)
        .bind(afd_wire::task::TaskState::Running.as_str())
        .bind(crate::lease::sql::RUNNER_STATUS_ACTIVE)
        .fetch_optional(connection)
        .await
        .map_err(query("commit one native decision credential grant"))?
        .ok_or(TaskRefusal::StaleState)?;
    Ok(())
}

fn encode(call: Call, payload: Payload, key: &SecretString) -> Result<ReservedDecision> {
    let packet = DecisionReservationResult::Dispatch {
        call_id: call.id,
        dispatch: Box::new(DecisionDispatch {
            assessment_id: call.assessment_id,
            attempt: call.attempt_number,
            assignment: call.profile_snapshot.0,
            request: payload.request.0,
            api_key: Cow::Borrowed(key.expose()),
            expires_at: call.logical_expires_at,
        }),
    };
    let body =
        Zeroizing::new(serde_json::to_string(&packet).map_err(afd_core::json::Unreadable::from)?);
    Ok(ReservedDecision { body, issued: true })
}
