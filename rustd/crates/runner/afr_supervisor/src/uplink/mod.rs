//! Workers share typed session operations; the native connection stays with the downlink owner.

pub(crate) mod queues;
pub(crate) mod renewals;
pub(crate) mod writer;

use std::{num::NonZeroU64, sync::Arc};

use af_stream::{Budget, OwnedFrame};
use af_wire::protocol::{StreamKind, Up};
use tokio::{sync::watch, time::Instant};
use tokio_util::sync::CancellationToken;

use crate::{
    Result,
    downlink::pending::{Expected, Requests},
    error::ErrorKind,
};
use queues::{Outbound, Queues};

/// Bound to one physical session; stale handles cannot send through its replacement.
#[derive(Debug)]
pub(crate) struct SessionSender {
    pub(crate) requests: Arc<Requests>,
    pub(crate) queues: Queues,
    pub(crate) budget: Budget,
    pub(crate) stop: CancellationToken,
}

/// Absence means disconnected, and every operation returns that failure to its caller.
#[derive(Clone, Debug)]
pub(crate) struct StreamHandle {
    session: watch::Receiver<Option<Arc<SessionSender>>>,
    pub(crate) running: Arc<crate::downlink::running::Running>,
    pub(crate) assignments: watch::Receiver<Option<Arc<OwnedFrame>>>,
    pub(crate) renewing: Arc<renewals::Renewals>,
}

impl StreamHandle {
    pub(crate) fn new(
        session: watch::Receiver<Option<Arc<SessionSender>>>,
        running: Arc<crate::downlink::running::Running>,
        assignments: watch::Receiver<Option<Arc<OwnedFrame>>>,
    ) -> Self {
        Self {
            session,
            running,
            assignments,
            renewing: Arc::new(renewals::Renewals::default()),
        }
    }

    #[cfg(feature = "test-util")]
    pub(crate) async fn wait_connected(&self, deadline: Instant) -> Result<()> {
        let mut session = self.session.clone();
        tokio::time::timeout_at(deadline, async {
            loop {
                if session
                    .borrow_and_update()
                    .as_ref()
                    .is_some_and(|owner| !owner.stop.is_cancelled())
                {
                    return Ok(());
                }
                session
                    .changed()
                    .await
                    .map_err(crate::error::stream_disconnected)?;
            }
        })
        .await?
    }

    fn connected(&self) -> Result<Arc<SessionSender>> {
        self.session
            .borrow()
            .as_ref()
            .filter(|session| !session.stop.is_cancelled())
            .cloned()
            .ok_or_else(|| ErrorKind::Disconnected.into())
    }

    pub(crate) async fn request<'a, F>(
        &self,
        expected: Expected,
        make: F,
        deadline: Instant,
    ) -> Result<OwnedFrame>
    where
        F: FnOnce(NonZeroU64) -> Up<'a> + Send,
    {
        let session = self.connected()?;
        let operation = async {
            let mut registration = session.requests.register(expected).await?;
            let message = make(registration.id);
            let write_deadline =
                deadline.min(Instant::now() + af_core::timing::STREAM_ANSWER_TIMEOUT);
            session.send(&message, write_deadline).await?;
            registration.receive().await
        };
        tokio::select! {
            biased;
            () = session.stop.cancelled() => Err(ErrorKind::Disconnected.into()),
            result = tokio::time::timeout_at(deadline, operation) => match result {
                Ok(answer) => answer,
                Err(timeout) => {
                    // A missing answer invalidates this connection's delivery path.
                    // Dropping an individual caller still only drops its registration.
                    session.stop.cancel();
                    Err(timeout.into())
                }
            },
        }
    }

    pub(crate) async fn send(&self, message: &Up<'_>) -> Result<()> {
        self.connected()?
            .send(
                message,
                Instant::now() + af_core::timing::STREAM_ANSWER_TIMEOUT,
            )
            .await
    }
}

impl SessionSender {
    pub(crate) async fn send(&self, message: &Up<'_>, deadline: Instant) -> Result<()> {
        let kind = message.stream_kind();
        let priority = matches!(
            message,
            Up::Ping { .. } | Up::RenewBatch { .. } | Up::Ready { .. } | Up::AuthoringRenew { .. }
        );
        let frame = af_stream::encode(message, &self.budget, kind, &self.stop, deadline).await?;
        let message = Outbound { frame, deadline };
        if kind == StreamKind::Activity {
            return self.queues.activity(message);
        }
        self.queues.send(kind, priority, message, &self.stop).await
    }
}
