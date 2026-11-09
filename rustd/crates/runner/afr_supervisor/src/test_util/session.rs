//! Integration fixtures use the production downlink, not an HTTP polling adapter.

use crate::{
    Result,
    client::{ControlPlane, HttpRunnerApi},
    config::Config,
    downlink::{Downlink, Offered, running::LeaseRegistration},
    error::ErrorKind,
    halt::Halt,
};
use af_core::id::Uuid7;
use af_stream::OwnedFrame;
use af_wire::{
    activity::ActivityRequest,
    report::{RenewRequest, RenewResponse},
    runner::HeartbeatRequest,
};
use parking_lot::Mutex;
use std::{collections::HashMap, sync::Arc};
use tokio::{sync::mpsc, time::Instant};
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

/// A native runner session with caller-driven work, backed by production recovery and correlation.
#[derive(Debug)]
pub struct Session {
    plane: ControlPlane,
    offers: tokio::sync::Mutex<mpsc::Receiver<Offered>>,
    held: Mutex<HashMap<Uuid7, LeaseRegistration>>,
    halt: Arc<Halt>,
    owner: AbortOnDropHandle<Result<()>>,
}

impl Session {
    /// Open the configured native origin and retain its supervised downlink.
    ///
    /// # Errors
    /// Returns configuration, authentication or connection failures before the opening deadline.
    pub async fn connect(config: &Config) -> Result<Self> {
        let (stream, downlink, offers) = Downlink::new(config)?;
        let halt = Arc::new(Halt::new(CancellationToken::new()));
        let serving_halt = Arc::clone(&halt);
        let owner =
            AbortOnDropHandle::new(tokio::spawn(
                async move { downlink.run(&serving_halt).await },
            ));
        let session = Self {
            plane: ControlPlane::new(Box::new(HttpRunnerApi::new(config)?), stream),
            offers: tokio::sync::Mutex::new(offers),
            held: Mutex::new(HashMap::new()),
            halt,
            owner,
        };
        session
            .plane
            .stream
            .wait_connected(Instant::now() + af_core::timing::STREAM_ANSWER_TIMEOUT)
            .await?;
        Ok(session)
    }

    /// Advertise physical worker capacity; held registrations consume their own slots.
    pub fn capacity(&self, workers: usize) {
        self.plane.stream.running.capacity(workers);
    }

    /// Apply the current capability report and return the complete assigned policy.
    ///
    /// # Errors
    /// Preserves native transport and coded domain refusals.
    pub async fn heartbeat(&self, request: &HeartbeatRequest<'_>) -> Result<serde_json::Value> {
        self.plane.heartbeat(request).await?.decode()
    }

    /// Receive an issued offer while retaining its cancellation and worker registration.
    ///
    /// # Errors
    /// Returns a disconnected owner or a malformed protocol offer.
    pub async fn offer(&self) -> Result<OwnedFrame> {
        let offered = self
            .offers
            .lock()
            .await
            .recv()
            .await
            .ok_or(ErrorKind::Disconnected)?;
        self.held
            .lock()
            .insert(offered.registration.lease.clone(), offered.registration);
        Ok(offered.frame)
    }

    /// Deliver the current report and release its registration only after acknowledgement.
    ///
    /// # Errors
    /// Preserves malformed input, native transport failures and coded settlement refusals.
    pub async fn report(&self, bytes: &[u8]) -> Result<af_wire::report::ReportResponse> {
        let request: af_wire::report::ReportRequest<'_> =
            af_core::json::object_from_slice(bytes)
                .map_err(crate::error::malformed(crate::client::Verb::Report))?;
        let lease = Uuid7::parse(&request.lease_id)?;
        let response = self
            .plane
            .stream
            .compute(crate::client::stream::Report::try_from(bytes)?)
            .await?;
        self.held.lock().remove(&lease);
        Ok(response)
    }

    /// Renew one lease through the production batch computation.
    ///
    /// # Errors
    /// Preserves native failures and the lease's domain refusal.
    pub async fn renew(&self, lease: &Uuid7, request: &RenewRequest) -> Result<RenewResponse> {
        self.plane.renew(lease, request).await
    }

    /// Keep a real lease meter registered with the host renewal ticker until cancellation.
    pub async fn keep_usage(
        &self,
        lease: &Uuid7,
        expires: jiff::Timestamp,
        meter: &Arc<afr_agent::Meter>,
    ) -> af_wire::report::FailureClass {
        crate::renew::Renewal::new(
            &self.plane,
            lease,
            expires,
            &af_core::clock::SystemClock,
            meter,
        )
        .keep()
        .await
    }

    /// Publish bounded activity over its independent stream.
    ///
    /// # Errors
    /// Returns transport, encoding or queue failures.
    pub async fn activity(&self, lease: &Uuid7, request: &ActivityRequest<'_>) -> Result<()> {
        self.plane.activity(lease, request).await
    }

    /// Read pending folds under the current lease fence.
    ///
    /// # Errors
    /// Preserves native and domain refusals.
    pub async fn folds(&self, lease: &Uuid7, fence: u64, after: i64) -> Result<serde_json::Value> {
        self.plane.folds(lease, fence, after).await?.decode()
    }

    /// Invoke a model through the same native connection as the lease offer.
    ///
    /// # Errors
    /// Preserves native transport failures and coded authoring refusals.
    pub async fn authoring_model(
        &self,
        lease: &Uuid7,
        request: &af_wire::authoring::ModelCall,
    ) -> Result<af_wire::authoring::ModelResult> {
        self.plane.authoring_model(lease, request).await
    }

    /// Submit a proposal and receive its durable activity receipt.
    ///
    /// # Errors
    /// Preserves native transport failures and coded authoring refusals.
    pub async fn authoring_activity(
        &self,
        lease: &Uuid7,
        request: &af_wire::authoring::ActivityRequest,
    ) -> Result<af_wire::authoring::ActivityAccepted> {
        self.plane.authoring_activity(lease, request).await
    }

    /// Renew the authoring lease under this connection's authority.
    ///
    /// # Errors
    /// Preserves native transport failures and coded authoring refusals.
    pub async fn authoring_renew(
        &self,
        lease: &Uuid7,
        request: &af_wire::authoring::Renew,
    ) -> Result<af_wire::authoring::Renewed> {
        self.plane.authoring_renew(lease, request).await
    }

    /// Complete authoring, retaining the worker until accounting is settled.
    ///
    /// # Errors
    /// Preserves native transport failures and coded settlement refusals.
    pub async fn authoring_report(
        &self,
        lease: &Uuid7,
        request: &af_wire::authoring::Report,
    ) -> Result<af_wire::authoring::Reported> {
        let result = self.plane.authoring_report(lease, request).await?;
        if result.settled {
            self.held.lock().remove(lease);
        }
        Ok(result)
    }

    /// Stop recovery and join the downlink and native child tasks.
    ///
    /// # Errors
    /// Returns an owner failure or an expired shutdown deadline.
    pub async fn close(&mut self) -> Result<()> {
        self.halt.stop();
        tokio::time::timeout(af_core::timing::STREAM_ANSWER_TIMEOUT, &mut self.owner).await???;
        Ok(())
    }

    /// Interrupt native I/O while an integration test retains its outstanding request future.
    pub fn disconnect(&self) {
        self.halt.stop();
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.halt.stop();
    }
}
