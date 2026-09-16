//! The subscription hub's own connection deadline.
//!
//! `connect_refusals` covers a socket nobody answers, which fails at the
//! kernel. This is the other shape, and the one an operator actually meets
//! when a datastore is wedged rather than gone: the socket OPENS, and then the
//! server never completes the handshake the cluster driver insists on. The
//! hub's start must give up on its declared budget instead of waiting on a
//! server that will never answer — a process hanging at boot tells whoever is
//! watching nothing at all.
//!
//! Its own binary-local module rather than a case in `hub_socket_faults`:
//! that file is within a handful of lines of the file cap.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::{Duration, Instant};

use afd_dragonfly::SubscriptionHub;
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};

use crate::fake_redis::{FakeRedis, Reply};

/// The connection budget the hub is given. Short, because the whole claim is
/// that it is HONOURED — a server that never answers would otherwise be
/// waited on for the driver's own default.
const CONNECT_BUDGET: Duration = Duration::from_millis(250);

/// How long the test waits for a budget it expects to expire. Far enough above
/// the budget that a slow runner cannot fail this, close enough that a hub
/// ignoring its deadline does.
const PATIENCE: Duration = Duration::from_secs(10);

/// The most the connect may take and still be honouring its budget.
///
/// A multiple, not the budget itself: the wait is a real timer plus whatever a
/// loaded runner adds to waking from it. It is far below the driver's own
/// default, which is the regression this bounds — a connect that ignored
/// `CONNECT_BUDGET` would land seconds out, not milliseconds.
const BUDGET_CEILING: Duration = Duration::from_secs(3);

/// The question every cluster client asks before it will send anything else.
const CMD_CLUSTER: &str = "CLUSTER";

/// A server that accepts the socket and then never answers the topology
/// question fails the hub's start on its declared budget.
///
/// The socket is open, so nothing is refused at the kernel and no error
/// arrives on its own: the only thing that ends this is the deadline. A hub
/// without one boots into a wait nobody can see.
#[tokio::test(flavor = "multi_thread")]
async fn test_a_handshake_that_never_completes_expires_the_connect_budget() {
    let server = FakeRedis::spawn(&[(CMD_CLUSTER, Reply::Silent)]).await;
    let config = DragonflyConfig::from_url(DragonflyRole::Default, server.url())
        .with_connect_timeout(CONNECT_BUDGET);

    let started = Instant::now();
    let error = tokio::time::timeout(PATIENCE, SubscriptionHub::start(config))
        .await
        .expect("the connect budget, not this timeout, must be what ends the wait")
        .expect_err("a handshake that never completes must not produce a hub");
    let waited = started.elapsed();

    // The BUDGET is the claim, so the elapsed time is what gets asserted.
    // Returning some role-bearing error eventually would satisfy a test that
    // only checked the message, including one that waited out the driver's own
    // default instead of the configured deadline.
    assert!(
        waited >= CONNECT_BUDGET,
        "the connect gave up after {waited:?}, inside the {CONNECT_BUDGET:?} it declared"
    );
    assert!(
        waited < BUDGET_CEILING,
        "the connect took {waited:?} against a {CONNECT_BUDGET:?} budget — the \
         configured deadline is being ignored for the driver's own"
    );
    assert!(
        error.to_string().contains(DragonflyRole::Default.tag()),
        "the failure must name the role, since a deployment runs two: {error}"
    );
}
