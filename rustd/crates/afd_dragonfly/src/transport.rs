//! The one transport: a redis-rs cluster client over RESP3.
//!
//! # Why there is no standalone path
//!
//! Dragonfly and Dragonfly speak the same protocol, so the only axis the code can
//! see is standalone versus cluster — and the daemon's datastore is a cluster.
//! A selector between two transports would be a knob nobody flips, carried
//! across every surface as a second arm. This module is the whole choice: one
//! builder, one connect, one lazy connect for the fault suites.
//!
//! # Why every connection is built here
//!
//! [`crate::Dragonfly`], [`crate::Dedicated`] and the hub's pump each hold their
//! own `ClusterConnection`. The driver keeps exactly one socket per node and
//! applies one reply deadline to every command on a connection, so a parked
//! read on a shared handle would stall the owning node's only socket and
//! impose the park-sized deadline on every other caller. Three connections,
//! one policy — and the policy lives in one place.
//!
//! # The connect ladder fits the budget
//!
//! `connect` runs the driver's whole retry ladder inside
//! [`DragonflyConfig::connect_timeout`]. For the outer deadline to be the LAST
//! thing that fires rather than the first, the ladder's worst case has to fit:
//!
//! ```text
//! CONNECT_ATTEMPTS * CONNECT_ATTEMPT_TIMEOUT       <- the attempts
//!   + jittered sum of the backoff delays            <- the sleeps
//!   < DragonflyConfig::connect_timeout                  <- the outer budget
//! ```
//!
//! While that holds, the driver's own error always arrives first and keeps its
//! source chain. When it does not, the outer deadline cancels the driver
//! mid-ladder and the caller is handed a `ConnectTimeout` naming the
//! datastore, with the initiating error destroyed.
//!
//! The ladder costs something even when it fits. A refusal the server will
//! repeat — a certificate the configured authority does not trust — is retried
//! anyway, and an attempt that trips `CONNECT_ATTEMPT_TIMEOUT` late in the
//! ladder replaces the refusal that opened it, because the driver keeps only
//! the LAST initial-connection error. That is what `diagnose` re-asks for, and
//! why a failed TLS dial can end in [`ErrorKind::CertificateRejected`] rather
//! than [`ErrorKind::Unreachable`].

use std::num::NonZeroUsize;
use std::time::Duration;

use redis::cluster::{ClusterClient, ClusterClientBuilder, ClusterConfig};
use redis::cluster_async::ClusterConnection;
use redis::{ProtocolVersion, PushInfo, TlsCertificates};
use tokio::sync::mpsc;

use crate::config::DragonflyConfig;
use crate::error::{self, Error, ErrorKind, Result};

/// Retries the driver makes on a redirect or a dropped node before it gives
/// the command up. A migration in flight answers `MOVED`/`ASK`, and a handful
/// of follows is the difference between a reroute and a failure.
const REDIRECT_RETRIES: u32 = 8;

/// The floor and ceiling of the driver's backoff between those retries.
///
/// Bounded rather than derived from the budget: a fraction-of-budget ladder
/// would grow with the budget and re-create the problem on a generous one.
const RETRY_MIN_WAIT: Duration = Duration::from_millis(50);
const RETRY_MAX_WAIT: Duration = Duration::from_millis(100);

/// The deadline on ONE node dial, pinned rather than inherited.
///
/// The driver defaults this to a second; it is still written down here
/// because the ladder arithmetic in the module note multiplies it by the
/// attempt count, and a release changing its own default would move our
/// worst case without touching this crate.
const CONNECT_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(1);

/// How many dials the driver may attempt before reporting the seed unreachable.
///
/// A `match` rather than `unwrap`, because a constant that panics at compile
/// time needs no runtime proof and the manifest forbids `unwrap` everywhere.
const CONNECT_ATTEMPTS: NonZeroUsize = match NonZeroUsize::new(3) {
    Some(attempts) => attempts,
    None => unreachable!(),
};

/// The attempt count the diagnosis dial uses.
///
/// One, and that is the entire point: the ladder above is what destroys the
/// answer that dial exists to recover.
const ONE_ATTEMPT: NonZeroUsize = match NonZeroUsize::new(1) {
    Some(attempts) => attempts,
    None => unreachable!(),
};

/// A connection and the receiver its server-initiated pushes arrive on.
pub(crate) struct Pushed {
    pub(crate) connection: ClusterConnection,
    pub(crate) pushes: mpsc::UnboundedReceiver<PushInfo>,
}

/// The builder every connection starts from: the seed, RESP3, the ladder, and
/// the authority when the seed is `rediss://`.
///
/// A certificate authority is meaningless without TLS, and the driver does not
/// merely ignore one: certificates on a `redis://` seed fail the whole build.
/// So the authority is consulted only when the scheme says TLS, which is the
/// lane's own shape — ordinary suites take the plaintext cluster while the
/// trust suite takes `rediss://`, both from a process that has the CA
/// configured.
///
/// `response_timeout` is the reply deadline for every command on the
/// connections this builder opens. It is set HERE and not only on the
/// per-connection config, because the driver hands the builder's value to
/// each node socket and applies the per-connection one as the overall bound —
/// whichever is smaller fires first, and a builder default of half a second
/// would fail every parked read at 500 ms.
pub(crate) fn builder(
    config: &DragonflyConfig,
    response_timeout: Duration,
) -> Result<ClusterClientBuilder> {
    let mut builder = ClusterClientBuilder::new([config.url().to_owned()])
        .use_protocol(ProtocolVersion::RESP3)
        .retries(REDIRECT_RETRIES)
        .min_retry_wait(RETRY_MIN_WAIT.as_millis().try_into().unwrap_or(u64::MAX))
        .max_retry_wait(RETRY_MAX_WAIT.as_millis().try_into().unwrap_or(u64::MAX))
        .max_connection_attempts(CONNECT_ATTEMPTS)
        .connection_timeout(CONNECT_ATTEMPT_TIMEOUT)
        .response_timeout(response_timeout);
    if let Some(path) = config.ca_cert_file().filter(|_| config.is_tls()) {
        let root_cert = std::fs::read(path).map_err(|source| {
            Error::new(ErrorKind::CaCertUnreadable {
                path: path.display().to_string(),
                source,
            })
        })?;
        builder = builder.certs(TlsCertificates {
            client_tls: None,
            root_cert: Some(root_cert),
        });
    }
    Ok(builder)
}

/// Builds the client, which parses the seed and reads the authority but opens
/// no socket. A seed that is not a URL is refused here, by role.
pub(crate) fn client(
    config: &DragonflyConfig,
    response_timeout: Duration,
) -> Result<ClusterClient> {
    // No socket is opened here, so nothing that fails here can be an outage.
    // Reporting it as one sent an operator to look at the network for a seed
    // their own configuration had malformed -- and this function's own
    // documentation already promised a config error.
    builder(config, response_timeout)?
        .build()
        .map_err(|source| error::config_rejected(config.role().tag(), source))
}

/// The per-connection settings: the reply deadline a caller declares, and the
/// push channel when one is wanted.
fn connection_config(
    response_timeout: Duration,
    pushes: Option<mpsc::UnboundedSender<PushInfo>>,
) -> ClusterConfig {
    let config = ClusterConfig::new()
        .set_connection_timeout(CONNECT_ATTEMPT_TIMEOUT)
        .set_response_timeout(response_timeout);
    match pushes {
        Some(sender) => config.set_push_sender(sender),
        None => config,
    }
}

/// Opens a connection, proving the cluster answers, inside the role's connect
/// budget. `response_timeout` is the reply deadline every command on this
/// connection will carry.
///
/// # Errors
/// Returns a config error when the seed is not a URL or a named authority is
/// unreadable, a connect-timeout error when the budget lapses, and an
/// unavailable error naming the role otherwise.
pub(crate) async fn connect(
    config: &DragonflyConfig,
    response_timeout: Duration,
) -> Result<ClusterConnection> {
    let client = client(config, response_timeout)?;
    let dial = client.get_async_connection_with_config(connection_config(response_timeout, None));
    match tokio::time::timeout(config.connect_timeout(), dial).await {
        Ok(Ok(connection)) => Ok(connection),
        Ok(Err(source)) => Err(dial_failure(config, response_timeout, source).await),
        Err(_elapsed) => Err(error::connect_timed_out(
            config.role().tag(),
            config.connect_timeout().as_millis(),
        )),
    }
}

/// As [`connect`], with server pushes routed to the returned receiver.
pub(crate) async fn connect_with_pushes(
    config: &DragonflyConfig,
    response_timeout: Duration,
) -> Result<Pushed> {
    let client = client(config, response_timeout)?;
    let (sender, pushes) = mpsc::unbounded_channel();
    let dial =
        client.get_async_connection_with_config(connection_config(response_timeout, Some(sender)));
    match tokio::time::timeout(config.connect_timeout(), dial).await {
        Ok(Ok(connection)) => Ok(Pushed { connection, pushes }),
        Ok(Err(source)) => Err(dial_failure(config, response_timeout, source).await),
        Err(_elapsed) => Err(error::connect_timed_out(
            config.role().tag(),
            config.connect_timeout().as_millis(),
        )),
    }
}

/// A connection that has NOT been proven to answer: the driver dials in the
/// background and the first command reports the outcome. This is what the
/// fault suites use to prove the request path against a datastore that is
/// not there, without taking the lane's datastore away from everyone else.
///
/// Gated with its one caller, [`crate::Dragonfly::unreachable`]: the workspace
/// lints with `--all-features`, so a function reachable only under
/// `test-util` reads as dead to anyone building this crate the way a
/// dependent does.
#[cfg(feature = "test-util")]
pub(crate) fn pending(
    config: &DragonflyConfig,
    response_timeout: Duration,
) -> Result<ClusterConnection> {
    Ok(client(config, response_timeout)?
        .get_pending_async_connection_with_config(connection_config(response_timeout, None)))
}

/// Whether a failed dial says the server's certificate was not trusted.
///
/// Read out of the rendering rather than matched on a typed cause, because
/// there is no typed cause left to match. When every initial connection
/// fails, the cluster driver keeps ONE of the errors and folds it into an
/// `ErrorKind::Io` as `err.to_string()` — the lossy conversion
/// `docs/RUST_ERROR_STANDARD.md` rule 3 forbids, performed upstream where
/// this crate cannot decline it. The text is what survives, so the text is
/// what we read.
fn names_a_certificate(source: &redis::RedisError) -> bool {
    let rendered = format!("{source:?}");
    rendered.contains("UnknownIssuer") || rendered.contains("certificate")
}

/// Re-dials once, without retries, to recover the cause the ladder discarded.
///
/// The driver stores only the LAST initial-connection error. A trust failure
/// is refused in about twenty milliseconds and can never succeed on a retry,
/// so the retries that follow it are pure cost — and when one of them trips
/// the one-second attempt timeout instead, THAT is the error kept, and a
/// certificate the server will never present acceptably reads as an
/// unreachable port. Measured on the lane: the refusal lands at 0.66s and the
/// displaced timeout at 1.4s and up, from the same configuration, at roughly
/// one run in five.
///
/// Costs a single handshake, only on a path that has already failed.
/// `None` means the diagnosis did not produce an answer worth preferring —
/// the dial unexpectedly succeeded, or it timed out in its own right — and
/// the caller keeps the error it already had.
async fn diagnose(
    config: &DragonflyConfig,
    response_timeout: Duration,
) -> Option<redis::RedisError> {
    let client = builder(config, response_timeout)
        .ok()?
        .max_connection_attempts(ONE_ATTEMPT)
        .build()
        .ok()?;
    let dial = client.get_async_connection_with_config(connection_config(response_timeout, None));
    match tokio::time::timeout(CONNECT_ATTEMPT_TIMEOUT, dial).await {
        Ok(Err(source)) => Some(source),
        Ok(Ok(_)) | Err(_) => None,
    }
}

/// Names the cause of a failed dial, asking a second time when it has to.
///
/// The happy path never reaches here, and a plaintext endpoint pays nothing:
/// only a TLS dial that failed without naming a certificate is worth a second
/// question, because only there can the answer have been thrown away.
async fn dial_failure(
    config: &DragonflyConfig,
    response_timeout: Duration,
    source: redis::RedisError,
) -> Error {
    if names_a_certificate(&source) {
        return error::certificate_rejected(config.role().tag(), source);
    }
    if config.is_tls()
        && let Some(recovered) = diagnose(config, response_timeout).await
        && names_a_certificate(&recovered)
    {
        return error::certificate_rejected(config.role().tag(), recovered);
    }
    error::unreachable(config.role().tag(), source)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{CONNECT_ATTEMPT_TIMEOUT, CONNECT_ATTEMPTS, RETRY_MAX_WAIT, builder};
    use crate::config::{DragonflyConfig, DragonflyRole};

    fn default_budget() -> Duration {
        DragonflyConfig::from_url(DragonflyRole::Default, "redis://127.0.0.1:6379".to_owned())
            .connect_timeout()
    }

    /// The regression this pins, and the reason it is a correctness test rather
    /// than a performance one: while the dial ladder fits the budget, the
    /// driver's own error always arrives first and keeps its source chain. Any
    /// change to the constants has to preserve it. Jitter is additive, so a
    /// retry's ceiling is twice its capped wait.
    #[test]
    fn test_the_connect_ladder_answers_before_the_budget_expires() {
        let attempts_made = u32::try_from(CONNECT_ATTEMPTS.get()).unwrap_or(u32::MAX);
        let attempts = CONNECT_ATTEMPT_TIMEOUT * attempts_made;
        let sleeps = RETRY_MAX_WAIT * 2 * attempts_made.saturating_sub(1);
        let worst = attempts + sleeps;
        let budget = default_budget();
        assert!(
            worst < budget,
            "dials plus jittered backoff must finish inside the connect budget, \
             or the driver is cancelled mid-retry and its error is lost: \
             worst case {worst:?} against a {budget:?} budget",
        );
    }

    /// A configured authority does not make a plaintext seed a TLS one: the
    /// scheme selects the transport, the authority only says whom to trust
    /// once TLS is chosen. The path names a file that does not exist, and that
    /// is the assertion — the plaintext branch never reads it.
    #[test]
    fn test_a_configured_authority_does_not_force_tls_on_a_plaintext_url() {
        let config =
            DragonflyConfig::from_url(DragonflyRole::Api, "redis://127.0.0.1:6379".to_owned())
                .with_ca_cert_file(Some("/nonexistent/authority.pem".into()));
        assert!(
            builder(&config, Duration::from_secs(1)).is_ok(),
            "a redis:// seed opens plaintext whatever authority is configured"
        );
    }

    /// And the scheme that does mean TLS still reaches the authority, failing
    /// on the file rather than quietly opening plaintext to a TLS port.
    #[test]
    fn test_a_tls_url_reads_the_authority_it_was_given() {
        let config =
            DragonflyConfig::from_url(DragonflyRole::Api, "rediss://127.0.0.1:6380".to_owned())
                .with_ca_cert_file(Some("/nonexistent/authority.pem".into()));
        let refusal = builder(&config, Duration::from_secs(1))
            .err()
            .map(|error| error.to_string());
        assert!(
            refusal
                .as_deref()
                .is_some_and(|message| message.contains("/nonexistent/authority.pem")),
            "a rediss:// seed must consult the authority and name it when unreadable: {refusal:?}"
        );
    }
}
