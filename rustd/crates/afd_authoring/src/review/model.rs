//! Runtime defaults resolve only through the workspace owner's explicit selection.

use afd_credential::provider::{PLATFORM_MODEL_REF, runtime_model};
use serde_json::json;
use sqlx::PgConnection;

use super::Observed;
use crate::{
    Result,
    error::raise::query,
    types::{Action, ActionKind, Candidate, CheckKind, CheckState},
};

const CONTEXT: &str = "resolve reviewed runtime selection";
pub(super) async fn observe(
    connection: &mut PgConnection,
    draft: &super::Context,
    candidate: &Candidate,
    observed: &mut Observed,
) -> Result<Option<String>> {
    // The owner's selection, else the platform default: never no reference.
    let reference = match candidate.definition.runtime().model() {
        Some(reference) => reference.to_owned(),
        None => sqlx::query_scalar(super::sql::DEFAULT_MODEL)
            .bind(draft.tenant_id.as_str())
            .bind(PLATFORM_MODEL_REF)
            .fetch_one(&mut *connection)
            .await
            .map_err(query(CONTEXT))?,
    };
    let resolved = runtime_model(connection, &draft.tenant_id, &reference).await?;
    observed
        .versions
        .push(json!({"kind":"runtime_model", "reference":reference,
        "version":resolved.as_ref().map(|model| &model.resolution_version)}));
    let ready = resolved.is_some();
    observed.check(
        CheckKind::Model,
        &reference,
        if ready {
            CheckState::Ready
        } else {
            CheckState::Missing
        },
        Some(Action {
            kind: ActionKind::AccountModels,
            label: "Open owner models".to_owned(),
            href: format!("/w/{}/settings/models", draft.workspace_id),
        }),
    );
    Ok(Some(reference))
}
