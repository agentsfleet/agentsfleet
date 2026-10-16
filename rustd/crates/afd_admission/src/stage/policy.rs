//! New admissions hold the selected setup until their ledger row commits.

use crate::{
    Admission, Result,
    error::{ErrorKind, query},
};
use sqlx::PgConnection;

const CONTEXT: &str = "check installed admission policy";
const REPEATED: &str = "SELECT EXISTS(SELECT 1 FROM core.fleet_admissions
WHERE fleet_id=$1::uuid AND workspace_id=$2::uuid AND producer=$3 AND producer_key=$4)";
const POLICY:&str="SELECT r.policy_semantics FROM core.fleets f
LEFT JOIN core.fleet_definition_revisions r ON r.id=f.current_definition_revision_id AND r.fleet_id=f.id
WHERE f.id=$1::uuid AND f.workspace_id=$2::uuid FOR SHARE OF f";

pub(super) async fn check(
    connection: &mut PgConnection,
    admission: &Admission<'_>,
    key: &str,
) -> Result<()> {
    let repeated: bool = sqlx::query_scalar(REPEATED)
        .bind(admission.fleet)
        .bind(admission.workspace)
        .bind(admission.producer.as_str())
        .bind(key)
        .fetch_one(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    if repeated {
        return Ok(());
    }
    let policy: Option<Option<String>> = sqlx::query_scalar(POLICY)
        .bind(admission.fleet)
        .bind(admission.workspace)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    if policy.flatten().as_deref() != Some(afd_fleet_runtime::definition::POLICY_SEMANTICS) {
        return Err(ErrorKind::Policy.into());
    }
    Ok(())
}
