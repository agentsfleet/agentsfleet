//! Lock observations identify native grant statements before replacing durable authority.

use super::TestResult;
use crate::e2e::Scenario;
use af_crypto::entropy::Entropy;
use afd_auth::{credential::CredentialKind, minted::Minted};
use sqlx::{Postgres, Transaction};
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub(super) enum Replacement {
    Rotate,
    Supersede,
}

pub(super) struct HostLock {
    transaction: Transaction<'static, Postgres>,
    blocker: i32,
}

impl HostLock {
    pub(super) async fn hold(run: &Scenario) -> TestResult<Self> {
        let mut transaction =
            Transaction::begin(run.booted.database.acquire().await?, None).await?;
        // Let admission foreign keys take KEY SHARE before the issue reaches its host lock.
        let blocker = sqlx::query_scalar(
            "SELECT pg_backend_pid() FROM runner.runners WHERE id=$1 FOR NO KEY UPDATE",
        )
        .bind(&run.runner_id)
        .fetch_one(&mut *transaction)
        .await?;
        Ok(Self {
            transaction,
            blocker,
        })
    }

    pub(super) async fn blocked(&self, run: &Scenario) -> TestResult<i32> {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let waiting: Option<i32> = sqlx::query_scalar(
                    "SELECT pid FROM pg_stat_activity \
                     WHERE $1=ANY(pg_blocking_pids(pid)) AND query LIKE '%AS authorized%' \
                     AND query LIKE '%FOR UPDATE%' LIMIT 1",
                )
                .bind(self.blocker)
                .fetch_optional(&mut *run.booted.database.acquire().await?)
                .await?;
                if let Some(waiter) = waiting {
                    return Ok::<_, Box<dyn std::error::Error + Send + Sync>>(waiter);
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await?
    }

    pub(super) async fn replace(
        mut self,
        run: &Scenario,
        replacement: Replacement,
    ) -> TestResult<String> {
        // This transaction deliberately emits no command: the native waiter must
        // consult its locked database proof even when every ownership hint is lost.
        let token = match replacement {
            Replacement::Rotate => {
                let token = Minted::draw(CredentialKind::RunnerToken, &Entropy::new())?;
                sqlx::query("UPDATE runner.runners SET token_hash=$2 WHERE id=$1")
                    .bind(&run.runner_id)
                    .bind(token.digest().as_str())
                    .execute(&mut *self.transaction)
                    .await?;
                token.expose().to_owned()
            }
            Replacement::Supersede => {
                sqlx::query("UPDATE runner.runners SET connection_id=$2 WHERE id=$1")
                    .bind(&run.runner_id)
                    .bind(afd_db::test_util::mint_id())
                    .execute(&mut *self.transaction)
                    .await?;
                run.token.clone()
            }
        };
        self.transaction.commit().await?;
        Ok(token)
    }
}

/// An issue's host-lock transaction must have ended before checking its durable effects.
pub(super) async fn finished(run: &Scenario, backend: i32) -> TestResult {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let ended: bool = sqlx::query_scalar(
                "SELECT NOT EXISTS(SELECT 1 FROM pg_stat_activity WHERE pid=$1 AND xact_start IS NOT NULL)",
            ).bind(backend).fetch_one(&mut *run.booted.database.acquire().await?).await?;
            if ended { return Ok::<_, Box<dyn std::error::Error + Send + Sync>>(()); }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }).await?
}
