//! Broker loss asks Postgres again; a broker message never grants or removes authority itself.

use std::sync::Arc;

use af_core::timing::{READY_WAKE_DEADLINE, STREAM_ANSWER_TIMEOUT};
use afd_bus::{Command, Received, Subject};

use super::{SessionHandle, SessionServices};
use crate::{Gateway, Operation, Result};

/// Announce committed ownership with a bounded best-effort publish.
pub(super) async fn announce(gateway: &Gateway, runner: &af_core::id::Uuid7) -> Result<()> {
    let subject = Subject::RunnerCommand(runner.clone());
    let publish = gateway
        .inner
        .dependencies
        .bus
        .publish_frame(&subject, &Command::Recheck);
    tokio::time::timeout(READY_WAKE_DEADLINE, publish)
        .await
        .map_err(|source| crate::error::ErrorKind::Timeout {
            operation: Operation::Recheck,
            source,
        })??;
    Ok(())
}

/// Reconnect only the subscription. The database backstop does not depend on broker availability.
pub(super) async fn listen(services: Arc<SessionServices>) -> Result<()> {
    let mut retry = std::time::Duration::from_millis(100);
    let mut lost = false;
    loop {
        if services.handle.stop.is_cancelled() {
            return Ok(());
        }
        let subscription = services
            .operation(Operation::Recheck, async {
                Ok(services
                    .gateway
                    .inner
                    .dependencies
                    .bus
                    .subscribe(&Subject::RunnerCommand(services.handle.runner.clone()))
                    .await?)
            })
            .await;
        if let Ok(mut subscription) = subscription {
            services.handle.command(Command::Recheck);
            lost = false;
            retry = std::time::Duration::from_millis(100);
            match receive(&services.handle, &mut subscription).await {
                Ok(()) => return Ok(()),
                Err(error) => crate::Gateway::observe(&error, Operation::Recheck),
            }
        }
        if !lost {
            services.handle.command(Command::Recheck);
            lost = true;
        }
        let stagger = services
            .handle
            .runner
            .as_str()
            .bytes()
            .fold(0_u64, |sum, byte| sum + u64::from(byte))
            % 251;
        tokio::select! {
            biased;
            () = services.handle.stop.cancelled() => return Ok(()),
            () = tokio::time::sleep(retry + std::time::Duration::from_millis(stagger)) => {}
        }
        retry = retry.saturating_mul(2).min(STREAM_ANSWER_TIMEOUT);
    }
}

/// Typed commands survive decoding; gaps request a complete authority and cancellation refresh.
async fn receive(handle: &SessionHandle, subscription: &mut afd_bus::Subscription) -> Result<()> {
    loop {
        tokio::select! {
            biased;
            () = handle.stop.cancelled() => return Ok(()),
            next = subscription.recv() => match next {
                Ok(Received::Message(message)) => {
                    if let Ok(command) = serde_json::from_str::<Command>(&message.payload) {
                        handle.command(command);
                    } else {
                        tracing::warn!(event = "runner_stream_command_invalid");
                        handle.command(Command::Recheck);
                    }
                }
                Ok(Received::Gap | Received::Lagged(_)) => handle.command(Command::Recheck),
                Err(error) => return Err(error.into()),
            }
        }
    }
}

impl SessionServices {
    /// Recheck reloads ownership and policy; Cancel confirms just its durable lease marker.
    pub(super) async fn command(&self, command: Command) -> Result<()> {
        match command {
            Command::Recheck => self.recheck().await,
            Command::Cancel { lease_id } => self.check_cancellation(&lease_id).await,
        }
    }

    async fn check_cancellation(&self, lease: &af_core::id::Uuid7) -> Result<()> {
        let cancelled = self
            .operation(Operation::Recheck, async {
                Ok(
                    sqlx::query_scalar::<_, bool>(include_str!(
                        "sql/select_lease_cancellation.sql"
                    ))
                    .bind(lease)
                    .bind(&self.handle.runner)
                    .fetch_optional(&mut *self.gateway.inner.dependencies.database.acquire().await?)
                    .await?,
                )
            })
            .await?;
        if cancelled == Some(true) {
            self.cancel(lease, af_core::error_code::RUN_LEASE_CANCELLED.as_str())
                .await?;
        }
        Ok(())
    }
}
