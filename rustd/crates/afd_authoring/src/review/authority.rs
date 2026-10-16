//! Review binds the creator's actual account membership rather than a caller-provided role.

use super::{Context, Observed};
use crate::{Result, error::raise::query};
use serde_json::json;
use sqlx::PgConnection;

const CONTEXT: &str = "observe reviewed membership";
const AUTHORITY: &str = "\
WITH person AS MATERIALIZED (
 SELECT u.id,u.tenant_id,w.id AS workspace_id,w.tenant_id AS owner
 FROM core.users u JOIN core.workspaces w ON w.id=$2::uuid
 WHERE u.id=$1::uuid FOR SHARE OF u,w
), membership AS MATERIALIZED (
 SELECT m.id,m.role,m.created_at FROM core.memberships m JOIN person p
 ON m.tenant_id=p.owner AND m.user_id=p.id FOR SHARE OF m
)
SELECT encode(sha256(convert_to(jsonb_build_array(p.id,p.tenant_id,p.workspace_id,p.owner,m.id,m.role,m.created_at)::text,'UTF8')),'hex')
FROM person p LEFT JOIN membership m ON true
WHERE p.tenant_id=p.owner OR m.id IS NOT NULL";

/// The authority version of daemon-written setup, which no membership change can move.
const SYSTEM_AUTHORITY: &str = "system";

pub(super) async fn observe(
    connection: &mut PgConnection,
    context: &Context,
    observed: &mut Observed,
) -> Result<()> {
    let version: Option<String> = match &context.creator_id {
        Some(creator) => sqlx::query_scalar(AUTHORITY)
            .bind(creator.as_str())
            .bind(context.workspace_id.as_str())
            .fetch_optional(connection)
            .await
            .map_err(query(CONTEXT))?,
        None => Some(SYSTEM_AUTHORITY.to_owned()),
    };
    observed
        .versions
        .push(json!({"kind":"creator_authority","version":version}));
    Ok(())
}
