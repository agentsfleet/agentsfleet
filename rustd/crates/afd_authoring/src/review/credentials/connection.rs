//! Routing installs bind the reviewed handle and its actual granted scopes.

use super::{
    CONTEXT, CheckKind, CheckState, CredentialInspection, Observed, PgConnection, Result, json,
    query, readiness,
};
use afd_connector::{Provider, registry::Archetype};

/// Observes the reverse-routing install behind `held`, whose integration is
/// `service`.
pub(super) async fn observe(
    connection: &mut PgConnection,
    draft: &super::super::Context,
    service: &str,
    held: &CredentialInspection,
    observed: &mut Observed,
) -> Result<()> {
    let (version, covered) = if let Some(id) = held.connection_id.as_deref() {
        sqlx::query_as::<_, (Option<String>, bool)>(crate::review::sql::CONNECTION)
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

/// The scopes an install of `service` must have granted: Slack's consent
/// scopes, and none for a service routed without a consent flow.
fn required_scopes(service: &str) -> Vec<&'static str> {
    match (Provider::parse(service), Provider::Slack.archetype()) {
        (Some(Provider::Slack), Archetype::Oauth2(flow)) => {
            flow.scopes.split(flow.scope_delimiter).collect()
        }
        _ => Vec::new(),
    }
}
