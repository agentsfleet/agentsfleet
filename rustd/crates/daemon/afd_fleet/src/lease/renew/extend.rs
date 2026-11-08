//! The transaction owns host, slot and lease locks through its final decision.

use af_core::id::Uuid7;
use afd_billing::biller::Settlement;
use jiff::Timestamp;
use sqlx::{Connection as _, FromRow as _, Postgres, Transaction};

use super::{RenewAnswer, Renewed};
use crate::{
    Result,
    error::query,
    lease::{
        sql::{
            self,
            meter::{Ending, Metering},
        },
        store::Leases,
        usage::{self, Pricing, SliceColumns},
    },
};

const CONTEXT: &str = "renew and meter";

struct Extension<'a> {
    store: &'a Leases,
    lease: Metering<'a>,
    key: &'a Uuid7,
    pricing: &'a Pricing,
    now: Timestamp,
}

impl Leases {
    /// Fence, measure and extend together; cancellation never grants another slice.
    ///
    /// # Errors
    /// Preserves database, pricing and entropy failures with their original causes.
    pub async fn extend(
        &self,
        lease: Metering<'_>,
        key: &Uuid7,
        pricing: &Pricing,
        now: Timestamp,
    ) -> Result<Renewed> {
        let mut connection = self.pool().acquire().await?;
        let transaction = connection.begin().await.map_err(query(CONTEXT))?;
        Extension {
            store: self,
            lease,
            key,
            pricing,
            now,
        }
        .run(transaction)
        .await
    }
}

impl Extension<'_> {
    async fn run(self, mut transaction: Transaction<'_, Postgres>) -> Result<Renewed> {
        let capacity = self
            .store
            .lock_capacity(&mut transaction, self.lease.runner_id)
            .await?;
        let Some(capacity) = capacity.filter(|capacity| capacity.can_renew()) else {
            return self.refuse(transaction, Renewed::Lost).await;
        };
        let cancelled: Option<bool> = sqlx::query_scalar(sql::renew::LOCK_CANCELLED)
            .bind(self.lease.lease_id)
            .bind(self.lease.runner_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(query(CONTEXT))?;
        match cancelled {
            Some(false) => {}
            Some(true) => return self.refuse(transaction, Renewed::Cancelled).await,
            None => return self.refuse(transaction, Renewed::Lost).await,
        }
        let row = self
            .lease
            .bind(Ending::Renew)
            .fetch_one(&mut *transaction)
            .await
            .map_err(query(CONTEXT))?;
        let answer = RenewAnswer::from_row(&row).map_err(query(CONTEXT))?;
        match answer.extended() {
            Some(expires_at) => {
                let slice = SliceColumns::from_row(&row).map_err(query(CONTEXT))?;
                self.extend(transaction, expires_at, slice).await
            }
            None => {
                self.refuse(transaction, answer.verdict(capacity.now()))
                    .await
            }
        }
    }

    async fn refuse(
        self,
        mut transaction: Transaction<'_, Postgres>,
        verdict: Renewed,
    ) -> Result<Renewed> {
        let settlement = self
            .pricing
            .original_requests(
                &mut transaction,
                self.key,
                self.lease.runner_id,
                self.lease.requests,
            )
            .await?;
        self.commit(transaction, settlement.as_ref()).await?;
        Ok(match (verdict, settlement) {
            (Renewed::Cancelled, _) => Renewed::Cancelled,
            (_, Some(settlement)) => Renewed::Stopped {
                settlement: Some(Box::new(settlement)),
            },
            (verdict, None) => verdict,
        })
    }

    async fn extend(
        self,
        transaction: Transaction<'_, Postgres>,
        expires_at: Timestamp,
        columns: SliceColumns,
    ) -> Result<Renewed> {
        let settlement = columns.into_slice().map_or(Ok(None), |slice| {
            self.pricing
                .settlement(self.key, slice, self.lease.requests)
        })?;
        let allowed = self.commit(transaction, settlement.as_ref()).await?;
        let settlement = settlement.map(Box::new);
        Ok(if allowed {
            Renewed::Extended {
                expires_at,
                settlement,
            }
        } else {
            Renewed::Stopped { settlement }
        })
    }

    async fn commit(
        &self,
        mut transaction: Transaction<'_, Postgres>,
        settlement: Option<&Settlement>,
    ) -> Result<bool> {
        let allowed =
            usage::log_measured(&mut transaction, self.store.entropy(), self.now, settlement)
                .await?;
        transaction.commit().await.map_err(query(CONTEXT))?;
        Ok(allowed)
    }
}
