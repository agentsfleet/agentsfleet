//! One connection per role, multiplexed, with a deadline on every command.
//!
//! # Why there is no pool
//!
//! The Zig daemon keeps a pool of request/reply sockets behind a mutex, because
//! a blocking client can only have one command in flight per socket. An async
//! client does not have that problem: `ConnectionManager` writes concurrent
//! commands down one socket and routes each reply back to the caller that is
//! waiting for it. So the ~3.0k lines of pooling, RESP framing and reconnect
//! logic under `src/agentsfleetd/queue/` become one field here — and the
//! reconnect that pool hand-rolled is the manager's own behaviour.
//!
//! The one thing a multiplexed connection must not do is run a blocking
//! command, because it would stall every other caller sharing the socket. That
//! is why pub/sub gets its own connection in [`crate::hub`] and why the stream
//! reads in [`crate::streams`] never pass `BLOCK`.
//!
//! # Deadlines are here, not in the caller
//!
//! Invariant 4: every I/O deadline is a `tokio::time::timeout` at the call
//! site. [`Redis::command`] is that call site, so no caller can start an
//! unbounded Redis operation by forgetting to wrap one.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use redis::{Cmd, FromRedisValue, Value};

use crate::config::{RedisConfig, RedisRole};
use crate::error::{self, Error, ErrorKind, Result};

/// Correlates one connection boundary's started and terminal records.
static NEXT_CONNECT_ATTEMPT: AtomicU64 = AtomicU64::new(0);

/// The liveness probe, and the only command this module issues by name.
const CMD_PING: &str = "PING";

/// The invariant every constant below serves.
///
/// `connect_inner` runs the driver's whole reconnection ladder inside
/// [`RedisConfig::connect_timeout`]. For the outer deadline to be the LAST
/// thing that fires rather than the first, the ladder's worst case has to fit:
///
/// ```text
/// (CONNECT_RETRIES + 1) * CONNECT_ATTEMPT_TIMEOUT   <- the attempts
///   + jittered sum of the backoff delays            <- the sleeps
///   < RedisConfig::connect_timeout                  <- the outer budget
/// ```
///
/// `redis` 1.6.0 satisfies none of that by default. It ships six retries over a
/// jittered doubling backoff from 100 ms with `max_delay: None`, so `backon`'s
/// own 60 s ceiling applies and nothing caps the ladder
/// (`connection_manager.rs`, `DEFAULT_NUMBER_OF_CONNECTION_RETRIES`). The six
/// base delays are 100+200+400+800+1600+3200 = 6300 ms of SLEEP, and because
/// `backon`'s jitter is additive — each delay becomes `d..2d` — the real range
/// is 6.3 s to 12.6 s, before a single connection attempt is counted.
///
/// Against a 5 s budget that ladder cannot be exhausted. The failure this
/// produces is not "connecting is slow": most first failures recover on the
/// next attempt after a 100–200 ms sleep. It is that a run of failures leaves
/// the driver mid-ladder when the outer deadline cancels it, and the caller is
/// handed `ConnectTimeout` — an error naming Redis, carrying no trace of
/// whatever actually caused the retries. The initiating error is destroyed.
///
/// So bounding the ladder IS the diagnostic fix. Once the worst case fits, the
/// driver always returns its own error first, and that error keeps its source
/// chain through [`ErrorKind::Unreachable`].
const CONNECT_RETRIES: usize = 2;

/// The floor and ceiling of the driver's backoff between those retries.
///
/// Bounded rather than derived from the budget: a fraction-of-budget ladder
/// would grow with the budget and re-create the problem on a generous one.
/// Worst case here is jitter-doubled 50+100 = 300 ms.
const CONNECT_RETRY_MIN_DELAY: Duration = Duration::from_millis(50);
const CONNECT_RETRY_MAX_DELAY: Duration = Duration::from_millis(100);

/// The deadline on ONE connection attempt, pinned rather than inherited.
///
/// `redis` defaults this to 1 s and the response deadline to 500 ms
/// (`client.rs`, `DEFAULT_CONNECTION_TIMEOUT`). Those are reasonable, and they
/// are still written down here: the invariant above multiplies this value by
/// the attempt count, so a future release changing its own default would move
/// our worst case without touching this crate. Pinning makes the arithmetic
/// ours.
const CONNECT_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(1);

/// The deadline on the reply within one attempt, pinned for the same reason.
const CONNECT_RESPONSE_TIMEOUT: Duration = Duration::from_millis(500);

/// A connection to one role's Redis.
///
/// Cheap to clone: cloning shares the same multiplexed connection rather than
/// opening another, which is what keeps "one connection per process per role"
/// true no matter how many components hold one.
#[derive(Debug, Clone)]
pub struct Redis {
    role: RedisRole,
    manager: ConnectionManager,
    request_timeout: Duration,
}

impl Redis {
    /// Opens the connection for `config`'s role, proving Redis answers.
    ///
    /// # Errors
    /// Returns an unavailable error when Redis cannot be reached, and a config
    /// error when a certificate authority file was named but not readable.
    pub async fn connect(config: &RedisConfig) -> Result<Self> {
        let started = Instant::now();
        let attempt_id = NEXT_CONNECT_ATTEMPT.fetch_add(1, Ordering::Relaxed);
        let role = config.role().tag();
        let timeout_ms = config.connect_timeout().as_millis();
        let tls = config.is_tls();
        tracing::info!(
            attempt_id,
            role,
            timeout_ms,
            tls,
            event = "redis_connect_started"
        );

        let result =
            match tokio::time::timeout(config.connect_timeout(), Self::connect_inner(config)).await
            {
                Ok(result) => result,
                Err(_elapsed) => Err(error::connect_timed_out(role, timeout_ms)),
            };
        let duration_ms = started.elapsed().as_millis();
        match result {
            Ok(redis) => {
                let request_timeout_ms = config.request_timeout().as_millis();
                tracing::info!(
                    attempt_id,
                    role,
                    duration_ms,
                    request_timeout_ms,
                    tls,
                    event = "redis_connect_completed"
                );
                Ok(redis)
            }
            Err(failure) => {
                let error_code = failure.code().as_str();
                tracing::warn!(
                    attempt_id,
                    role,
                    duration_ms,
                    error_code,
                    reason = %failure,
                    event = "redis_connect_failed"
                );
                Err(failure)
            }
        }
    }

    async fn connect_inner(config: &RedisConfig) -> Result<Self> {
        let client = build_client(config)?;
        let manager = ConnectionManager::new_with_config(client, connect_retry_policy())
            .await
            .map_err(|source| {
                Error::new(ErrorKind::Unreachable {
                    role: config.role().tag(),
                    source: Box::new(source),
                })
            })?;

        let redis = Self {
            role: config.role(),
            manager,
            request_timeout: config.request_timeout(),
        };
        // A connection that has not answered is a connection that might not
        // exist: `ConnectionManager::new` establishes one, but the boot
        // preflight's claim is that Redis SERVES, and only a reply proves that.
        redis.ping().await?;
        Ok(redis)
    }

    /// A handle over a Redis that has NOT been proven to answer.
    ///
    /// The mirror of [`afd_db::Db::unreachable`], and behind `test-util` for
    /// the same reason: the ping in [`Redis::connect`] is the promise that a
    /// boot which returned has a Redis that SERVES, and a constructor skipping
    /// it would let a binary start against a queue that is not there.
    ///
    /// What it exists for is the other half of that promise — proving what the
    /// request path does when the queue is gone. `ConnectionManager` is built
    /// lazily against the configured address, so every command through it
    /// fails at the socket rather than at a fake.
    ///
    /// # Why a test needs its OWN unreachable handle
    ///
    /// The integration lane's Redis is SHARED by every test binary running in
    /// parallel, so the obvious injections — pausing the container, killing the
    /// server, dropping the port — fail unrelated suites at the same instant.
    /// A handle only one test holds is the only way to prove the drop path
    /// without taking the queue away from everybody else.
    ///
    /// # Errors
    /// Returns a config error when a certificate authority file was named but
    /// not readable, and an unreachable error when the manager cannot even be
    /// constructed — both happen before any socket.
    #[cfg(feature = "test-util")]
    pub fn unreachable(config: &RedisConfig) -> Result<Self> {
        let client = build_client(config)?;
        // `new_lazy_with_config` builds the manager WITHOUT opening a socket,
        // which is the whole point: `connect` above opens one and pings it, and
        // this seam exists to skip exactly that.
        let manager = ConnectionManager::new_lazy_with_config(client, connect_retry_policy())
            .map_err(|source| {
                Error::new(ErrorKind::Unreachable {
                    role: config.role().tag(),
                    source: Box::new(source),
                })
            })?;
        Ok(Self {
            role: config.role(),
            manager,
            request_timeout: config.request_timeout(),
        })
    }

    /// The role this connection serves.
    #[must_use]
    pub const fn role(&self) -> RedisRole {
        self.role
    }

    /// How long any one command may take.
    #[must_use]
    pub const fn request_timeout(&self) -> Duration {
        self.request_timeout
    }

    /// Runs one command under this connection's deadline.
    ///
    /// `name` is what a failure reports; it is the command verb rather than the
    /// whole argument vector, because arguments carry payloads and payloads do
    /// not belong in error text.
    ///
    /// # Errors
    /// Returns a timeout error when the deadline passes, a group-missing error
    /// for `NOGROUP`, an unavailable error when the connection dropped, and a
    /// command error otherwise.
    pub async fn command<T: FromRedisValue>(
        &self,
        name: &'static str,
        context: &str,
        cmd: &Cmd,
    ) -> Result<T> {
        let mut manager = self.manager.clone();
        let value =
            tokio::time::timeout(self.request_timeout, cmd.query_async::<Value>(&mut manager))
                .await
                .map_err(|_elapsed| error::timed_out(name, self.request_timeout.as_millis()))?
                .map_err(|source| error::classify(name, context, source))?;

        // A parse failure is not a Redis failure: the server answered, and the
        // reply is a shape this client did not expect. Reporting it as a
        // command error would send an operator looking at Redis.
        T::from_redis_value(value).map_err(|_parse| error::unexpected_reply(name))
    }

    /// Runs a prepared script invocation, under the same deadline a command
    /// gets.
    ///
    /// Its own method rather than a `Cmd`, because a script invocation is not
    /// one: `redis` loads the body by digest and falls back to sending it when
    /// the server has never seen it, and that retry is the crate's to perform.
    /// What this adds is what [`Self::command`] adds — the timeout, the error
    /// classification, and the rule that a reply shape we did not expect is
    /// reported as such rather than as a Redis fault.
    ///
    /// # Errors
    /// As [`Self::command`].
    pub async fn script<T: FromRedisValue>(
        &self,
        name: &'static str,
        context: &str,
        invocation: &redis::ScriptInvocation<'_>,
    ) -> Result<T> {
        let mut manager = self.manager.clone();
        let value = tokio::time::timeout(
            self.request_timeout,
            invocation.invoke_async::<Value>(&mut manager),
        )
        .await
        .map_err(|_elapsed| error::timed_out(name, self.request_timeout.as_millis()))?
        .map_err(|source| error::classify(name, context, source))?;

        T::from_redis_value(value).map_err(|_parse| error::unexpected_reply(name))
    }

    /// `PING`, which is how boot asks whether Redis is actually serving.
    ///
    /// # Errors
    /// Returns an unavailable error when Redis does not answer.
    pub async fn ping(&self) -> Result<()> {
        let reply: String = self.command(CMD_PING, "", &redis::cmd(CMD_PING)).await?;
        if reply.eq_ignore_ascii_case("PONG") {
            Ok(())
        } else {
            Err(error::unexpected_reply(CMD_PING))
        }
    }
}

/// Builds the client, wiring a custom certificate authority when one is named.
///
/// The local compose Redis serves a self-signed certificate, so the CA arrives
/// by path rather than from a trust store. `REDIS_TLS_CA_CERT_FILE` is the same
/// knob `redis_config.zig` reads, and the same file the Zig lane extracts from
/// the container.
/// The driver's reconnection ladder, bounded to fit inside a connect budget.
///
/// See [`CONNECT_RETRIES`] for why the default does not fit. This is also what
/// [`Redis::unreachable`] builds on, so the two paths answer with one policy.
pub(crate) fn connect_retry_policy() -> ConnectionManagerConfig {
    ConnectionManagerConfig::new()
        .set_number_of_retries(CONNECT_RETRIES)
        .set_min_delay(CONNECT_RETRY_MIN_DELAY)
        .set_max_delay(CONNECT_RETRY_MAX_DELAY)
        .set_connection_timeout(Some(CONNECT_ATTEMPT_TIMEOUT))
        .set_response_timeout(Some(CONNECT_RESPONSE_TIMEOUT))
}

pub(crate) fn build_client(config: &RedisConfig) -> Result<redis::Client> {
    let Some(path) = config.ca_cert_file() else {
        return redis::Client::open(config.url()).map_err(|source| {
            Error::new(ErrorKind::Unreachable {
                role: config.role().tag(),
                source: Box::new(source),
            })
        });
    };

    let root_cert = std::fs::read(path).map_err(|source| {
        Error::new(ErrorKind::CaCertUnreadable {
            path: path.display().to_string(),
            source,
        })
    })?;

    redis::Client::build_with_tls(
        config.url(),
        redis::TlsCertificates {
            client_tls: None,
            root_cert: Some(root_cert),
        },
    )
    .map_err(|source| {
        Error::new(ErrorKind::Unreachable {
            role: config.role().tag(),
            source: Box::new(source),
        })
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        CONNECT_ATTEMPT_TIMEOUT, CONNECT_RESPONSE_TIMEOUT, CONNECT_RETRIES,
        CONNECT_RETRY_MAX_DELAY, CONNECT_RETRY_MIN_DELAY, connect_retry_policy,
    };
    use crate::config::{RedisConfig, RedisRole};

    /// The longest the driver's ladder can take before it gives an answer.
    ///
    /// Attempts and sleeps both count. `backon` doubles the delay per retry,
    /// caps it at `max_delay`, and adds a random `(0, delay)` on top — jitter is
    /// ADDITIVE, so one retry's ceiling is twice its capped delay, not the
    /// delay. Budgeting against `max_delay` alone would under-count by half.
    fn worst_case_ladder() -> Duration {
        // Summed rather than multiplied: the retry count is a `usize` because
        // that is what the driver's setter takes, and converting it to scale a
        // `Duration` buys a fallible conversion that cannot fail, for nothing.
        let mut attempts = Duration::ZERO;
        for _ in 0..=CONNECT_RETRIES {
            attempts += CONNECT_ATTEMPT_TIMEOUT;
        }

        let mut sleeps = Duration::ZERO;
        let mut delay = CONNECT_RETRY_MIN_DELAY;
        for _ in 0..CONNECT_RETRIES {
            sleeps += delay.min(CONNECT_RETRY_MAX_DELAY) * 2;
            delay *= 2;
        }

        attempts + sleeps
    }

    fn default_budget() -> Duration {
        RedisConfig::from_url(RedisRole::Default, "redis://127.0.0.1:6379".to_owned())
            .connect_timeout()
    }

    /// The regression this pins, and the reason it is a correctness test rather
    /// than a performance one.
    ///
    /// `redis`'s defaults put a 6.3–12.6 s ladder inside a 5 s budget, which
    /// cannot be exhausted. The damage is not the waiting: it is that the outer
    /// deadline cancels the driver mid-ladder, so the error that started the
    /// retries never returns and the caller is told `ConnectTimeout` — pointing
    /// every future investigation at Redis rather than at the real fault.
    ///
    /// While the ladder fits, the driver's own error always arrives first and
    /// keeps its source chain. So this assertion is what makes the crate's
    /// errors truthful, and any change to the five constants has to preserve it.
    #[test]
    fn test_the_connect_ladder_answers_before_the_budget_expires() {
        let worst = worst_case_ladder();
        let budget = default_budget();

        assert!(
            worst < budget,
            "attempts plus jittered backoff must finish inside the connect \
             budget, or the driver is cancelled mid-retry and its error is lost: \
             worst case {worst:?} against a {budget:?} budget",
        );
    }

    /// The policy is applied, and applied where this module says it is.
    ///
    /// Separate from the budget proof because the two fail for different
    /// reasons: this catches a policy that stopped being applied at all — a
    /// bare `ConnectionManagerConfig::new()` creeping back into `connect_inner`
    /// and silently restoring the six-retry default — where that one catches a
    /// policy still applied but grown past the budget.
    #[test]
    fn test_the_driver_never_inherits_its_own_unbounded_defaults() {
        let policy = connect_retry_policy();

        assert_eq!(
            policy.number_of_retries(),
            CONNECT_RETRIES,
            "the six-retry default must not be inherited",
        );
        assert_eq!(
            policy.max_delay(),
            Some(CONNECT_RETRY_MAX_DELAY),
            "`max_delay: None` is what let backon's 60 s ceiling apply",
        );
        assert_eq!(policy.min_delay(), CONNECT_RETRY_MIN_DELAY);
        assert_eq!(policy.connection_timeout(), Some(CONNECT_ATTEMPT_TIMEOUT));
        assert_eq!(policy.response_timeout(), Some(CONNECT_RESPONSE_TIMEOUT));
    }

    /// The arithmetic that made the default wrong, kept as a worked example.
    ///
    /// Without it a reader has to trust the prose in [`CONNECT_RETRIES`]. This
    /// recomputes `redis`'s shipped defaults — six retries, 100 ms minimum,
    /// doubling, uncapped — and shows the result does not fit, so the claim the
    /// constants are chosen against stays checkable rather than remembered.
    #[test]
    fn test_the_shipped_defaults_are_the_ones_that_do_not_fit() {
        const SHIPPED_RETRIES: u32 = 6;
        const SHIPPED_MIN_DELAY: Duration = Duration::from_millis(100);

        let mut sleeps = Duration::ZERO;
        let mut delay = SHIPPED_MIN_DELAY;
        for _ in 0..SHIPPED_RETRIES {
            // Uncapped: `max_delay` is None, so backon's 60 s ceiling is the
            // only limit and no delay here approaches it.
            sleeps += delay;
            delay *= 2;
        }

        assert_eq!(sleeps, Duration::from_millis(6300));
        assert!(
            sleeps > default_budget(),
            "the shipped ladder sleeps {sleeps:?} against a {:?} budget, before \
             a single connection attempt is counted",
            default_budget(),
        );
    }
}
