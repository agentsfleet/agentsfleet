//! Observe committed Postgres state at the actual bus publication boundary.

use afd_bus::{Bus, Command, Handle, Result, Subject, Subscription};
use afd_db::Db;
use async_trait::async_trait;
use tokio::sync::mpsc;

use super::TestResult;

#[derive(Debug)]
pub(super) struct BeforePublish {
    pub(super) bus: Handle,
    pub(super) database: Db,
    pub(super) observed: mpsc::UnboundedSender<TestResult<bool>>,
}

#[async_trait]
impl Bus for BeforePublish {
    async fn publish(&self, subject: &Subject, payload: &str) -> Result<()> {
        if matches!(subject, Subject::RunnerCommand(_))
            && let Ok(Command::Cancel { lease_id }) = serde_json::from_str(payload)
        {
            let visible = async {
                let visible = sqlx::query_scalar(
                    "SELECT cancel_requested_at IS NOT NULL FROM runner.leases WHERE id = $1",
                )
                .bind(lease_id)
                .fetch_one(&mut *self.database.acquire().await?)
                .await?;
                Ok::<_, Box<dyn std::error::Error + Send + Sync>>(visible)
            }
            .await;
            let _test_finished = self.observed.send(visible);
        }
        self.bus.publish(subject, payload).await
    }

    async fn publish_batch(&self, subject: &Subject, payloads: &[String]) -> Result<()> {
        self.bus.publish_batch(subject, payloads).await
    }

    async fn subscribe(&self, subject: &Subject) -> Result<Subscription> {
        self.bus.subscribe(subject).await
    }

    async fn shutdown(&self) -> Result<()> {
        self.bus.shutdown().await
    }
}
