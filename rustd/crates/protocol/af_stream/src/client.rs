//! The owner of a single native connection and all of its upload tasks.

use std::fmt;

use http::{Request, Response};
use tokio::time::Instant;
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};
use tracing::Instrument as _;

use crate::{Endpoint, Incoming, Outgoing, Result, Stage, error::within, tls::Trust};

/// Finite native transport labels shared by metrics and traces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    /// HTTP/3 over QUIC, preferred on each new connection.
    H3,
    /// HTTP/2 over verified TLS and TCP.
    H2,
}

impl Transport {
    /// Stable spelling for bounded labels.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::H3 => "http3",
            Self::H2 => "http2",
        }
    }
}

pub(crate) enum Native {
    H2(crate::h2::Client),
    H3(crate::h3::Client),
}

/// Transport choice and the finite reason for falling back, when applicable.
#[derive(Debug)]
pub struct Selection {
    /// Owner of the selected native connection.
    pub connection: Connection,
    /// HTTP/3 failure that permitted HTTP/2, for the session's telemetry.
    pub http3_failure: Option<crate::Reason>,
}

/// Dropping the owner cancels its drivers and aborts its supervised task.
pub struct Connection {
    native: Native,
    stop: CancellationToken,
    task: AbortOnDropHandle<Result<()>>,
}

impl fmt::Debug for Connection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Connection")
            .field("transport", &self.transport())
            .finish_non_exhaustive()
    }
}

impl Connection {
    /// Prefer HTTP/3 on a new connection; identity and protocol refusals are terminal.
    ///
    /// # Errors
    /// Returns a terminal refusal or the final connection failure before the deadline.
    pub async fn preferred(
        endpoint: &Endpoint,
        trust: &Trust,
        parent: &CancellationToken,
        deadline: Instant,
    ) -> Result<Selection> {
        let budget = std::time::Duration::from_secs(2);
        let attempt = deadline.min(Instant::now() + budget);
        match Self::connect(Transport::H3, endpoint, trust, parent, attempt).await {
            Ok(connection) => Ok(Selection {
                connection,
                http3_failure: None,
            }),
            Err(error) => {
                let reason = error.reason();
                if !crate::telemetry::fallback(reason) {
                    return Err(error);
                }
                let attempt = deadline.min(Instant::now() + budget);
                let connection =
                    Self::connect(Transport::H2, endpoint, trust, parent, attempt).await?;
                Ok(Selection {
                    connection,
                    http3_failure: Some(reason),
                })
            }
        }
    }

    /// Connect and verify one transport within the caller's absolute deadline.
    ///
    /// # Errors
    /// Returns peer verification, network, deadline or cancellation failures with their causes.
    pub async fn connect(
        transport: Transport,
        endpoint: &Endpoint,
        trust: &Trust,
        parent: &CancellationToken,
        deadline: Instant,
    ) -> Result<Self> {
        let observation = crate::telemetry::Observation::connect(transport);
        let result = async {
            let stop = parent.child_token();
            let guard = stop.clone().drop_guard();
            let (native, task) = match transport {
                Transport::H2 => {
                    crate::h2::connect(endpoint, trust, stop.clone(), deadline).await?
                }
                Transport::H3 => {
                    crate::h3::connect(endpoint, trust, stop.clone(), deadline).await?
                }
            };
            let _stop = guard.disarm();
            Ok(Self { native, stop, task })
        }
        .instrument(observation.span())
        .await;
        observation.finish(result)
    }

    /// Headers resolve while the request body stays open; no body is collected.
    ///
    /// # Errors
    /// Returns a native refusal, cancellation or expired opening deadline and cancels the connection.
    pub async fn open(
        &mut self,
        mut request: Request<Outgoing>,
        deadline: Instant,
    ) -> Result<Response<Incoming>> {
        if let Some(value) = request.headers_mut().get_mut(http::header::AUTHORIZATION) {
            value.set_sensitive(true);
        }
        let guard = self.stop.clone().drop_guard();
        let response = within(&self.stop, deadline, Stage::Open, async {
            match &mut self.native {
                Native::H2(client) => client.open(request, self.stop.clone()).await,
                Native::H3(client) => client.open(request).await,
            }
        })
        .await?;
        let _stop = guard.disarm();
        Ok(response)
    }

    /// The transport stays fixed for the lifetime of this connection.
    #[must_use]
    pub const fn transport(&self) -> Transport {
        match self.native {
            Native::H2(_) => Transport::H2,
            Native::H3(_) => Transport::H3,
        }
    }

    /// Current native QUIC path counters; HTTP/2 has no QUIC path.
    #[must_use]
    pub fn network_stats(&self) -> Option<crate::NetworkStats> {
        match &self.native {
            Native::H2(_) => None,
            Native::H3(client) => Some(*client.stats.borrow()),
        }
    }

    /// Stop and observe driver completion; an expired close aborts the owner task.
    ///
    /// # Errors
    /// Returns the driver's failure or an expired close deadline.
    pub async fn close(mut self, deadline: Instant) -> Result<()> {
        self.stop.cancel();
        // The cancelled connection token cannot also govern its cleanup wait.
        // Dropping this owner aborts the supervised task if cleanup times out.
        within(&CancellationToken::new(), deadline, Stage::Close, async {
            (&mut self.task).await?
        })
        .await
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}
