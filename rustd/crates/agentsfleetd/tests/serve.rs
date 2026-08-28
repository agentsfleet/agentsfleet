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

mod support;

use afd_core::env::MapEnv;
use agentsfleetd::serve::{Acceptor, BootFailure, DEFAULT_PORT, serve_accepts};
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
        .chain(support::SESSION_PEPPER)
        .chain(support::IDENTITY),
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
