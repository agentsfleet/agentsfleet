//! Host capacity and live authorization stay locked through the fresh expiry write.

use afd_core::{id::Uuid7, timing::LEASE_TTL_MS};
use afd_wire::authoring::{Renew, Renewed};
use sqlx::Connection as _;

use super::{failure, guard, meter};
use crate::{
    Drafts, Result,
    error::{ErrorKind, raise::query},
    funnel::Owed,
    model::budget,
};

const CONTEXT: &str = "renew authoring lease";
const EXTEND: &str = "\
UPDATE fleet.authoring_attempts SET lease_expires_at=$2,reported_runtime_ms=$3,updated_at=$4
WHERE id=$1::uuid AND lease_expires_at>$4 AND deadline>$4";

impl Drafts {
    /// Renews an unexpired, authorized own lease without issuing a new capability.
    ///
    /// # Errors
    /// Refuses stale fences, expiry, lost access/model, capacity or wallet coverage.
    pub async fn renew(&self, runner: &Uuid7, lease: &Uuid7, request: Renew) -> Result<Renewed> {
        let request = garde::Unvalidated::new(request).validate()?;
        let counters = guard::Counters::renew(&request)?;
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let host = afd_runner::capacity::lock(&mut tx, runner)
            .await?
            .ok_or(ErrorKind::Hidden)?;
        if !host.can_renew() {
            return Err(ErrorKind::Fence.into());
        }
        let guarded = guard::bound(&mut tx, runner, lease, &counters).await?;
        let mut owed = Owed::default();
        let (verified, remaining_output_tokens) = match available(&mut tx, &guarded, &request).await
        {
            Ok(verified) => verified,
            Err(error) if failure::record(&mut tx, &guarded.attempt, &error, &mut owed).await? => {
                tx.commit().await.map_err(query(CONTEXT))?;
                owed.settle(&self.analytics);
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        let now = guard::refresh(&mut tx, &guarded, &counters).await?;
        let until = now
            .saturating_add_millis(LEASE_TTL_MS)
            .as_millis()
            .min(guarded.attempt.deadline);
        let changed = sqlx::query(EXTEND)
            .bind(guarded.attempt.id.as_str())
            .bind(until)
            .bind(verified.runtime_ms)
            .bind(now.as_millis())
            .execute(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        if changed.rows_affected() != 1 {
            return Err(ErrorKind::Fence.into());
        }
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(Renewed {
            expires_at: until,
            stop_requested: false,
            remaining_output_tokens,
            model_access_expires_at: until,
        })
    }
}

async fn available(
    connection: &mut sqlx::PgConnection,
    guarded: &guard::Guarded,
    request: &garde::Valid<Renew>,
) -> Result<(meter::Verified, u64)> {
    guard::current_model(connection, guarded).await?;
    let observed = garde::Unvalidated::new(request.usage.clone()).validate()?;
    let verified = meter::verify(
        connection,
        &guarded.attempt,
        &observed,
        guarded.now.as_millis(),
    )
    .await?;
    budget::coverage(connection, &guarded.attempt).await?;
    let remaining = u64::try_from(
        budget::output_limit(&guarded.attempt.model_snapshot) - guarded.attempt.output_tokens,
    )
    .map_err(|_negative| ErrorKind::Token)?;
    Ok((verified, remaining))
}
