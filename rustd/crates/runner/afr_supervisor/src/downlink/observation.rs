//! Outage time survives failed openings and ends only when all native lanes attach.

use af_stream::Transport;
use tokio::time::Instant;
use tracing::Span;

pub(super) struct Outage {
    since: Option<Instant>,
    retried: bool,
}

impl Outage {
    pub(super) fn new() -> Self {
        Self {
            since: Some(Instant::now()),
            retried: false,
        }
    }

    pub(super) fn connected(&mut self, carrier: Transport) {
        let disconnected_seconds = self
            .since
            .take()
            .map_or(0.0, |start| start.elapsed().as_secs_f64());
        let transport = carrier.as_str();
        let recovery = if self.retried { "restored" } else { "initial" };
        let span = Span::current();
        span.record("transport", transport);
        span.record("recovery", recovery);
        span.record("disconnected_seconds", disconnected_seconds);
        let event = if self.retried {
            "runner_stream_recovered"
        } else {
            "runner_stream_connected"
        };
        tracing::info!(event, transport, recovery, disconnected_seconds);
    }

    pub(super) fn disconnected(&mut self, span: &Span) {
        if let Some(since) = self.since {
            span.record("disconnected_seconds", since.elapsed().as_secs_f64());
        } else {
            self.since = Some(Instant::now());
        }
        self.retried = true;
    }
}
