//! Boot's refusals, and the port rule, without opening anything.
//!
//! The happy path needs live datastores and lives in `integration_serve.rs`.
//! What is here is every way boot declines to proceed — which is the half that
//! matters for a daemon, because each one is a process that must not end up
//! serving traffic it cannot answer.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::env::MapEnv;
use agentsfleetd::serve::{
    Acceptor, BootFailure, DEFAULT_PORT, dual_stack_listener, serve_accepts,
};
use agentsfleetd::supervisor::Supervisor;

/// The API role's Postgres knob.
const DATABASE_KNOB: &str = "DATABASE_URL_API";

/// The API role's Redis knob.
const REDIS_KNOB: &str = "REDIS_URL_API";

/// The master-key knob.
const KEK_KNOB: &str = "ENCRYPTION_MASTER_KEY";

/// Sixty-four hex characters.
const GOOD_KEK: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// A Postgres URL that parses and points at nothing listening.
const DEAD_DATABASE: &str = "postgres://afd:afd@127.0.0.1:1/afd?sslmode=disable";

/// A Redis URL that parses and points at nothing listening.
const DEAD_REDIS: &str = "redis://127.0.0.1:1";

/// An environment whose knobs all parse but whose datastores are not there.
fn parses_but_dead() -> MapEnv {
    MapEnv::from_pairs(
        [
            (DATABASE_KNOB, DEAD_DATABASE),
            (REDIS_KNOB, DEAD_REDIS),
            (KEK_KNOB, GOOD_KEK),
        ]
        .into_iter()
        // Both are required at boot and resolved rather than dialled, so
        // supplying them well-formed keeps this fixture's failure the DATASTORE
        // one it is asserting about.
        .chain(crate::support::SESSION_PEPPER)
        .chain(crate::support::IDENTITY),
    )
}

/// An unusable environment refuses before anything is opened.
///
/// The ordering is the claim: `preflight` runs first, so a daemon with a
/// malformed key never reaches a socket. Asserted by the VARIANT — a
/// `Datastore` failure here would mean boot had already tried to connect.
#[tokio::test]
async fn test_boot_refuses_an_unusable_environment_before_connecting() {
    let mut supervisor = Supervisor::new();
    let failure = agentsfleetd::serve::boot(&MapEnv::default(), DEFAULT_PORT, &mut supervisor)
        .await
        .expect_err("an empty environment cannot boot");

    assert!(
        matches!(failure, BootFailure::Environment(_)),
        "boot must refuse on the environment, before any connection: {failure:?}"
    );
    assert_eq!(failure.phase(), "preflight");
    assert_eq!(failure.code(), afd_core::error_code::STARTUP_ENV_CHECK);
    assert!(
        supervisor.inventory().is_empty(),
        "nothing is supervised by a boot that refused"
    );

    let rendered = failure.to_string();
    for knob in [DATABASE_KNOB, REDIS_KNOB, KEK_KNOB] {
        assert!(
            rendered.contains(knob),
            "the refusal names every missing knob; {knob} is absent from: {rendered}"
        );
    }
}

/// A datastore that will not answer is a different refusal, and says so.
#[tokio::test]
async fn test_boot_refuses_when_a_datastore_will_not_answer() {
    let mut supervisor = Supervisor::new();
    let failure = agentsfleetd::serve::boot(&parses_but_dead(), DEFAULT_PORT, &mut supervisor)
        .await
        .expect_err("a datastore that is not there cannot be booted against");

    assert!(
        matches!(failure, BootFailure::Database(_)),
        "the URL parsed, so this is a database failure and not an environment one: {failure:?}"
    );
    assert_eq!(failure.phase(), "database");
    assert_eq!(failure.code(), afd_core::error_code::STARTUP_DB_CONNECT);
    assert!(
        supervisor.inventory().is_empty(),
        "the accept loop is spawned last; a boot that failed earlier supervises nothing"
    );
    assert!(
        failure.to_string().contains("cannot boot"),
        "the message leads with what happened"
    );

    // The point of the whole variant shape: the ORIGINAL error survives as a
    // source. An earlier revision stringified it here, which compiled and read
    // fine and left `fatal::render` with an empty chain to walk.
    let source = std::error::Error::source(&failure).expect("the sqlx failure is preserved");
    assert!(
        !source.to_string().is_empty(),
        "the underlying datastore error is carried, not summarised away"
    );

    let rendered = agentsfleetd::fatal::render(&failure, agentsfleetd::tty::Rendering::Plain);
    assert!(
        rendered.contains("caused by:"),
        "the fatal renderer can walk the chain it was built for: {rendered}"
    );
}

/// Every failure variant renders something an operator can act on.
#[test]
fn test_every_boot_failure_renders_a_reason() {
    let listen = BootFailure::Listen(std::io::Error::new(
        std::io::ErrorKind::AddrInUse,
        "address already in use",
    ));
    assert!(listen.to_string().contains("cannot listen"));
    assert_eq!(listen.phase(), "listen");
    assert_eq!(
        listen.code(),
        afd_core::error_code::INTERNAL_OPERATION_FAILED
    );
    assert!(
        std::error::Error::source(&listen)
            .expect("the io error is the source")
            .to_string()
            .contains("address already in use"),
        "the operating system's own words survive the conversion"
    );

    // Composition is by `From`, so `?` lifts at the call site and nothing has
    // to remember which variant a given foreign error belongs in.
    let lifted: BootFailure = std::io::Error::other("bind refused").into();
    assert!(
        matches!(lifted, BootFailure::Listen(_)),
        "an io error lifts to the listen variant on its own"
    );

    let (_kind, queue_source) = afd_redis::error::one_of_each_kind()
        .into_iter()
        .next()
        .expect("the Redis error fixture is exhaustive");
    let queue = BootFailure::from(queue_source);
    assert_eq!(queue.phase(), "queue");
    assert_eq!(queue.code(), afd_core::error_code::STARTUP_REDIS_CONNECT);
    assert!(
        std::error::Error::source(&queue).is_some(),
        "the queue failure preserves the original Redis error"
    );
}

/// An acceptor that fails a fixed number of times, then blocks forever.
///
/// The "blocks forever" half is load-bearing: after the failures the loop must
/// go back to waiting, and an acceptor that returned `Ok` would end the test on
/// the connection path instead of the one under examination.
#[derive(Debug)]
struct FailsThenParks {
    remaining: std::sync::atomic::AtomicUsize,
    observed: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl Acceptor for FailsThenParks {
    async fn accept(&self) -> std::io::Result<tokio::net::TcpStream> {
        use std::sync::atomic::Ordering;
        if self.remaining.fetch_sub(1, Ordering::SeqCst) > 0 {
            self.observed.fetch_add(1, Ordering::SeqCst);
            return Err(std::io::Error::other("out of file descriptors"));
        }
        std::future::pending().await
    }
}

/// A failed accept costs one client, not the daemon.
///
/// `accept()` fails for reasons no test can arrange — EMFILE, a peer that reset
/// between the SYN and the accept. The loop's answer is to log and keep
/// serving, and a loop that returned instead would leave a process alive,
/// holding its port, accepting nothing. That is the worst shape of outage:
/// every health check that only pings the process still passes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_a_failed_accept_does_not_stop_the_daemon() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let observed = Arc::new(AtomicUsize::new(0));
    let acceptor = FailsThenParks {
        remaining: AtomicUsize::new(3),
        observed: Arc::clone(&observed),
    };
    let token = tokio_util::sync::CancellationToken::new();

    let loop_token = token.clone();
    let serving =
        tokio::spawn(async move { serve_accepts(acceptor, axum::Router::new(), loop_token).await });

    // The loop must survive all three failures and still be waiting.
    while observed.load(Ordering::SeqCst) < 3 {
        tokio::task::yield_now().await;
    }
    assert!(
        !serving.is_finished(),
        "three failed accepts must not end serving"
    );

    token.cancel();
    tokio::time::timeout(std::time::Duration::from_secs(1), serving)
        .await
        .expect("the loop stops when cancelled, even after failures")
        .expect("the loop does not panic");
}

// ── The bind address ─────────────────────────────────────────────────────
//
// These are the guard the Zig daemon carried and the Rust port dropped. The
// deployment has no public Fly service: Cloudflare Tunnel reaches
// `agentsfleetd-<env>.internal:3000`, Fly resolves `.internal` to a 6PN
// address that is IPv6 only, and a listener that refuses IPv6 answers the
// edge with 502 while every local check stays green. That asymmetry is why
// the bug shipped twice, and why the assertion is about the SOCKET rather
// than about a configuration string.

/// The port that asks the kernel to choose one, so these can run anywhere.
const EPHEMERAL: u16 = 0;

#[tokio::test]
async fn test_the_listener_binds_ipv6_not_ipv4_only() {
    let listener = dual_stack_listener(EPHEMERAL).expect("an ephemeral port binds");
    let address = listener
        .local_addr()
        .expect("a bound listener has an address");

    assert!(
        address.is_ipv6(),
        "the listener must be IPv6: Fly 6PN resolves the tunnel's \
         `.internal` target to an IPv6 address only, and `0.0.0.0` refused it \
         — got {address}"
    );
    assert!(
        address.ip().is_unspecified(),
        "the listener must accept on every interface, not one — got {address}"
    );
}

#[tokio::test]
async fn test_an_ipv6_client_connects() {
    let listener = dual_stack_listener(EPHEMERAL).expect("an ephemeral port binds");
    let port = listener
        .local_addr()
        .expect("a bound listener has an address")
        .port();

    // The connection the Cloudflare Tunnel makes, and the one that was
    // refused: IPv6 to the loopback address.
    let client = tokio::net::TcpStream::connect((std::net::Ipv6Addr::LOCALHOST, port)).await;

    assert!(
        client.is_ok(),
        "an IPv6 client must connect — this is the 502 the tunnel saw: {:?}",
        client.err()
    );
}

#[tokio::test]
async fn test_an_ipv4_client_still_connects() {
    let listener = dual_stack_listener(EPHEMERAL).expect("an ephemeral port binds");
    let port = listener
        .local_addr()
        .expect("a bound listener has an address")
        .port();

    // The half that must NOT regress in exchange. `[checks.readiness]` in the
    // Fly configuration probes port 3000 and passes today against an
    // IPv4-only listener, so a v6-only socket would trade one outage for
    // another. This is what `set_only_v6(false)` buys.
    let client = tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).await;

    assert!(
        client.is_ok(),
        "an IPv4 client must still connect — the Fly readiness check is one: {:?}",
        client.err()
    );
}

#[tokio::test]
async fn test_two_listeners_cannot_share_a_port() {
    let held = dual_stack_listener(EPHEMERAL).expect("an ephemeral port binds");
    let port = held
        .local_addr()
        .expect("a bound listener has an address")
        .port();

    // `set_reuse_address` must not have become `SO_REUSEPORT`: two daemons
    // silently load-balancing one port is a far worse failure than a refused
    // second boot.
    let second = dual_stack_listener(port);

    assert!(
        second.is_err(),
        "a second bind on a held port must be refused"
    );
}
