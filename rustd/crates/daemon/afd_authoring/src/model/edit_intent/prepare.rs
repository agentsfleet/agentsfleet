//! Newest accepted message and reviewed profile are read from native owners.

use af_core::id::Uuid7;
use af_wire::decision::{
    DecisionPurpose, DecisionQuestion, DecisionRequest, NamedQuestion, NoulCriteria,
};
use afd_billing::price::PriceBook;
use sqlx::Connection as _;

use super::{CONTEXT, ModelCallInput, ModelCalls, Prepared, QUESTION, QUESTION_NAME, Snapshot};
use crate::{
    Result,
    error::{ErrorKind, raise::query},
    lease::guard,
};

impl ModelCalls {
    pub(super) async fn prepare_edit(
        &self,
        runner: &Uuid7,
        lease: &Uuid7,
        request: &garde::Valid<ModelCallInput>,
        counters: &guard::Counters,
    ) -> Result<Option<Prepared>> {
        let mut connection = self.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let guarded = self
            .drafts
            .publication(&mut tx, runner, lease, counters)
            .await?;
        let prepared = self.edit_inputs(&mut tx, runner, request, &guarded).await;
        let prepared = match prepared {
            Ok(prepared) => Some(prepared),
            Err(fault) => {
                record_fallback(&mut tx, request, &guarded, "unavailable").await?;
                tracing::info!(event = "setup_edit_intent", dispatch_id = %request.dispatch_id,
                    disposition = "unavailable", error_code = %fault.code());
                None
            }
        };
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(prepared)
    }

    async fn edit_inputs(
        &self,
        connection: &mut sqlx::PgConnection,
        runner: &Uuid7,
        input: &ModelCallInput,
        guarded: &guard::Guarded,
    ) -> Result<Prepared> {
        self.decisions.as_ref().ok_or(ErrorKind::Model)?;
        let unresolved: bool = sqlx::query_scalar(include_str!("sql/paid_unavailable.sql"))
            .bind(&guarded.attempt.id)
            .fetch_one(&mut *connection)
            .await
            .map_err(query(CONTEXT))?;
        if unresolved {
            return Err(ErrorKind::Balance.into());
        }
        let selected = afd_runner::decision_assignment_on(connection, runner)
            .await?
            .ok_or(ErrorKind::Model)?;
        let attempt = &guarded.attempt;
        let accepted =
            crate::message::accepted(connection, &attempt.draft_id, attempt.message_sequence)
                .await?;
        let request = garde::Unvalidated::new(DecisionRequest {
            call_id: input.dispatch_id.clone(), purpose: DecisionPurpose::Intent,
            state: serde_json::json!({"newest_message":accepted.user_text}),
            questions: vec![NamedQuestion { name: QUESTION_NAME.into(), question: DecisionQuestion::Noul {
                instructions: QUESTION.into(), criteria: NoulCriteria {
                    yes: "The person requests a change to the fleet setup.".into(),
                    no: "The person asks a question or discusses options without requesting a setup change.".into(),
                },
            }}],
        }).validate()?;
        let profile = &selected.assignment.profile;
        garde::Validate::validate(selected.assignment.as_ref())?;
        if !profile.capabilities.noul
            || !af_wire::decision::serialized_within(
                &(&request.state, &request.questions),
                usize::try_from(profile.input_max_bytes).unwrap_or(usize::MAX),
            )
        {
            return Err(ErrorKind::Token.into());
        }
        let book = PriceBook::load_decision(
            &mut *connection,
            &Uuid7::parse(&profile.model_id)?,
            &Uuid7::parse(&selected.assignment.revision_id)?,
            guarded.now,
        )
        .await?;
        book.quote(guarded.now, 0).ok_or(ErrorKind::Model)?;
        let reserved_nanos = book
            .decision_reservation(
                guarded.now,
                profile.input_max_bytes,
                u32::try_from(af_wire::decision::REPLY_MAX_BYTES)
                    .map_err(|_overflow| ErrorKind::Balance)?,
            )
            .ok_or(ErrorKind::Balance)?
            .as_i64()
            .max(20_000_000);
        Ok(Prepared {
            request,
            attempt: attempt.clone(),
            candidate_revision: guarded.draft.candidate_revision,
            reserved_nanos,
            snapshot: Snapshot {
                assignment: *selected.assignment,
                provider: selected.provider,
                book,
                at: guarded.now,
            },
        })
    }
}

pub(super) async fn record_fallback(
    connection: &mut sqlx::PgConnection,
    request: &ModelCallInput,
    guarded: &guard::Guarded,
    disposition: &'static str,
) -> Result<()> {
    sqlx::query(include_str!("sql/fallback.sql"))
        .bind(super::super::dispatch_id(request)?)
        .bind(&guarded.attempt.id)
        .bind(&guarded.attempt.tenant_id)
        .bind(&guarded.attempt.workspace_id)
        .bind(i64::from(af_wire::usage::DECISION_SEQ_BASE) + i64::from(request.call_index))
        .bind(guarded.draft.candidate_revision)
        .bind(disposition)
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}
