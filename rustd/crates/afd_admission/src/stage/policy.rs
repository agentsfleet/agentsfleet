//! New admissions hold the selected setup until their ledger row commits.

use crate::{
    Admission, Result,
    error::{ErrorKind, query},
    sql,
};
use sqlx::PgConnection;

const CONTEXT: &str = "check installed admission policy";

pub(super) async fn check(
    connection: &mut PgConnection,
    admission: &Admission<'_>,
    key: &str,
) -> Result<()> {
    let repeated: bool = sqlx::query_scalar(sql::SELECT_REPEATED_ON_FLEET)
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
    let policy: Option<Option<String>> = sqlx::query_scalar(sql::SELECT_INSTALLED_POLICY)
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
