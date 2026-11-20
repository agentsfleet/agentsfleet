//! Operation replay precedes rendering and fresh cumulative budget checks.

use af_core::clock;
use af_core::id::Uuid7;
use af_wire::{
    decision::DecisionPurpose,
    task::{DecisionCallState, DecisionReservationInput},
};
use sqlx::PgConnection;

use super::{
    attempt::Attempt,
    budget, context,
    intent::Intent,
    prepare,
    row::{self, Call, Lookup},
    sql,
};
use crate::{
    Result,
    error::{query, stale_fence},
    task::{TaskRefusal, TaskRun, Tasks, guard, transaction},
};

pub(super) enum Allocation {
    Reserved(Call),
    Existing(Call),
}

pub(super) async fn allocate(
    tasks: &Tasks,
    run: TaskRun<'_>,
    input: &garde::Valid<DecisionReservationInput>,
) -> Result<Allocation> {
    if input.fencing_token != run.fencing_token {
        return Err(stale_fence());
    }
    let request = Reservation {
        tasks,
        run,
        input,
        operation: guard::wire(&input.operation_id)?,
        digest: digest(input)?,
    };
    if let Some(call) = row::peek(tasks.runs.pool(), request.operation()).await? {
        return request.replay(call);
    }
    let mut connection = transaction::acquire(tasks.runs.pool()).await?;
    let result = request.on(&mut connection).await;
    transaction::recycle(connection);
    result
}

struct Reservation<'a> {
    tasks: &'a Tasks,
    run: TaskRun<'a>,
    input: &'a garde::Valid<DecisionReservationInput>,
    operation: Uuid7,
    digest: String,
}

impl Reservation<'_> {
    fn operation(&self) -> Lookup<'_> {
        Lookup::Operation {
            run: self.run.run_id,
            operation: &self.operation,
        }
    }

    fn replay(&self, call: Call) -> Result<Allocation> {
        if !call.owned_by(self.run) {
            return Err(stale_fence());
        }
        replay_matches(&call, self.input, &self.digest, true)?;
        classify(call)
    }

    async fn on(&self, connection: &mut PgConnection) -> Result<Allocation> {
        let mut tx = transaction::begin(connection).await?;
        let current = guard::run(&mut tx, self.run).await?;
        // A peer may have committed while this request waited for the thread lock.
        if let Some(call) = row::find(&mut tx, self.operation()).await? {
            return self.replay(call);
        }
        block_profile_switch(&mut tx, &current, self.input).await?;
        let call_id = self.tasks.runs.entropy().uuid7(clock::now())?;
        let assessment_id = self.tasks.runs.entropy().uuid7(clock::now())?;
        let (prepared, quote) = prepare::on(&mut tx, self.run, self.input, &call_id).await?;
        let key = prepared.context.key()?;
        let attempt = Attempt::resolve(
            &mut tx,
            &prepared,
            self.input
                .retry_of
                .as_deref()
                .map(guard::wire)
                .transpose()?
                .as_ref(),
            &key,
            &self.digest,
            &call_id,
            assessment_id,
        )
        .await?;
        if let Some(call) = row::find(
            &mut tx,
            Lookup::Assessment {
                key: &key,
                attempt: attempt.number(),
            },
        )
        .await?
        {
            replay_matches(&call, self.input, &self.digest, false)?;
            tx.commit()
                .await
                .map_err(query("commit saved decision reservation lookup"))?;
            return if call.owned_by(self.run) {
                classify(call)
            } else {
                Ok(Allocation::Existing(call))
            };
        }
        self.admit(&mut tx, &prepared).await?;
        Intent {
            run: self.run,
            input: self.input,
            id: &call_id,
            prepared: &prepared,
            quote: &quote,
            attempt: &attempt,
            key: &key,
            digest: &self.digest,
        }
        .save(&mut tx)
        .await?;
        let call = row::find(&mut tx, Lookup::Id(&call_id))
            .await?
            .ok_or(TaskRefusal::NotFound)?;
        tx.commit()
            .await
            .map_err(query("commit native physical decision reservation"))?;
        reserved(&call, &prepared.assignment.revision_id);
        Ok(Allocation::Reserved(call))
    }
    async fn admit(
        &self,
        connection: &mut PgConnection,
        prepared: &prepare::Prepared,
    ) -> Result<()> {
        if self.tasks.dependencies.admin_workspace.is_none() {
            return Err(TaskRefusal::DecisionUnavailable.into());
        }
        budget::check_on(
            connection,
            &prepared.current,
            &prepared.context,
            prepared.task.as_ref().map(|task| &task.snapshot),
            true,
        )
        .await
    }
}

fn classify(call: Call) -> Result<Allocation> {
    if call.state()? == DecisionCallState::Reserved && !call.issued && !call.accounted {
        Ok(Allocation::Reserved(call))
    } else {
        Ok(Allocation::Existing(call))
    }
}

fn replay_matches(
    call: &Call,
    input: &DecisionReservationInput,
    digest: &str,
    operation: bool,
) -> Result<()> {
    if call.request_digest != digest
        || call.retry_of_decision_call_id.as_ref().map(Uuid7::as_str) != input.retry_of.as_deref()
        || (operation && call.source_turn_id != input.source_turn_id)
    {
        return Err(TaskRefusal::ReplayConflict.into());
    }
    Ok(())
}

fn digest(input: &DecisionReservationInput) -> Result<String> {
    let source =
        (input.assessment.purpose() == DecisionPurpose::Intent).then_some(&input.source_turn_id);
    let bytes = serde_json::to_vec(&(source, &input.assessment, &input.task, &input.evidence))
        .map_err(af_core::json::Unreadable::from)?;
    Ok(af_core::etag::sha256_hex(&bytes))
}

async fn block_profile_switch(
    connection: &mut PgConnection,
    current: &guard::RunRow,
    input: &DecisionReservationInput,
) -> Result<()> {
    let assignment = current
        .decision_snapshot
        .as_ref()
        .ok_or(TaskRefusal::DecisionUnavailable)?;
    let subject = context::subject(
        &input.source_turn_id,
        &input.assessment,
        current
            .task_id
            .as_ref()
            .map(Uuid7::as_str)
            .zip(input.task.as_ref().map(|task| task.generation)),
    )?;
    let blocked: bool = sqlx::query_scalar(sql::BLOCK_PROFILE_SWITCH)
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .bind(format!("{subject}/%"))
        .bind(crate::task::guard::wire(&assignment.0.revision_id)?)
        .fetch_one(connection)
        .await
        .map_err(query(
            "retain unresolved assessments before changing decision profile",
        ))?;
    if blocked {
        return Err(TaskRefusal::DecisionUnavailable.into());
    }
    Ok(())
}

fn reserved(call: &Call, revision: &str) {
    tracing::info!(
        event = "dt_decision_reserved",
        decision_call_id = call.id.as_str(),
        assessment_id = call.assessment_id.as_str(),
        attempt = call.attempt_number,
        profile_revision = revision,
        purpose = call.purpose.as_str()
    );
}
