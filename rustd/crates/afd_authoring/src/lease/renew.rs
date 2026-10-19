//! Host capacity and live authorization stay locked through the fresh expiry write.

use afd_core::{clock, id::Uuid7, timing::LEASE_TTL};
use afd_wire::authoring::{Renew, Renewed};
use jiff_sqlx::ToSqlx as _;
use sqlx::Connection as _;

use super::{failure, guard, meter, sql};
use crate::{
    Drafts, Result,
    error::{ErrorKind, raise::query},
    funnel::Owed,
    model::budget,
};

const CONTEXT: &str = "renew authoring lease";

impl Drafts {
    /// Renews an unexpired, authorized own lease without issuing a new capability.
    ///
    /// # Errors
    /// Refuses stale fences, expiry, lost access/model, capacity or wallet coverage.
    pub async fn renew(&self, runner: &Uuid7, lease: &Uuid7, request: Renew) -> Result<Renewed> {
        let request = garde::Unvalidated::new(request).validate()?;
        let counters = guard::Counters::renew(&request)?;
        let held = budget::held(self, runner, lease).await;
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
        let (verified, remaining_output_tokens) = match available(held, &mut tx, &guarded, &request)
            .await
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
        let until: Option<jiff_sqlx::Timestamp> = sqlx::query_scalar(sql::EXTEND)
            .bind(guarded.attempt.id.as_str())
            .bind(now.to_sqlx())
            .bind(LEASE_TTL)
            .bind(clock::saturating_millis_signed(verified.runtime))
            .fetch_optional(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        let until = until.ok_or(ErrorKind::Fence)?.to_jiff();
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
    held: Result<()>,
    connection: &mut sqlx::PgConnection,
    guarded: &guard::Guarded,
    request: &garde::Valid<Renew>,
) -> Result<(meter::Verified, u64)> {
    guard::current_model(connection, guarded).await?;
    let observed = garde::Unvalidated::new(request.usage.clone()).validate()?;
    let verified = meter::verify(connection, &guarded.attempt, &observed, guarded.now).await?;
    held?;
    let remaining = u64::try_from(
        budget::output_limit(&guarded.attempt.model_snapshot) - guarded.attempt.output_token_count,
    )
    .map_err(|_negative| ErrorKind::Token)?;
    Ok((verified, remaining))
}
