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
const DEFAULT: &str = "\
SELECT CASE WHEN s.mode='self_managed' THEN (
 SELECT e.id::text FROM core.tenant_model_entries e
 WHERE e.tenant_id=$1::uuid AND e.model_id=s.model AND e.secret_ref=s.secret_ref
 ORDER BY e.id LIMIT 1) ELSE $2::text END
FROM (SELECT) seed LEFT JOIN core.tenant_model_selection s ON s.tenant_id=$1::uuid";

pub(super) async fn observe(
    connection: &mut PgConnection,
    draft: &super::Context,
    candidate: &Candidate,
    observed: &mut Observed,
) -> Result<Option<String>> {
    let reference = match candidate.definition.runtime().model() {
        Some(reference) => Some(reference.to_owned()),
        None => sqlx::query_scalar(DEFAULT)
            .bind(draft.tenant_id.as_str())
            .bind(PLATFORM_MODEL_REF)
            .fetch_one(&mut *connection)
            .await
            .map_err(query(CONTEXT))?,
    };
    let resolved = match &reference {
        Some(reference) => runtime_model(connection, &draft.tenant_id, reference).await?,
        None => None,
    };
    observed
        .versions
        .push(json!({"kind":"runtime_model", "reference":reference,
        "version":resolved.as_ref().map(|model| &model.resolution_version)}));
    let ready = resolved.is_some();
    observed.check(
        CheckKind::Model,
        reference.as_deref().unwrap_or("account-default"),
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
    Ok(reference)
}
