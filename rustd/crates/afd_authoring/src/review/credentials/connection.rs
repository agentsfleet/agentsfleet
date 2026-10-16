//! Routing installs bind the reviewed handle and its actual granted scopes.

use super::{
    CONTEXT, CheckKind, CheckState, CredentialInspection, Observed, PgConnection, Result, json,
    query, readiness,
};
use afd_connector::{Provider, registry::Archetype};

const CONNECTION: &str = "\
WITH held AS MATERIALIZED (
 SELECT id,external_account_id,scopes FROM core.connector_installs
 WHERE workspace_id=$1::uuid AND provider=$2 AND external_account_id=$3 FOR SHARE)
SELECT encode(sha256(convert_to(jsonb_agg(jsonb_build_array(id,external_account_id,scopes) ORDER BY id)::text,'UTF8')),'hex'),
coalesce(bool_and(scopes @> $4::text[]),false) FROM held";

pub(super) async fn observe(
    connection: &mut PgConnection,
    draft: &super::super::Context,
    held: &CredentialInspection,
    observed: &mut Observed,
) -> Result<()> {
    let Some(service) = held.integration.as_deref() else {
        return Ok(());
    };
    let (version, covered) = if let Some(id) = held.connection_id.as_deref() {
        sqlx::query_as::<_, (Option<String>, bool)>(CONNECTION)
            .bind(draft.workspace_id.as_str())
            .bind(service)
            .bind(id)
            .bind(required_scopes(service))
            .fetch_one(connection)
            .await
            .map_err(query(CONTEXT))?
    } else {
        // Refresh-only providers have no reverse-routing install row. Their
        // validated, name-bound handle is the landed workspace connection.
        (Some(held.resolution_version.clone()), true)
    };
    let state = match (version.is_some(), covered) {
        (true, true) => CheckState::Ready,
        (true, false) => CheckState::Blocked,
        (false, _) => CheckState::Missing,
    };
    observed.check(
        CheckKind::Connection,
        service,
        state,
        Some(readiness::connections(draft)),
    );
    observed
        .versions
        .push(json!({"kind":"connection", "service":service, "version":version}));
    Ok(())
}

fn required_scopes(service: &str) -> Vec<&'static str> {
    match Provider::parse(service) {
        Some(Provider::Slack) => match Provider::Slack.archetype() {
            Archetype::Oauth2(flow) => flow.scopes.split(flow.scope_delimiter).collect(),
            Archetype::AppInstall(_) => Vec::new(),
        },
        _ => Vec::new(),
    }
}
