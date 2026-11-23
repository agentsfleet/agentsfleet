//! A lost welcome retains the proposal until the database classifies the next opening.

use af_core::{id::Uuid7, timing::STREAM_STABLE_RESET_AFTER};
use af_stream::{Connection, Transport};
use backon::BackoffBuilder as _;
use std::time::Duration;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Default)]
pub(crate) enum Resume {
    #[default]
    Fresh,
    Confirmed(Uuid7),
    Confirming {
        previous: Option<Uuid7>,
        proposed: Uuid7,
    },
}

impl Resume {
    pub(crate) fn previous(&self) -> Option<&Uuid7> {
        match self {
            Self::Fresh => None,
            Self::Confirmed(owner) => Some(owner),
            Self::Confirming { previous, .. } => previous.as_ref(),
        }
    }

    pub(crate) fn unconfirmed(&self) -> Option<&Uuid7> {
        match self {
            Self::Confirming { proposed, .. } => Some(proposed),
            _ => None,
        }
    }
}

/// How long HTTP/3 is passed over after it fails while HTTP/2 works.
const HTTP3_SKIP: Duration = Duration::from_secs(30);

/// What one attempt taught the runner about the two carriers.
#[derive(Clone, Copy, Debug)]
enum Outcome {
    /// A carrier connected; HTTP/3 was tried and failed first when `http3_failure` names a reason.
    Carried {
        http3_failure: Option<af_stream::Reason>,
    },
    /// Neither carrier connected. It carries no reason: no reason may start the skip.
    Failed,
}

/// Recent UDP failure delays another attempt; a healthy carrier is never interrupted.
#[derive(Debug, Default)]
pub(crate) struct Preference {
    retry_http3_after: Option<Instant>,
}

impl Preference {
    pub(crate) async fn connect(
        &mut self,
        config: &crate::config::StreamConfig,
        stop: &CancellationToken,
        deadline: Instant,
    ) -> crate::Result<Connection> {
        if self.skips_http3(Instant::now()) {
            let deadline =
                deadline.min(Instant::now() + af_core::timing::STREAM_HTTP2_CONNECT_TIMEOUT);
            return Ok(Connection::connect(
                Transport::H2,
                &config.endpoint,
                &config.trust,
                stop,
                deadline,
            )
            .await?);
        }
        match Connection::preferred(&config.endpoint, &config.trust, stop, deadline).await {
            Ok(selected) => {
                let http3_failure = selected.http3_failure;
                self.record(Outcome::Carried { http3_failure }, Instant::now());
                Ok(selected.connection)
            }
            Err(error) => {
                self.record(Outcome::Failed, Instant::now());
                Err(error.into())
            }
        }
    }

    /// Whether an attempt at `now` goes straight to HTTP/2.
    fn skips_http3(&self, now: Instant) -> bool {
        self.retry_http3_after.is_some_and(|after| now < after)
    }

    fn record(&mut self, outcome: Outcome, now: Instant) {
        // Only HTTP/2 connecting after HTTP/3 failed points at UDP. Neither carrier connecting
        // shows an unreachable daemon, and arming the skip then would send the first connection
        // after a restart to HTTP/2, where a healthy session stays.
        let udp_suspect = match outcome {
            Outcome::Carried { http3_failure } => http3_failure.is_some(),
            Outcome::Failed => false,
        };
        if udp_suspect {
            self.retry_http3_after = Some(now + HTTP3_SKIP);
        }
    }
}

/// Failed openings and short-lived sessions share one retry history.
pub(crate) struct Recovery {
    builder: backon::ExponentialBuilder,
    pauses: backon::ExponentialBackoff,
}

impl Recovery {
    pub(crate) fn new(builder: backon::ExponentialBuilder) -> Self {
        let builder = builder.without_max_times();
        Self {
            builder,
            pauses: builder.build(),
        }
    }

    pub(crate) fn after(&mut self, connected_for: Option<Duration>) -> crate::Result<Duration> {
        if connected_for.is_some_and(|duration| duration >= STREAM_STABLE_RESET_AFTER) {
            self.pauses = self.builder.build();
        }
        // Backon's jitter is added after its maximum-delay clamp.
        self.pauses
            .next()
            .map(|pause| pause.min(Duration::from_secs(30)))
            .ok_or_else(|| crate::error::ErrorKind::StreamProtocol.into())
    }
}

#[cfg(test)]
#[path = "reconnect_tests.rs"]
mod tests;

#[cfg(all(test, feature = "test-util"))]
#[path = "reconnect_outage_tests.rs"]
mod outage_tests;
