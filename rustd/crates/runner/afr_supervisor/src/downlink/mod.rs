//! The downlink owns connection recovery and every task attached to the current transport.

mod connect;
mod dispatch;
mod io;
mod observation;
pub(crate) mod pending;
pub(crate) mod policy;
#[cfg(test)]
mod pressure_tests;
mod reconnect;
pub(crate) mod running;

use std::{collections::HashSet, sync::Arc};

use af_stream::{Budget, OwnedFrame};
use af_wire::protocol::{LEASES_MAX, MESSAGE_BYTES_MAX, StreamKind};
use http::header::HeaderValue;
use tokio::{
    sync::{mpsc, watch},
    task::JoinSet,
    time::Instant,
};
use tokio_util::sync::CancellationToken;
use tracing::Instrument as _;

use crate::{
    Result,
    config::Config,
    error::{self, ErrorKind},
    halt::Halt,
    uplink::{SessionSender, StreamHandle, queues::Queues},
};
use connect::Connected;
use dispatch::{Dispatch, Progress};
use reconnect::{Preference, Recovery, Resume};
use running::{LeaseRegistration, Running};

#[derive(Debug)]
pub(crate) struct Offered {
    pub(crate) frame: OwnedFrame,
    pub(crate) registration: LeaseRegistration,
    pub(crate) session: CancellationToken,
}

#[derive(Debug)]
pub(crate) struct Downlink {
    handle: StreamHandle,
    config: crate::config::StreamConfig,
    authorization: HeaderValue,
    current: watch::Sender<Option<Arc<SessionSender>>>,
    running: Arc<Running>,
    offers: mpsc::Sender<Offered>,
    offer_budget: Budget,
    assignments: watch::Sender<Option<Arc<OwnedFrame>>>,
}

impl Downlink {
    pub(crate) fn new(config: &Config) -> Result<(StreamHandle, Self, mpsc::Receiver<Offered>)> {
        let bearer = zeroize::Zeroizing::new(format!("Bearer {}", config.token().expose()));
        let mut authorization = HeaderValue::from_str(&bearer).map_err(|source| {
            error::boundary(
                "the runner credential cannot be carried in a header",
                source,
            )
        })?;
        authorization.set_sensitive(true);
        let running = Arc::new(Running::default());
        let (current, session) = watch::channel(None);
        let (offers, offered) = mpsc::channel(af_wire::protocol::LEASES_MAX);
        let (assignments, policies) = watch::channel(None);
        let handle = StreamHandle::new(session, Arc::clone(&running), policies);
        Ok((
            handle.clone(),
            Self {
                handle,
                config: config.stream().clone(),
                authorization,
                current,
                running,
                offers,
                offer_budget: Budget::new(0, LEASES_MAX * MESSAGE_BYTES_MAX)?,
                assignments,
            },
            offered,
        ))
    }

    pub(crate) async fn run(self, halt: &Halt) -> Result<()> {
        let stop = CancellationToken::new();
        let renewing = self.handle.renewing.run(&self.handle, stop.clone());
        let sessions = self.sessions(halt);
        tokio::pin!(renewing, sessions);
        tokio::select! {
            result = &mut sessions => { stop.cancel(); renewing.await?; result }
            result = &mut renewing => { halt.stop(); sessions.await?; result }
        }
    }

    async fn sessions(&self, halt: &Halt) -> Result<()> {
        let (input, output) = Budget::duplex(af_stream::MemoryLimits::default());
        let mut resume = Resume::default();
        let mut preference = Preference::default();
        let mut retries = Recovery::new(crate::client::backoff());
        let mut outage = observation::Outage::new();
        loop {
            if halt.serving().is_cancelled() {
                return Ok(());
            }
            let attempt = af_crypto::entropy::Entropy::new().uuid7(af_core::clock::now())?;
            let span = tracing::info_span!(target: af_observability::semconv::RUNNER_SCOPE_NAME,
                "runner.connection", attempt_id = %attempt, connection_id = tracing::field::Empty,
                transport = tracing::field::Empty, recovery = tracing::field::Empty,
                outcome = tracing::field::Empty, disconnected_seconds = tracing::field::Empty);
            let stop = halt.serving().child_token();
            let deadline = self
                .handle
                .renewing
                .recovery_deadline(Instant::now() + af_core::timing::STREAM_RECOVERY_BUDGET);
            let opening = connect::Opening {
                config: &self.config,
                authorization: &self.authorization,
                running: &self.running,
                input: &input,
                output: &output,
                stop: &stop,
                deadline,
            };
            let (result, connected_for) = self
                .connection_attempt(&opening, &mut resume, &mut preference, &mut outage)
                .instrument(span.clone())
                .await;
            if halt.serving().is_cancelled() {
                span.record("outcome", "stopped");
                return Ok(());
            }
            self.retry(halt, &span, &mut outage, result)?;
            let pause = retries.after(connected_for)?;
            tokio::select! { () = halt.serving().cancelled() => return Ok(()), () = tokio::time::sleep(pause) => {} }
        }
    }

    fn retry(
        &self,
        halt: &Halt,
        span: &tracing::Span,
        outage: &mut observation::Outage,
        result: Result<Progress>,
    ) -> Result<()> {
        outage.disconnected(span);
        match result {
            Ok(Progress::Halt(code)) => {
                let failure = ErrorKind::StreamRefused {
                    code,
                    retryable: false,
                }
                .into();
                Err(self.refuse(halt, span, failure))
            }
            Err(error) if !error.is_retryable() => Err(self.refuse(halt, span, error)),
            Err(error) => {
                let error_code = error.code().as_str();
                span.record("outcome", "retrying");
                span.in_scope(|| tracing::warn!(event = "runner_stream_reconnecting", error_code));
                Ok(())
            }
            Ok(_) => {
                span.record("outcome", "drain");
                span.in_scope(|| {
                    tracing::info!(event = "runner_stream_reconnecting", reason = "drain");
                });
                Ok(())
            }
        }
    }

    fn refuse(&self, halt: &Halt, span: &tracing::Span, failure: crate::Error) -> crate::Error {
        span.record("outcome", "refused");
        self.running.cancel_all();
        if !halt.stops_on(&failure) {
            halt.stop();
        }
        failure
    }

    async fn connection_attempt(
        &self,
        opening: &connect::Opening<'_>,
        resume: &mut Resume,
        preference: &mut Preference,
        outage: &mut observation::Outage,
    ) -> (Result<Progress>, Option<std::time::Duration>) {
        let answer = match opening.open(resume, preference).await {
            Ok(connection) => {
                outage.connected(connection.connection.transport());
                let opened_at = Instant::now();
                let result = self
                    .serve(connection, opening.output.clone(), opening.stop.clone())
                    .await;
                (result, Some(opened_at.elapsed()))
            }
            Err(error) => (Err(error), None),
        };
        opening.stop.cancel();
        self.current.send_replace(None);
        answer
    }

    async fn serve(
        &self,
        connected: Connected,
        budget: Budget,
        stop: CancellationToken,
    ) -> Result<Progress> {
        self.running.new_session();
        let (queues, receivers) = Queues::new();
        let requests = pending::Requests::new();
        let sender = Arc::new(SessionSender {
            requests: Arc::clone(&requests),
            queues,
            budget,
            stop: stop.clone(),
        });
        let (frames_tx, mut frames) = mpsc::channel(16);
        let (control_tx, mut control_frames) = mpsc::channel(16);
        let mut workers = JoinSet::new();
        let connection = io::spawn(
            connected,
            receivers,
            control_tx,
            &frames_tx,
            &stop,
            &mut workers,
        );
        self.current.send_replace(Some(Arc::clone(&sender)));
        io::track(&mut workers, io::ping(self.handle.clone(), stop.clone()));
        let mut dispatch = Dispatch {
            requests: &requests,
            running: &self.running,
            offers: &self.offers,
            offer_budget: &self.offer_budget,
            assignments: &self.assignments,
            stop: &stop,
            early_cancel: HashSet::new(),
        };
        let result = self
            .events(
                &sender,
                &mut dispatch,
                &mut control_frames,
                &mut frames,
                &mut workers,
                &stop,
            )
            .await;
        self.finish(connection, workers, &requests, &stop, result)
            .await
    }

    async fn finish(
        &self,
        connection: af_stream::Connection,
        mut workers: JoinSet<Result<()>>,
        requests: &pending::Requests,
        stop: &CancellationToken,
        result: Result<Progress>,
    ) -> Result<Progress> {
        self.current.send_replace(None);
        stop.cancel();
        requests.close();
        while let Some(done) = workers.join_next().await {
            match done {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    let error_code = error.code().as_str();
                    tracing::debug!(event = "runner_stream_worker_ended", error_code);
                }
                Err(error) => {
                    let is_panic = error.is_panic();
                    let is_cancelled = error.is_cancelled();
                    let failure: crate::Error = error.into();
                    let error_code = failure.code().as_str();
                    tracing::warn!(
                        event = "runner_stream_worker_failed",
                        error_code,
                        is_panic,
                        is_cancelled
                    );
                }
            }
        }
        let closed = connection
            .close(Instant::now() + af_core::timing::STREAM_ANSWER_TIMEOUT)
            .await;
        if let Err(error) = closed {
            let reason = error.reason().as_str();
            let error_code = error.code().as_str();
            tracing::warn!(event = "runner_stream_close_failed", error_code, reason);
        }
        result
    }

    async fn events(
        &self,
        sender: &SessionSender,
        dispatch: &mut Dispatch<'_>,
        control: &mut mpsc::Receiver<(StreamKind, OwnedFrame)>,
        frames: &mut mpsc::Receiver<(StreamKind, OwnedFrame)>,
        workers: &mut JoinSet<Result<()>>,
        stop: &CancellationToken,
    ) -> Result<Progress> {
        // Control opens with zero credits. Policy must arrive before a worker can advertise capacity.
        sender
            .send(
                &self.running.ready()?,
                Instant::now() + af_core::timing::STREAM_ANSWER_TIMEOUT,
            )
            .await?;
        loop {
            tokio::select! {
                biased;
                () = stop.cancelled() => return Err(ErrorKind::Disconnected.into()),
                frame = control.recv() => {
                    let (kind, frame) = frame.ok_or(ErrorKind::Disconnected)?;
                    match dispatch.frame(kind, frame)? { Progress::Continue => {}, ended => return Ok(ended) }
                }
                done = workers.join_next() => {
                    let ended = done.ok_or(ErrorKind::Disconnected)??;
                    return dispatch.until_control_ends(control, ended).await;
                }
                () = self.running.changed.notified() => {
                    sender.send(&self.running.ready()?,
                        Instant::now() + af_core::timing::STREAM_ANSWER_TIMEOUT).await?;
                }
                frame = frames.recv() => {
                    let (kind, frame) = frame.ok_or(ErrorKind::Disconnected)?;
                    match dispatch.frame(kind, frame)? { Progress::Continue => {}, ended => return Ok(ended) }
                }
            }
        }
    }
}

#[cfg(all(test, feature = "test-util"))]
mod telemetry_tests;
#[cfg(all(test, feature = "test-util"))]
mod terminal_tests;
