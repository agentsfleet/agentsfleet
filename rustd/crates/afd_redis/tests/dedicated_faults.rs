//! What a dedicated connection does when the socket misbehaves.
//!
//! Two proofs, both without a Redis. The first is the deadline: a parked read
//! is given the park it declared, not the driver's own half-second default —
//! the difference between a `BLOCK 5000` that returns an entry and one that
//! fails at 500 ms while the server keeps the socket parked. The second is the
//! redial: a server that hangs up mid-command costs one failed command, not
//! every command for the rest of the process.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::{Duration, Instant};

use afd_redis::Dedicated;
use afd_redis::config::{RedisConfig, RedisRole};
use redis::Value;

use crate::fake_redis::{FakeRedis, Reply, install_subscriber};

/// The stream the fake is asked about. Any name would do: the fake matches on
/// the command, and the deadline is a property of the connection.
const STREAM: &str = "probe:stream";
const CMD_XREADGROUP: &str = "XREADGROUP";

/// The park the connection is opened for. Longer than the driver's default
/// reply deadline, which is exactly the case the first test exists for.
const PARK: Duration = Duration::from_millis(700);

/// The ordinary allowance for an answer to travel, added to the park.
const REQUEST_DEADLINE: Duration = Duration::from_millis(100);

/// How long a redial may take to be visible before the test gives up.
const REDIAL_BUDGET: Duration = Duration::from_secs(5);

/// Room for the runtime to schedule the wake, and nothing more.
///
/// Deliberately small. The bound this test exists to prove is the connection's
/// reply deadline; a generous ceiling would pass on a read that gave up
/// seconds late, which is what an earlier version did by borrowing
/// [`REDIAL_BUDGET`] — a term belonging to the redial test next door — and
/// asserting against a ceiling six times the deadline under test.
const SCHEDULING_SLACK: Duration = Duration::from_millis(150);

/// The first command the driver sends on every new socket, before anything
/// the caller asked for: its own `CLIENT SETINFO`. A fake told to swallow it
/// leaves the dial stalled with TCP already open.
const CMD_CLIENT: &str = "CLIENT";

/// How long the dial in the last test is given.
///
/// Deliberately below the driver's own retry ladder, so the outer timeout is
/// the one that fires and the branch under test is the one reached.
const CONNECT_DEADLINE: Duration = Duration::from_millis(120);

/// A blocking read, as the outbound reader spells one.
fn blocking_read(park: Duration) -> redis::Cmd {
    let mut cmd = redis::cmd(CMD_XREADGROUP);
    cmd.arg("GROUP")
        .arg("probe_group")
        .arg("probe_consumer")
        .arg("COUNT")
        .arg(1)
        .arg("BLOCK")
        .arg(park.as_millis())
        .arg("STREAMS")
        .arg(STREAM)
        .arg(">");
    cmd
}

/// A read is given the park it declared, and a bound beyond it.
///
/// A server that never answers stands in for one still honouring a `BLOCK`:
/// the driver must wait out the whole park plus the request allowance before
/// it gives up, and it must give up — a peer that vanishes without closing the
/// socket is an outage, not a longer park.
#[tokio::test(flavor = "multi_thread")]
async fn test_a_dedicated_read_waits_out_the_park_it_declared() {
    install_subscriber();
    let server = FakeRedis::spawn(&[(CMD_XREADGROUP, Reply::Silent)]).await;
    let config = RedisConfig::from_url(RedisRole::Default, server.url())
        .with_request_timeout(REQUEST_DEADLINE);
    let mut owned = Dedicated::connect(&config, PARK)
        .await
        .expect("a listening socket must be connectable");

    let started = Instant::now();
    let failure = owned
        .command::<Value>(CMD_XREADGROUP, STREAM, &blocking_read(PARK))
        .await
        .expect_err("a server that never answers must not be waited on forever");
    let waited = started.elapsed();

    assert!(
        waited >= PARK,
        "the read gave up after {waited:?}, inside the {PARK:?} park it declared — \
         the driver's own default deadline is being applied"
    );
    // The deadline this connection actually sets, plus scheduling room.
    assert!(
        waited < PARK + REQUEST_DEADLINE + SCHEDULING_SLACK,
        "the read waited {waited:?}, past the {PARK:?} park plus the \
         {REQUEST_DEADLINE:?} reply allowance — that is not a bound"
    );
    assert!(
        failure.is_unavailable(),
        "a peer that never answers is unavailable, got: {failure}"
    );
    assert!(
        server
            .seen()
            .iter()
            .any(|command| command == CMD_XREADGROUP),
        "the read never reached the server: {:?}",
        server.seen()
    );
}

/// A hangup mid-command costs that command, and the next one goes down a new
/// socket.
///
/// The failure this guards against is the quiet one: a dropped socket that
/// nothing redials fails every read for the rest of the process, and the
/// worker holding it reports the same outage forever while the queue fills.
#[tokio::test(flavor = "multi_thread")]
async fn test_a_dedicated_socket_that_is_hung_up_on_is_redialled() {
    install_subscriber();
    let server = FakeRedis::spawn(&[(CMD_XREADGROUP, Reply::Hangup)]).await;
    let config = RedisConfig::from_url(RedisRole::Default, server.url())
        .with_request_timeout(REQUEST_DEADLINE);
    let mut owned = Dedicated::connect(&config, PARK)
        .await
        .expect("a listening socket must be connectable");

    let dropped = owned
        .command::<Value>(CMD_XREADGROUP, STREAM, &blocking_read(PARK))
        .await
        .expect_err("a server that hangs up mid-command fails that command");
    assert!(
        dropped.is_unavailable(),
        "a hangup is a dropped connection, got: {dropped}"
    );

    // From here the server answers, and the only question is whether the
    // client comes back to ask.
    server.set_reply(CMD_XREADGROUP, Reply::Raw("$-1\r\n"));
    let deadline = Instant::now() + REDIAL_BUDGET;
    let answered = loop {
        match owned
            .command::<Value>(CMD_XREADGROUP, STREAM, &blocking_read(PARK))
            .await
        {
            Ok(value) => break value,
            Err(again) if Instant::now() < deadline => {
                tracing::debug!(reason = %again, event = "probe_redial_pending");
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(never) => panic!("the socket was never redialled: {never}"),
        }
    };

    assert_eq!(
        answered,
        Value::Nil,
        "the answer after the redial must be the server's, not a stale error"
    );
    let reads = server
        .seen()
        .iter()
        .filter(|command| *command == CMD_XREADGROUP)
        .count();
    assert!(
        reads >= 2,
        "the server must have seen the read before and after the hangup, saw {reads}"
    );
}

/// A dial that never completes is given up on, and says how long it waited.
///
/// The connection opens with a reply allowance of park plus request timeout —
/// ten seconds on production defaults — and it would be easy to assume the DIAL
/// inherits it. It does not: the outer `connect_timeout` bounds the whole
/// setup, so a peer whose handshake never completes costs that and no more.
/// This is the branch that proves it, and the error it produces names the
/// number rather than surfacing as a driver error nobody can act on.
///
/// The peer is the fake on loopback, told to swallow `CLIENT`: TCP completes,
/// the driver's handshake never does, and nothing between here and the socket
/// can turn that into a different fault. An earlier version dialled a
/// black-holed address instead, which proves the same thing only where the
/// network drops the packet, and made the verdict depend on the route to it.
/// Nothing here leaves loopback.
#[tokio::test(flavor = "multi_thread")]
async fn test_a_dial_that_is_never_answered_gives_up_at_the_connect_timeout() {
    install_subscriber();
    let server = FakeRedis::spawn(&[(CMD_CLIENT, Reply::Silent)]).await;
    let config = RedisConfig::from_url(RedisRole::Default, server.url())
        .with_connect_timeout(CONNECT_DEADLINE);

    let started = Instant::now();
    let failure = Dedicated::connect(&config, PARK)
        .await
        .expect_err("a dial nothing answers must not be waited on forever");
    let waited = started.elapsed();

    // The floor proves the timeout is what ended the dial, the ceiling that
    // the reply allowance did not govern it. Neither says the fake was
    // reached — the driver retries a refused port until the same timeout —
    // which is what the last assertion is for.
    assert!(
        waited >= CONNECT_DEADLINE,
        "the dial gave up after {waited:?}, inside the {CONNECT_DEADLINE:?} connect timeout — \
         it was refused rather than timed out, and the timeout branch was never reached"
    );
    assert!(
        waited < CONNECT_DEADLINE + SCHEDULING_SLACK,
        "the dial waited {waited:?} against a {CONNECT_DEADLINE:?} connect timeout — \
         the reply allowance is governing the dial rather than the connect timeout"
    );
    let reported = failure.to_string();
    assert!(
        reported.contains(&CONNECT_DEADLINE.as_millis().to_string()),
        "the refusal must name what it waited; got: {reported}"
    );
    assert!(
        server.seen().iter().any(|command| command == CMD_CLIENT),
        "the dial never reached the server, so nothing was stalled: {:?}",
        server.seen()
    );
}
