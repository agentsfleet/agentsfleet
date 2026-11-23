//! A reservation reads with no lock, then issues its call in one statement
//! (`dispatch.rs`). Operation replay and the assessment lookup precede every
//! money check, so a saved call answers `existing` whatever the wallet or the budget.

use af_core::clock;
use af_core::id::Uuid7;
use af_wire::{decision::DecisionPurpose, durable_task::DecisionReservationInput};
use sqlx::PgConnection;

use super::{
    ReservedDecision,
    attempt::Attempt,
    budget,
    dispatch::Issuance,
    fund::{self, Coverage},
    prepare::{self, Prepared},
    quote::FrozenQuote,
    replay,
    row::{self, Call, Lookup},
};
use crate::{
    Result,
    durable_task::{DurableTaskRefusal, DurableTaskRun, DurableTasks, guard, transaction},
    error::stale_fence,
};

/// Everything the issue statement needs, read and judged before it.
pub(super) struct Ready {
    pub prepared: Prepared,
    pub quote: FrozenQuote,
    pub attempt: Attempt,
    pub key: String,
    pub coverage: Coverage,
}

enum Read {
    Existing(Call),
    Ready(Box<Ready>),
}

pub(super) async fn allocate(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    input: &garde::Valid<DecisionReservationInput>,
) -> Result<ReservedDecision> {
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
        return request.replay(call).await;
    }
    let call_id = tasks.runs.entropy().uuid7(clock::now())?;
    let assessment_id = tasks.runs.entropy().uuid7(clock::now())?;
    let mut connection = transaction::acquire(tasks.runs.pool()).await?;
    let read = request.read(&mut connection, &call_id, assessment_id).await;
    transaction::recycle(connection);
    let ready = match read? {
        Read::Existing(call) => return replay::response(tasks, call).await,
        Read::Ready(ready) => ready,
    };
    if let Err(refusal) = fund::cover(tasks, run, &ready.prepared.current, &ready.coverage).await {
        let mut connection = transaction::acquire(tasks.runs.pool()).await?;
        let twin = request
            .twin_or(&mut connection, &ready.key, ready.attempt.number(), refusal)
            .await;
        transaction::recycle(connection);
        return replay::response(tasks, twin?).await;
    }
    let key = fund::key(tasks, ready.prepared.assignment.profile.api).await?;
    Issuance {
        tasks,
        run,
        input,
        id: &call_id,
        digest: &request.digest,
        ready: &ready,
        key: &key,
    }
    .commit()
    .await
}

pub(super) struct Reservation<'a> {
    pub tasks: &'a DurableTasks,
    pub run: DurableTaskRun<'a>,
    pub input: &'a garde::Valid<DecisionReservationInput>,
    pub operation: Uuid7,
    pub digest: String,
}

impl Reservation<'_> {
    fn operation(&self) -> Lookup<'_> {
        Lookup::Operation {
            run: self.run.run_id,
            operation: &self.operation,
        }
    }

    /// The saved call of this operation, answered as `existing` when it is this
    /// fence's and was made from the same request.
    pub(super) async fn replay(&self, call: Call) -> Result<ReservedDecision> {
        if !call.owned_by(self.run) {
            return Err(stale_fence());
        }
        replay_matches(&call, self.input, &self.digest, true)?;
        replay::response(self.tasks, call).await
    }

    /// Statements 2 to 15 of the reservation, each in its own round trip on one
    /// connection and none holding a lock.
    async fn read(
        &self,
        connection: &mut PgConnection,
        call_id: &Uuid7,
        assessment_id: Uuid7,
    ) -> Result<Read> {
        let (prepared, quote) = prepare::on(connection, self.run, self.input, call_id).await?;
        let key = prepared.context.key()?;
        let retry_of = self
            .input
            .retry_of
            .as_deref()
            .map(guard::wire)
            .transpose()?;
        let attempt = Attempt::resolve(
            connection,
            &prepared,
            retry_of.as_ref(),
            &key,
            &self.digest,
            call_id,
            assessment_id,
        )
        .await?;
        let saved = row::read(
            connection,
            Lookup::Assessment {
                key: &key,
                attempt: attempt.number(),
            },
        )
        .await?;
        if let Some(call) = saved {
            replay_matches(&call, self.input, &self.digest, false)?;
            return Ok(Read::Existing(call));
        }
        let coverage = match self.money(connection, &prepared).await {
            Ok(coverage) => coverage,
            Err(refusal) => {
                let twin = self.twin_or(connection, &key, attempt.number(), refusal);
                return Ok(Read::Existing(twin.await?));
            }
        };
        Ok(Read::Ready(Box::new(Ready {
            prepared,
            quote,
            attempt,
            key,
            coverage,
        })))
    }
}

impl Reservation<'_> {
    /// The checks that can refuse for money: the budget, the goal's limits, an earlier
    /// overage and the run's priced headroom.
    async fn money(&self, connection: &mut PgConnection, prepared: &Prepared) -> Result<Coverage> {
        if self.tasks.dependencies.admin_workspace.is_none() {
            return Err(DurableTaskRefusal::DecisionUnavailable.into());
        }
        budget::check_on(
            connection,
            &prepared.current,
            &prepared.context,
            prepared.task.as_ref().map(|task| &task.snapshot),
        )
        .await?;
        fund::proposed(connection, self.run).await
    }

    /// A money refusal may have counted a twin of this very reservation that issued
    /// after the lookups ran: the saved call then answers, as it would have before any
    /// money check. Without one, or for any other failure, `refusal` stands.
    async fn twin_or(
        &self,
        connection: &mut PgConnection,
        key: &str,
        attempt: i16,
        refusal: crate::Error,
    ) -> Result<Call> {
        if !matches!(
            refusal.kind(),
            crate::error::ErrorKind::DurableTaskRefused { .. }
        ) {
            return Err(refusal);
        }
        if let Some(call) = row::read(connection, self.operation()).await? {
            if !call.owned_by(self.run) {
                return Err(stale_fence());
            }
            replay_matches(&call, self.input, &self.digest, true)?;
            return Ok(call);
        }
        match row::read(connection, Lookup::Assessment { key, attempt }).await? {
            Some(call) => {
                replay_matches(&call, self.input, &self.digest, false)?;
                Ok(call)
            }
            None => Err(refusal),
        }
    }
}

pub(super) fn replay_matches(
    call: &Call,
    input: &DecisionReservationInput,
    digest: &str,
    operation: bool,
) -> Result<()> {
    if call.request_digest != digest
        || call.retry_of_decision_call_id.as_ref().map(Uuid7::as_str) != input.retry_of.as_deref()
        || (operation && call.source_turn_id != input.source_turn_id)
    {
        return Err(DurableTaskRefusal::ReplayConflict.into());
    }
    Ok(())
}

fn digest(input: &DecisionReservationInput) -> Result<String> {
    let source =
        (input.assessment.purpose() == DecisionPurpose::Intent).then_some(&input.source_turn_id);
    let bytes = serde_json::to_vec(&(
        source,
        &input.assessment,
        &input.durable_task,
        &input.evidence,
    ))
    .map_err(af_core::json::Unreadable::from)?;
    Ok(af_core::etag::sha256_hex(&bytes))
}
