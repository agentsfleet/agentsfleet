//! A QUIC endpoint never sends a datagram the path cannot carry, whatever the payload size.
//!
//! The HTTP/3 carrier runs through a relay that drops what a real path drops, and in one case
//! loses datagrams on top, at payload sizes from one packet to two mebibytes. Loopback hides all
//! of that, because it carries 64 KiB datagrams.
//!
//! Both endpoints enforce the ceiling in `src/mtu.rs`: the daemon caps what it sends, and the
//! runner tells the daemon the largest datagram it accepts. Either half alone bounds the daemon,
//! so the cases that run this repository's pair pass with one half removed. The cases that give
//! one endpoint no ceiling are the ones that fail when the other half goes.

mod assertions;
mod harness;
mod path_limit;

use crate::support::{Fixture, Result};
use af_stream::test_util::MAX_DATAGRAM_BYTES;
use assertions::{
    assert_datagram_budget, assert_every_payload_arrived, assert_nothing_dropped,
    assert_within_ceiling,
};
use harness::{
    Builds, CASE_DEADLINE, IPV6_MIN_MTU, LOSS_EVERY, LOSSY_DEADLINE, MEASURED_PATH_MTU,
    STALL_DEADLINE, Verdict, all_sizes, bulk_sizes, connect, matrix, open_all_lanes,
    path_payload_limit, payload, stream_window,
};
use path_limit::{Path, PathLimit};
use tokio::time::Instant;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_unconstrained_path_carries_every_payload_within_the_ceiling() -> Result {
    let outcomes = matrix(
        &all_sizes()?,
        Path::unconstrained(),
        CASE_DEADLINE,
        None,
        Builds::CURRENT,
    )
    .await?;
    assert_every_payload_arrived(&outcomes, "an unconstrained path");
    assert_within_ceiling(&outcomes, "an unconstrained path");
    assert_datagram_budget(&outcomes, "an unconstrained path");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_path_at_the_measured_dev_mtu_carries_every_payload_intact() -> Result {
    let path = Path::limited(path_payload_limit(MEASURED_PATH_MTU));
    let outcomes = matrix(&all_sizes()?, path, CASE_DEADLINE, None, Builds::CURRENT).await?;
    assert_every_payload_arrived(&outcomes, "a 1370-byte path");
    assert_nothing_dropped(&outcomes, "a 1370-byte path");
    assert_within_ceiling(&outcomes, "a 1370-byte path");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_path_at_the_ipv6_minimum_mtu_carries_every_payload_intact() -> Result {
    let path = Path::limited(path_payload_limit(IPV6_MIN_MTU));
    let outcomes = matrix(&all_sizes()?, path, CASE_DEADLINE, None, Builds::CURRENT).await?;
    assert_every_payload_arrived(&outcomes, "a 1280-byte path");
    assert_nothing_dropped(&outcomes, "a 1280-byte path");
    assert_within_ceiling(&outcomes, "a 1280-byte path");
    Ok(())
}

/// The route changes after the connection is up: no handshake-time discovery can have seen it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_path_that_shrinks_after_the_handshake_still_carries_every_payload() -> Result {
    let shrunk = path_payload_limit(IPV6_MIN_MTU);
    let outcomes = matrix(
        &bulk_sizes()?,
        Path::unconstrained(),
        CASE_DEADLINE,
        Some(shrunk),
        Builds::CURRENT,
    )
    .await?;
    assert_every_payload_arrived(&outcomes, "a path that shrank to 1280 bytes");
    assert_nothing_dropped(&outcomes, "a path that shrank to 1280 bytes");
    Ok(())
}

/// Retransmissions and probes are datagrams too: under loss they must respect the ceiling as well.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn loss_on_a_limited_path_never_loses_a_byte_and_never_exceeds_the_ceiling() -> Result {
    let path = Path::lossy(path_payload_limit(MEASURED_PATH_MTU), LOSS_EVERY);
    let outcomes = matrix(&bulk_sizes()?, path, LOSSY_DEADLINE, None, Builds::CURRENT).await?;
    assert_every_payload_arrived(&outcomes, "a lossy 1370-byte path");
    assert_nothing_dropped(&outcomes, "a lossy 1370-byte path");
    assert_within_ceiling(&outcomes, "a lossy 1370-byte path");
    assert!(
        outcomes.iter().any(|outcome| outcome.traffic.lost > 0),
        "the lossy path lost nothing, so retransmission never ran"
    );
    Ok(())
}

/// Production runs four lanes on one connection; a limit that starves one must not hide behind
/// the others.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn four_lanes_share_a_limited_path_without_starving_each_other() -> Result {
    let fixture = Fixture::new(vec!["localhost".to_owned()], false)?;
    let host = fixture.serve().await?;
    let limit = path_payload_limit(MEASURED_PATH_MTU);
    let path = PathLimit::start(&host, Path::limited(limit)).await?;
    let deadline = Instant::now() + CASE_DEADLINE;
    let mut connection = connect(&fixture.trust, &path.endpoint, deadline).await?;
    let [mut control, mut work, mut reports, mut activity] =
        open_all_lanes(&mut connection, &path.endpoint, deadline).await?;
    let sent = [
        payload(1_024),
        payload(stream_window()? + 1),
        payload(250_000),
        payload(256 * 1024),
    ];
    let (a, b, c, d) = tokio::join!(
        control.echo(&sent[0], deadline),
        work.echo(&sent[1], deadline),
        reports.echo(&sent[2], deadline),
        activity.echo(&sent[3], deadline),
    );
    let failures: Vec<String> = [("control", a), ("work", b), ("reports", c), ("activity", d)]
        .into_iter()
        .zip(&sent)
        .filter_map(|((lane, answer), expected)| match answer {
            Ok(bytes) if bytes == *expected => None,
            Ok(_) => Some(format!("{lane} came back altered")),
            Err(failure) => Some(format!("{lane}: {failure}")),
        })
        .collect();
    assert!(failures.is_empty(), "lanes failed: {failures:?}");
    let traffic = path.traffic();
    assert_eq!(traffic.dropped_to_client + traffic.dropped_to_server, 0);
    assert!(
        traffic.largest_to_client <= MAX_DATAGRAM_BYTES,
        "server sent {}",
        traffic.largest_to_client
    );
    Ok(())
}

/// The runner's advertised limit holds back a daemon that has no ceiling of its own.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_runner_keeps_a_daemon_without_the_ceiling_inside_a_limited_path() -> Result {
    let path = Path::limited(path_payload_limit(MEASURED_PATH_MTU));
    let outcomes = matrix(
        &bulk_sizes()?,
        path,
        CASE_DEADLINE,
        None,
        Builds::DAEMON_WITHOUT_CEILING,
    )
    .await?;
    let name = "a 1370-byte path with a daemon that has no ceiling";
    assert_every_payload_arrived(&outcomes, name);
    assert_nothing_dropped(&outcomes, name);
    assert_within_ceiling(&outcomes, name);
    Ok(())
}

/// The runner tells the daemon nothing, and the daemon's own ceiling is all that keeps it inside
/// the path; the 1280-byte path leaves no byte to spare over that ceiling.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_daemon_keeps_a_runner_without_the_ceiling_inside_a_limited_path() -> Result {
    for (name, mtu) in [
        ("a 1370-byte path", MEASURED_PATH_MTU),
        ("a 1280-byte path", IPV6_MIN_MTU),
    ] {
        let path = Path::limited(path_payload_limit(mtu));
        let outcomes = matrix(
            &bulk_sizes()?,
            path,
            CASE_DEADLINE,
            None,
            Builds::RUNNER_WITHOUT_CEILING,
        )
        .await?;
        let name = format!("{name} with a runner that has no ceiling");
        assert_every_payload_arrived(&outcomes, &name);
        assert_nothing_dropped(&outcomes, &name);
        assert_within_ceiling(&outcomes, &name);
    }
    Ok(())
}

/// With no ceiling on either endpoint the daemon's datagrams exceed the path, so the relay drops
/// them and the transfer stalls. That is what gives the passing cases their meaning.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_daemon_and_a_runner_without_the_ceiling_lose_datagrams_on_the_measured_path() -> Result {
    let limit = path_payload_limit(MEASURED_PATH_MTU);
    let outcomes = matrix(
        &[250_000],
        Path::limited(limit),
        STALL_DEADLINE,
        None,
        Builds::BOTH_WITHOUT_CEILING,
    )
    .await?;
    let outcome = outcomes.first().ok_or("no case ran")?;
    assert!(
        matches!(outcome.verdict, Verdict::Stalled(_)),
        "expected the transfer to stall, got {}",
        outcome.verdict
    );
    assert!(
        outcome.traffic.largest_to_client > limit,
        "the daemon never sent more than the path carries: {} bytes",
        outcome.traffic.largest_to_client
    );
    assert!(
        outcome.traffic.dropped_to_client > 0,
        "the path dropped nothing toward the client"
    );
    Ok(())
}
