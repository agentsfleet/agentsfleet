//! Scripted peer on the shipping native transports, with real framing and opening.

#[cfg(test)]
mod relay;
mod reply;

use std::{convert::Infallible, sync::Arc};

use af_stream::{Budget, Incoming, Outgoing, Reader, server::Listener};
use af_wire::protocol::{self, StreamKind, Up};
use http::{Request, Response};
use serde_json::Value;
use tokio::{
    io::DuplexStream,
    sync::{Mutex, Notify},
    time::Instant,
};
use tokio_util::{
    sync::CancellationToken,
    task::{AbortOnDropHandle, TaskTracker},
};

use super::Healthy;
use crate::{Result, error::ErrorKind};

/// Loopback native daemon whose observer can retain or stall current protocol frames.
#[derive(Debug)]
pub struct NativeDaemon {
    /// HTTPS origin configured on the runner.
    pub url: String,
    identity: af_stream::test_util::IdentityFixture,
    stop: CancellationToken,
    task: AbortOnDropHandle<af_stream::Result<()>>,
    workers: TaskTracker,
    failure: Arc<parking_lot::Mutex<Option<crate::Error>>>,
    #[cfg(test)]
    cancellation: Arc<parking_lot::Mutex<Option<af_core::id::Uuid7>>>,
}

impl NativeDaemon {
    /// Serve one pushed lease and healthy protocol answers on both native transports.
    ///
    /// # Errors
    /// Preserves identity creation, listener binding and address failures.
    pub async fn serve<H, F>(
        healthy: Healthy,
        lease: Value,
        observe: H,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>>
    where
        H: Fn(Value) -> F + Send + Sync + 'static,
        F: Future<Output = ()> + Send,
    {
        Self::start(healthy, Some(lease), observe).await
    }

    /// Serve idle native sessions without injecting a lease offer.
    ///
    /// # Errors
    /// Preserves identity creation, listener binding and address failures.
    pub async fn serve_idle<H, F>(
        healthy: Healthy,
        observe: H,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>>
    where
        H: Fn(Value) -> F + Send + Sync + 'static,
        F: Future<Output = ()> + Send,
    {
        Self::start(healthy, None, observe).await
    }

    async fn start<H, F>(
        healthy: Healthy,
        lease: Option<Value>,
        observe: H,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>>
    where
        H: Fn(Value) -> F + Send + Sync + 'static,
        F: Future<Output = ()> + Send,
    {
        let identity = af_stream::test_util::IdentityFixture::new(vec!["localhost".into()])?;
        let listener = Listener::bind("[::1]:0".parse()?, identity.identity()).await?;
        let url = format!("https://localhost:{}", listener.local_addr()?.port());
        let stop = CancellationToken::new();
        let workers = TaskTracker::new();
        let failure = Arc::new(parking_lot::Mutex::new(None));
        let cancellation = Arc::new(parking_lot::Mutex::new(None));
        let peer = Arc::new(Peer {
            healthy,
            lease: Mutex::new(lease),
            ready: Notify::new(),
            observe,
            workers: workers.clone(),
            failure: Arc::clone(&failure),
            cancellation: Arc::clone(&cancellation),
        });
        let task = tokio::spawn(listener.serve(
            move |connection| {
                let peer = Arc::clone(&peer);
                tower::service_fn(move |request| {
                    let peer = Arc::clone(&peer);
                    let stop = connection.stop.clone();
                    async move { Ok::<_, Infallible>(peer.open(request, stop)) }
                })
            },
            stop.clone(),
            stop.clone(),
        ));
        Ok(Self {
            url,
            identity,
            stop,
            task: AbortOnDropHandle::new(task),
            workers,
            failure,
            #[cfg(test)]
            cancellation,
        })
    }

    /// Push a lease cancellation immediately after its next renewal acknowledgement.
    #[cfg(test)]
    pub(crate) fn cancel_after_renewal(&self, lease: &af_core::id::Uuid7) {
        self.cancellation.lock().replace(lease.clone());
    }

    /// Leave UDP unanswered while forwarding TCP to this same native listener.
    #[cfg(test)]
    pub(crate) async fn tcp_only(
        &self,
    ) -> Result<relay::TcpOnly, Box<dyn std::error::Error + Send + Sync>> {
        relay::TcpOnly::start(&self.url).await
    }

    /// Public root bundle for the runtime-generated issuer.
    #[must_use]
    pub fn root(&self) -> std::path::PathBuf {
        self.identity.root()
    }

    /// Cancel native owners and wait for both transports to join.
    ///
    /// # Errors
    /// Preserves shutdown timeouts, task failures and native listener failures.
    pub async fn close(&mut self) -> Result<()> {
        self.stop.cancel();
        tokio::time::timeout(af_core::timing::STREAM_ANSWER_TIMEOUT, async {
            (&mut self.task).await??;
            self.workers.close();
            self.workers.wait().await;
            Ok::<_, crate::Error>(())
        })
        .await??;
        let failure = self.failure.lock().take();
        if let Some(failure) = failure {
            return Err(failure);
        }
        Ok(())
    }
}

impl Drop for NativeDaemon {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

struct Peer<H> {
    healthy: Healthy,
    lease: Mutex<Option<Value>>,
    ready: Notify,
    observe: H,
    workers: TaskTracker,
    failure: Arc<parking_lot::Mutex<Option<crate::Error>>>,
    cancellation: Arc<parking_lot::Mutex<Option<af_core::id::Uuid7>>>,
}

impl<H, F> Peer<H>
where
    H: Fn(Value) -> F + Send + Sync + 'static,
    F: Future<Output = ()> + Send,
{
    fn open(
        self: Arc<Self>,
        request: Request<Incoming>,
        stop: CancellationToken,
    ) -> Response<Outgoing> {
        let kind = request
            .headers()
            .get(protocol::STREAM_KIND_HEADER)
            .and_then(|value| value.to_str().ok())
            .and_then(StreamKind::parse);
        let Some(kind) = kind else {
            let (writer, body) = Outgoing::pipe();
            drop(writer);
            return Response::builder()
                .status(400)
                .body(body)
                .unwrap_or_else(|never| unreachable!("literal status: {never}"));
        };
        let (writer, body) = Outgoing::pipe();
        self.workers.clone().spawn(async move {
            let result = self.run(request.into_body(), writer, kind, &stop).await;
            if let Err(failure) = result
                && !stop.is_cancelled()
            {
                *self.failure.lock() = Some(failure);
                stop.cancel();
            }
        });
        let mut response = Response::new(body);
        response.headers_mut().insert(
            protocol::VERSION_HEADER,
            http::HeaderValue::from_static(protocol::VERSION),
        );
        response
    }

    async fn run(
        &self,
        body: Incoming,
        mut writer: DuplexStream,
        kind: StreamKind,
        stop: &CancellationToken,
    ) -> Result<()> {
        let budget = Budget::default();
        let mut reader = Reader::new(
            body.reader(),
            budget.clone(),
            kind,
            stop.clone(),
            af_core::timing::STREAM_ANSWER_TIMEOUT,
        );
        if kind == StreamKind::Work {
            tokio::select! { () = stop.cancelled() => return Ok(()), () = self.ready.notified() => {} }
            let lease = self.lease.lock().await.take();
            if let Some(lease) = lease {
                send(
                    &mut writer,
                    &serde_json::json!({"type":"offer", "lease":lease}),
                    &budget,
                    kind,
                    stop,
                )
                .await?;
            }
        }
        loop {
            let next = reader.next().await?;
            let Some(frame) = next else {
                break;
            };
            let message: Up<'_> = frame.decode()?;
            if message.stream_kind() != kind {
                return Err(ErrorKind::StreamProtocol.into());
            }
            tokio::select! {
                () = stop.cancelled() => return Ok(()),
                () = (self.observe)(frame.decode()?) => {},
            }
            if let Up::Ready { credits, .. } = &message
                && *credits > 0
            {
                self.ready.notify_one();
            }
            self.answer(&mut writer, &message, &budget, kind, stop)
                .await?;
        }
        Ok(())
    }

    async fn answer(
        &self,
        writer: &mut DuplexStream,
        message: &Up<'_>,
        budget: &Budget,
        kind: StreamKind,
        stop: &CancellationToken,
    ) -> Result<()> {
        if let Some(response) = reply::answer(&self.healthy, message)? {
            send(writer, &response, budget, kind, stop).await?;
        }
        if matches!(message, Up::RenewBatch { .. }) {
            let cancelled = self.cancellation.lock().take();
            if let Some(lease) = cancelled {
                let response = serde_json::json!({"type":"cancel", "lease_id":lease,
                    "error_code":af_core::error_code::RUN_LEASE_CANCELLED.as_str()});
                send(writer, &response, budget, kind, stop).await?;
            }
        }
        Ok(())
    }
}

async fn send(
    writer: &mut DuplexStream,
    value: &Value,
    budget: &Budget,
    kind: StreamKind,
    stop: &CancellationToken,
) -> Result<()> {
    let deadline = Instant::now() + af_core::timing::STREAM_ANSWER_TIMEOUT;
    let frame = af_stream::encode(value, budget, kind, stop, deadline).await?;
    af_stream::write(writer, &frame, stop, deadline).await?;
    Ok(())
}
