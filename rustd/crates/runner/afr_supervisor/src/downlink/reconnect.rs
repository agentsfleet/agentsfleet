//! A lost welcome retains the proposal until the database classifies the next opening.

use af_core::id::Uuid7;
use af_stream::{Connection, Transport};
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
        if self
            .retry_http3_after
            .is_some_and(|after| Instant::now() < after)
        {
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
                if selected.http3_failure.is_some() {
                    self.delay();
                }
                Ok(selected.connection)
            }
            Err(error) => {
                if matches!(
                    error.reason(),
                    af_stream::Reason::Network | af_stream::Reason::Timeout
                ) {
                    self.delay();
                }
                Err(error.into())
            }
        }
    }

    fn delay(&mut self) {
        self.retry_http3_after = Some(Instant::now() + std::time::Duration::from_secs(30));
    }
}
