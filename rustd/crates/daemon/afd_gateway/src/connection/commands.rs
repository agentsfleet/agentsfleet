//! Broker loss asks Postgres again; a broker message never grants or removes authority itself.

use std::sync::Arc;

use af_core::timing::{READY_WAKE_DEADLINE, STREAM_ANSWER_TIMEOUT};
use afd_bus::{Command, Received, Subject};
use backon::{BackoffBuilder as _, ExponentialBuilder};

use super::{ConnectionHandle, ConnectionServices};
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

/// Resubscribe waits: 100 ms, doubling to the stream's answer timeout, for as
/// long as the connection lives.
const RESUBSCRIBE: ExponentialBuilder = ExponentialBuilder::new()
    .with_factor(2.0)
    .with_min_delay(std::time::Duration::from_millis(100))
    .with_max_delay(STREAM_ANSWER_TIMEOUT)
    .without_max_times();

/// Reconnect only the subscription. The database backstop does not depend on broker availability.
pub(super) async fn listen(services: Arc<ConnectionServices>) -> Result<()> {
    let mut pauses = RESUBSCRIBE.build();
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
            pauses = RESUBSCRIBE.build();
            match receive(&services.handle, &mut subscription).await {
                Ok(()) => return Ok(()),
                Err(error) => Gateway::observe(&error, Operation::Recheck),
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
        // The schedule has no last attempt, so it always has a wait.
        let pause = pauses.next().unwrap_or_default() + std::time::Duration::from_millis(stagger);
        tokio::select! {
            biased;
            () = services.handle.stop.cancelled() => return Ok(()),
            () = tokio::time::sleep(pause) => {}
        }
    }
}

/// Typed commands survive decoding; gaps request a complete authority and cancellation refresh.
async fn receive(
    handle: &ConnectionHandle,
    subscription: &mut afd_bus::Subscription,
) -> Result<()> {
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

impl ConnectionServices {
    /// Recheck reloads ownership and policy; Cancel confirms just its durable run marker.
    pub(super) async fn command(&self, command: Command) -> Result<()> {
        match command {
            Command::Recheck => self.recheck().await,
            Command::Cancel { run_id } => self.check_cancellation(&run_id).await,
        }
    }

    async fn check_cancellation(&self, run: &af_core::id::Uuid7) -> Result<()> {
        let cancelled = self
            .operation(Operation::Recheck, async {
                Ok(
                    sqlx::query_scalar::<_, bool>(include_str!("sql/select_run_cancellation.sql"))
                        .bind(run)
                        .bind(&self.handle.runner)
                        .fetch_optional(
                            &mut *self.gateway.inner.dependencies.database.acquire().await?,
                        )
                        .await?,
                )
            })
            .await?;
        if cancelled == Some(true) {
            self.cancel(run, af_core::error_code::RUN_CANCELLED.as_str())
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! A broker message names a command or asks for a full recheck; it never grants authority.

    use super::receive;
    use crate::connection::ConnectionHandle;
    use af_core::id::Uuid7;
    use afd_bus::{Command, Subject};
    use std::time::Duration;

    type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

    async fn next(
        commands: &mut tokio::sync::mpsc::Receiver<Command>,
    ) -> Result<Option<Command>, tokio::time::error::Elapsed> {
        tokio::time::timeout(Duration::from_secs(5), commands.recv()).await
    }

    #[tokio::test]
    async fn commands_reach_the_connection_and_noise_or_a_gap_asks_for_a_recheck() -> TestResult {
        let (bus, server) = afd_bus::test_util::detached()?;
        let runner = Uuid7::parse("019f0000-0000-7000-8000-000000000001")?;
        let (handle, _passes, mut commands) =
            ConnectionHandle::fixture_with_queues(runner.clone(), runner.clone());
        let subject = Subject::RunnerCommand(runner);
        let mut subscription = bus.subscribe(&subject).await?;
        let receiving = tokio::spawn({
            let handle = std::sync::Arc::clone(&handle);
            async move { receive(&handle, &mut subscription).await }
        });
        let run = Uuid7::parse("019f0000-0000-7000-8000-0000000000aa")?;
        let cancel = Command::Cancel { run_id: run };
        server.publish(&subject, &serde_json::to_string(&cancel)?);
        assert_eq!(next(&mut commands).await?, Some(cancel));
        server.publish(&subject, r#"{"type":"grant","runner":"everything"}"#);
        assert_eq!(
            next(&mut commands).await?,
            Some(Command::Recheck),
            "an unreadable command"
        );
        server.gap(&subject);
        assert_eq!(next(&mut commands).await?, Some(Command::Recheck), "a gap");
        handle.stop.cancel();
        receiving.await??;
        Ok(())
    }
}
