//! What a claim reads about a candidate before it locks anything.
//!
//! A context is the candidate, up to [`super::context`]'s history rows, their
//! serialised envelope and a token count: the costliest step of admission, and
//! one that needs no lock to read. So a claim reads it here first, on the
//! connection it is about to open its transaction on, and the locked section
//! only proves the snapshot still describes the draft it locked.
//!
//! The snapshot is an optimisation, never a decision. When there is nothing to
//! read — no queued message yet, a model that will not resolve — there is no
//! snapshot, and the locked section reads the context itself and fails the
//! message durably exactly as it always has. When the locked draft is no
//! longer the one read, the candidate is skipped for this poll rather than
//! admitted on stale context; the next poll reads afresh.

use afd_core::{error_code, id::Uuid7};
use afd_credential::provider::RegisteredModel;
use sqlx::PgConnection;

use super::{
    context::{self, Context, Source},
    selection::{Hint, Selected},
    sql,
};
use crate::{
    Result,
    error::raise::query,
    model,
    types::{EDITING, QUEUED},
};

/// The draft facts a snapshot is checked against once the draft is locked.
#[derive(sqlx::FromRow)]
struct DraftRow {
    resource_version: i64,
    head_candidate_id: Option<String>,
    cancelled_through_sequence: i64,
}

/// [`sql::MESSAGE`]'s row, by column name.
#[derive(sqlx::FromRow)]
struct Next {
    sequence: i64,
    tenant_model_id: Option<String>,
}

/// One candidate's context, read without locks.
#[derive(Debug)]
pub(super) struct Snapshot {
    resource_version: i64,
    sequence: i64,
    head_candidate_id: Option<Uuid7>,
    model: RegisteredModel,
    /// The context, or the refusal reading it raised — a context too large is
    /// failed durably only once the snapshot is proved current.
    context: Result<Context>,
}

impl Snapshot {
    /// Whether the locked draft and its selected message are the ones read.
    ///
    /// Any write to a draft moves its `resource_version`, so an edit, a new
    /// message or a new head candidate between the read and the lock is seen.
    pub(super) fn current(&self, selected: &Selected) -> bool {
        self.resource_version == selected.draft.resource_version
            && self.sequence == selected.sequence
            && self.head_candidate_id == selected.draft.head_candidate_id
    }

    /// The context for `model`, the one the locked section resolved.
    ///
    /// The envelope does not depend on the model; its size verdict does, so a
    /// model that moved since the read is checked again here.
    ///
    /// # Errors
    /// The read's own refusal, or `AUTHORING_TOKEN` when the context does not
    /// fit the model resolved under the lock.
    pub(super) fn context(self, model: &RegisteredModel) -> Result<Context> {
        let context = self.context?;
        if &self.model != model {
            context::fits(&context, model)?;
        }
        Ok(context)
    }
}

/// Read `hint`'s next queued message and its context without taking a lock.
///
/// # Errors
/// A datastore that would not answer. A draft with nothing queued, or a model
/// that will not resolve, answers no snapshot, and the locked section decides.
pub(super) async fn take(connection: &mut PgConnection, hint: &Hint) -> Result<Option<Snapshot>> {
    let draft: Option<DraftRow> = sqlx::query_as(sql::SNAPSHOT)
        .bind(hint.id.as_str())
        .bind(EDITING)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(sql::CONTEXT))?;
    let Some(draft) = draft else {
        return Ok(None);
    };
    let next: Option<Next> = sqlx::query_as(sql::MESSAGE)
        .bind(hint.id.as_str())
        .bind(QUEUED)
        .bind(draft.cancelled_through_sequence)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(sql::CONTEXT))?;
    let Some(next) = next else {
        return Ok(None);
    };
    let reference = model::reference(next.tenant_model_id);
    let model = match model::selected(connection, &hint.tenant_id, &reference).await {
        Ok(model) => model,
        Err(error) if error.code() == error_code::AUTHORING_MODEL => return Ok(None),
        Err(error) => return Err(error),
    };
    let head = draft
        .head_candidate_id
        .as_deref()
        .map(Uuid7::parse)
        .transpose()?;
    let source = Source {
        draft: &hint.id,
        head: head.as_ref(),
        sequence: next.sequence,
    };
    // Only a context too large waits for the lock, which fails it durably;
    // a datastore that would not answer is reported now.
    let context = match context::read(connection, source, &model).await {
        Err(error) if error.code() != error_code::AUTHORING_TOKEN => return Err(error),
        read => read,
    };
    Ok(Some(Snapshot {
        resource_version: draft.resource_version,
        sequence: next.sequence,
        head_candidate_id: head,
        model,
        context,
    }))
}

#[cfg(test)]
mod tests {
    #![expect(clippy::expect_used, reason = "a test asserts by panicking")]

    use afd_core::error_code;
    use afd_credential::provider::RegisteredModel;
    use afd_wire::authoring::{ContextMessage, ContextRole};

    use super::{Context, Snapshot};

    /// A window wide enough for any framed bench context.
    const WIDE: i32 = 200_000;

    /// Narrower than the fixed framing every call carries, so nothing fits.
    const NARROW: i32 = 4_096;

    fn model(window: i32) -> RegisteredModel {
        RegisteredModel {
            reference: "platform".into(),
            provider: "anthropic".into(),
            model: "claude-opus-5".into(),
            base_url: None,
            inference: None,
            context_window_tokens: window,
            input_nanos_per_million: 1,
            output_nanos_per_million: 1,
            resolution_version: format!("window-{window}").into(),
            platform_managed: false,
        }
    }

    fn snapshot(read_with: RegisteredModel) -> Snapshot {
        Snapshot {
            resource_version: 1,
            sequence: 1,
            head_candidate_id: None,
            model: read_with,
            context: Ok(Context {
                candidate: None,
                messages: vec![ContextMessage {
                    role: ContextRole::User,
                    text: "bench".to_owned(),
                }],
            }),
        }
    }

    #[test]
    fn a_snapshot_read_under_the_locked_model_keeps_its_context() {
        let context = snapshot(model(WIDE))
            .context(&model(WIDE))
            .expect("same model, same verdict");
        assert_eq!(context.messages.len(), 1);
    }

    #[test]
    fn a_model_that_moved_since_the_read_is_sized_again() {
        let refused = snapshot(model(WIDE))
            .context(&model(NARROW))
            .expect_err("the locked model's window is the one that counts");
        assert_eq!(refused.code(), error_code::AUTHORING_TOKEN);
    }

    #[test]
    fn a_refusal_read_without_the_lock_is_raised_under_it() {
        let mut read = snapshot(model(WIDE));
        read.context = Err(crate::error::ErrorKind::Token.into());
        let refused = read
            .context(&model(WIDE))
            .expect_err("the deferred refusal");
        assert_eq!(refused.code(), error_code::AUTHORING_TOKEN);
    }
}
