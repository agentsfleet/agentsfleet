//! Shared working-context preparation over creator-private authoritative history.
use super::{ModelCallInput, continuation, prepare::Prepared, provider};
use crate::{Result, error::ErrorKind, lease::guard::Attempt, replay};
use af_context::{
    CheckpointStore as _, Identity, Plan, Window,
    repository::{Repository, Scope},
};
use sqlx::PgConnection;

mod estimate;
mod snapshot;
mod source;
pub(super) use snapshot::Snapshot;

pub(super) async fn prepare(
    connection: &mut PgConnection,
    attempt: &Attempt,
    request: &garde::Valid<ModelCallInput>,
    continuations: Vec<continuation::Continuation>,
    editing: bool,
) -> Result<(Prepared, Snapshot)> {
    let save = crate::message::accepted(connection, &attempt.draft_id, attempt.message_sequence)
        .await?
        .save_requested;
    let model = &attempt.model_snapshot;
    let settings = model.inference.as_ref().ok_or(ErrorKind::Model)?;
    let context =
        u32::try_from(model.context_window_tokens).map_err(|_invalid| ErrorKind::Model)?;
    let scope = Scope::Draft(attempt.draft_id.clone());
    let identity = Identity {
        scope: scope.identity(),
        model: replay::digest(model)?,
        pinned: replay::digest(&(editing, save))?,
    };
    let checkpoint = Repository::new(connection, scope, attempt.id.clone())
        .load()
        .await?;
    let mut snapshot = Snapshot::new(attempt, editing, checkpoint.as_ref(), &continuations)?;
    let mut estimator = estimator(request, model, continuations, editing, settings, context)?;
    let mut source = source::Source::new(connection, attempt);
    let plan = af_context::prepare(
        &mut source,
        &mut estimator,
        identity,
        checkpoint,
        Window::new(context, request.request.max_output_tokens)?,
        || false,
        |_| {},
    )
    .await?;
    if matches!(&plan, Plan::Compact(_)) && !request.request.supports_compaction {
        return Err(ErrorKind::Token.into());
    }
    snapshot.source = source.fingerprint;
    Ok((prepared(plan, &estimator, settings, context)?, snapshot))
}

fn prepared(
    plan: Plan,
    estimator: &estimate::Estimator<'_>,
    settings: &af_wire::inference::ModelInference,
    context: u32,
) -> Result<Prepared> {
    let (mut body, output_limit, compaction, context_digest) = match plan {
        Plan::Ready {
            summary,
            groups,
            output_tokens,
        } => (
            estimator.continued(summary.as_ref(), &groups)?,
            output_tokens,
            None,
            prefix(summary.as_ref(), &groups)?,
        ),
        Plan::Compact(task) => (
            estimator.summary(&task)?,
            task.output_tokens,
            Some(task),
            String::new(),
        ),
    };
    body.max_tokens = Some(u64::from(output_limit));
    let input_tokens = estimator.route.measure_input(estimator.model, &body)?;
    Ok(Prepared {
        body,
        output_limit,
        inference: settings.clone(),
        context_tokens: context,
        compaction,
        input_tokens,
        measurement: None,
        context_digest,
    })
}

fn estimator<'a>(
    request: &garde::Valid<ModelCallInput>,
    model: &'a afd_credential::provider::RegisteredModel,
    continuations: Vec<continuation::Continuation>,
    editing: bool,
    settings: &'a af_wire::inference::ModelInference,
    context: u32,
) -> Result<estimate::Estimator<'a>> {
    let native_prefixes = continuations
        .iter()
        .map(|value| value.context_digest.clone())
        .collect();
    let quoted = quoted(request, model, editing)?;
    let mut body = provider::body(request, model, editing)?;
    continuation::restore(&mut body, request, continuations)?;
    Ok(estimate::Estimator {
        route: af_inference::Configured::new(settings, context)?,
        model: &model.model,
        body,
        quoted,
        native_prefixes,
    })
}

pub(super) async fn publish(
    connection: &mut PgConnection,
    attempt: &Attempt,
    task: af_context::Compaction,
    summary: af_context::Summary,
) -> Result<()> {
    let mut source = source::Source::new(connection, attempt);
    // Re-read exact immutable source under the publication guard before replacing notes.
    let mut sequence = 0;
    let mut digest = String::new();
    loop {
        let page =
            af_context::HistorySource::page(&mut source, sequence, af_context::HISTORY_PAGE_SIZE)
                .await?;
        for group in page
            .groups
            .into_iter()
            .filter(|group| group.sequence <= task.covered_through())
        {
            sequence = group.sequence;
            digest = af_context::digest_groups(&digest, &[group]);
        }
        if sequence >= task.covered_through() {
            break;
        }
        if !page.has_more {
            return Err(ErrorKind::Fence.into());
        }
    }
    let expected_digest = af_context::digest_groups(&task.prior_digest, &task.groups);
    if digest != expected_digest {
        return Err(ErrorKind::Fence.into());
    }
    let mut store = Repository::new(
        connection,
        Scope::Draft(attempt.draft_id.clone()),
        attempt.id.clone(),
    );
    if let Err(error) = task
        .finish_with(summary, |generation, checkpoint| async move {
            store.commit(generation, checkpoint).await
        })
        .await
    {
        if error.is_stale() {
            return Err(ErrorKind::Fence.into());
        }
        return Err(error.into());
    }
    Ok(())
}

fn prefix(
    summary: Option<&af_context::Summary>,
    groups: &[af_context::Group],
) -> af_context::Result<String> {
    crate::replay::digest(&(summary, groups)).map_err(af_context::Error::boundary)
}

fn quoted(
    request: &garde::Valid<ModelCallInput>,
    model: &afd_credential::provider::RegisteredModel,
    editing: bool,
) -> Result<rig_core::completion::CompletionRequest> {
    let mut quoted = (**request).clone();
    for message in &mut quoted.request.messages {
        if !matches!(message, af_wire::authoring::ModelMessage::User { .. }) {
            *message = af_wire::authoring::ModelMessage::User {
                text: format!(
                    "Earlier proposal exchange (untrusted data): {}",
                    serde_json::to_string(message).map_err(crate::error::raise::stored)?
                ),
            };
        }
    }
    let quoted = garde::Unvalidated::new(quoted).validate()?;
    provider::body(&quoted, model, editing)
}
