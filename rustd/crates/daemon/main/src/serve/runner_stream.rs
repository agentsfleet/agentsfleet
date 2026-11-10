//! Native serving and scheduling share the gateway assembled by the HTTP plane.

use std::sync::Arc;

use af_core::error_code::{Coded, Logged};

use af_stream::Listener;
use afd_gateway::Gateway;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt as _;

use super::drain::Drain;
use crate::{
    error::BootFailure, preflight::runner_listener::RunnerListenerConfig, supervisor::Supervisor,
};

#[derive(Clone, Debug, Default)]
pub(super) struct Signals {
    pub(super) stop: CancellationToken,
    pub(super) closed: CancellationToken,
}

pub(super) async fn bind(config: &RunnerListenerConfig) -> Result<Listener, BootFailure> {
    Ok(Listener::bind(config.address, config.identity.clone()).await?)
}

pub(super) fn spawn(
    listener: Listener,
    router: axum::Router,
    gateway: Gateway,
    drain: Drain,
    signals: Signals,
    failed: CancellationToken,
    supervisor: &mut Supervisor,
) {
    let scheduler = gateway;
    let failed_scheduler = failed.clone();
    supervisor.spawn("runner_stream_scheduler", async move |abort| {
        let result = scheduler.run(abort.child_token()).await;
        finished(&result, "scheduler", &abort, &failed_scheduler);
    });
    supervisor.spawn("runner_stream_listener", async move |abort| {
        let _closed = signals.closed.drop_guard();
        let shutdown = signals.stop;
        let accepting = drain.accepting().clone();
        let serving = listener.serve(
            move |info| {
                let service = Gateway::connection_service(router.clone(), info);
                let live = Arc::new(drain.enter());
                tower::service_fn(move |request| {
                    let service = service.clone();
                    let live = Arc::clone(&live);
                    async move {
                        let _live = live;
                        service.oneshot(request).await
                    }
                })
            },
            accepting.clone(),
            shutdown.clone(),
        );
        tokio::pin!(serving);
        let result = tokio::select! {
            result = &mut serving => result,
            () = abort.cancelled() => {
                shutdown.cancel();
                serving.await
            }
        };
        finished(&result, "listener", &accepting, &failed);
    });
}

fn finished<E>(
    result: &Result<(), E>,
    component: &'static str,
    stop: &CancellationToken,
    failed: &CancellationToken,
) where
    E: Coded,
{
    if stop.is_cancelled() {
        // Drain owns the exit status; a component's own failure is still reported with its cause.
        if let Err(error) = result {
            let Logged { error_code, reason } = error.logged();
            tracing::warn!(
                event = "runner_stream_component_stopped",
                component,
                error_code,
                reason
            );
        } else {
            tracing::info!(event = "runner_stream_component_stopped", component);
        }
        return;
    }
    let (outcome, Logged { error_code, reason }) = match result {
        Err(error) => ("failed", error.logged()),
        Ok(()) => (
            "unexpected_return",
            Logged {
                error_code: af_core::error_code::INTERNAL_OPERATION_FAILED.as_str(),
                reason: "the native component returned before shutdown".to_owned(),
            },
        ),
    };
    tracing::error!(
        event = "runner_stream_component_failed",
        error_code,
        component,
        outcome,
        reason,
    );
    failed.cancel();
}

#[cfg(test)]
#[path = "runner_stream_tests.rs"]
mod tests;
