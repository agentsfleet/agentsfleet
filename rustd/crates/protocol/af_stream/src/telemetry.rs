//! One observation survives cancellation and records its original span on drop.

use tokio::time::Instant;

use af_observability::{
    metrics::label::fleet::{
        StreamFallback, StreamOutcome, StreamPeer, StreamStage, StreamTlsOutcome, StreamTransport,
    },
    producers::fleet::runner::stream,
};
use tracing::Span;

use crate::{Reason, Result, Stage, Transport};

enum Kind {
    Connect(Transport),
    Operation(Stage),
}

pub(crate) struct Observation {
    kind: Kind,
    started: Instant,
    span: Span,
    outcome: StreamOutcome,
    error_code: Option<&'static str>,
    io_kind: Option<std::io::ErrorKind>,
}

impl Observation {
    pub(crate) fn connect(transport: Transport) -> Self {
        Self::begin(Kind::Connect(transport))
    }

    pub(crate) fn operation(stage: Stage) -> Self {
        Self::begin(Kind::Operation(stage))
    }

    fn begin(kind: Kind) -> Self {
        let (operation, transport) = match kind {
            Kind::Connect(transport) => ("connection_attempt", Some(transport.as_str())),
            Kind::Operation(stage) => (stage.as_str(), None),
        };
        let span = tracing::debug_span!(
            "runner_stream_operation",
            operation,
            transport,
            outcome = tracing::field::Empty,
            elapsed_seconds = tracing::field::Empty
        );
        span.in_scope(|| tracing::debug!(event = "runner_stream_operation_started"));
        Self {
            kind,
            started: Instant::now(),
            span,
            outcome: StreamOutcome::Abandoned,
            error_code: None,
            io_kind: None,
        }
    }

    pub(crate) fn span(&self) -> Span {
        self.span.clone()
    }

    pub(crate) fn finish<T>(mut self, result: Result<T>) -> Result<T> {
        match &result {
            Ok(_) => self.outcome = StreamOutcome::Success,
            Err(error) => {
                self.outcome = error.reason().into();
                self.error_code = Some(error.code().as_str());
                self.io_kind = error.io_kind();
            }
        }
        result
    }
}

impl Drop for Observation {
    fn drop(&mut self) {
        let elapsed = self.started.elapsed();
        match self.kind {
            Kind::Connect(transport) => stream::connected(transport.into(), self.outcome, elapsed),
            Kind::Operation(stage) => stream::operation(stage.into(), self.outcome, elapsed),
        }
        let outcome = self.outcome.as_str();
        let elapsed_seconds = elapsed.as_secs_f64();
        self.span.record("outcome", outcome);
        self.span.record("elapsed_seconds", elapsed_seconds);
        let error_code = self.error_code;
        let io_kind = self.io_kind.map(|kind| format!("{kind:?}"));
        let _entered = self.span.enter();
        match self.outcome {
            StreamOutcome::Success => tracing::debug!(
                event = "runner_stream_operation_completed",
                outcome,
                elapsed_seconds
            ),
            StreamOutcome::Cancelled | StreamOutcome::Abandoned => tracing::debug!(
                event = "runner_stream_operation_failed",
                outcome,
                elapsed_seconds,
                error_code
            ),
            _ => tracing::warn!(
                event = "runner_stream_operation_failed",
                outcome,
                elapsed_seconds,
                error_code,
                io_kind
            ),
        }
    }
}

pub(crate) fn security(transport: Transport, peer: StreamPeer, group: crate::tls::KeyExchange) {
    let accepted = group == crate::tls::KeyExchange::X25519MlKem768;
    let outcome = if accepted {
        StreamTlsOutcome::Accepted
    } else {
        StreamTlsOutcome::Failed
    };
    stream::tls(transport.into(), peer, group.into(), outcome);
    let transport = transport.as_str();
    let role = peer.as_str();
    let key_exchange = group.as_str();
    let post_quantum_key_exchange = accepted;
    if accepted {
        tracing::info!(
            event = "runner_stream_tls_negotiated",
            transport,
            role,
            key_exchange,
            post_quantum_key_exchange
        );
    } else {
        let error_code = if group == crate::tls::KeyExchange::Other {
            af_core::error_code::INTERNAL_OPERATION_FAILED.as_str()
        } else {
            af_core::error_code::RUN_PROTOCOL_UNSUPPORTED.as_str()
        };
        tracing::warn!(
            event = "runner_stream_tls_failed",
            error_code,
            transport,
            role,
            key_exchange,
            post_quantum_key_exchange
        );
    }
}

pub(crate) fn fallback(reason: Reason) -> bool {
    let reason = match reason {
        Reason::Network => StreamFallback::Network,
        Reason::Timeout => StreamFallback::Timeout,
        Reason::Closed => StreamFallback::Closed,
        Reason::Unsupported => StreamFallback::Unsupported,
        Reason::Cancelled | Reason::Protocol | Reason::Worker | Reason::Tls => return false,
    };
    stream::fallback(reason);
    let reason = reason.as_str();
    tracing::info!(
        event = "runner_stream_fallback_attempt",
        transport = "http2",
        reason
    );
    true
}

impl From<Transport> for StreamTransport {
    fn from(value: Transport) -> Self {
        match value {
            Transport::H2 => Self::Http2,
            Transport::H3 => Self::Http3,
        }
    }
}

impl From<Stage> for StreamStage {
    fn from(value: Stage) -> Self {
        match value {
            Stage::Memory => Self::Memory,
            Stage::Read => Self::Read,
            Stage::Write => Self::Write,
            Stage::Connect => Self::Connect,
            Stage::Handshake => Self::Handshake,
            Stage::Open => Self::Open,
            Stage::Close => Self::Close,
        }
    }
}

impl From<Reason> for StreamOutcome {
    fn from(value: Reason) -> Self {
        match value {
            Reason::Network => Self::Network,
            Reason::Timeout => Self::Timeout,
            Reason::Cancelled => Self::Cancelled,
            Reason::Protocol => Self::Protocol,
            Reason::Closed => Self::Closed,
            Reason::Worker => Self::Worker,
            Reason::Tls => Self::Tls,
            Reason::Unsupported => Self::Unsupported,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_carrier_failures_permit_the_http2_fallback() {
        for (reason, permitted) in [
            (Reason::Network, true),
            (Reason::Timeout, true),
            (Reason::Closed, true),
            (Reason::Unsupported, true),
            (Reason::Cancelled, false),
            (Reason::Protocol, false),
            (Reason::Worker, false),
            (Reason::Tls, false),
        ] {
            assert_eq!(fallback(reason), permitted, "{reason:?}");
        }
    }

    #[test]
    fn every_reason_keeps_its_outcome_label() {
        for (reason, label) in [
            (Reason::Network, "network"),
            (Reason::Timeout, "timeout"),
            (Reason::Cancelled, "cancelled"),
            (Reason::Protocol, "protocol"),
            (Reason::Closed, "closed"),
            (Reason::Worker, "worker"),
            (Reason::Tls, "tls"),
            (Reason::Unsupported, "unsupported"),
        ] {
            assert_eq!(StreamOutcome::from(reason).as_str(), label);
        }
    }
}
