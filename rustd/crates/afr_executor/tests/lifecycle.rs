//! What happens to processes and calls when either end goes away.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::time::Duration;

use afr_executor::{Ending, Executor as _, Spawn};

use crate::support::{PATIENCE, finish, read_until, scratch, serve, start};

#[tokio::test]
async fn test_sandbox_death_interrupts_open_calls() {
    let harness = start().await;
    let quiet = harness
        .client
        .spawn(Spawn::program("sleep").arg("30"))
        .await
        .unwrap();
    let mut loud = harness.client.spawn(Spawn::program("yes")).await.unwrap();
    read_until(&mut loud, "y\n").await;

    // The sandbox dies under both: the executor stops with no goodbye.
    harness.server.abort();
    let (quiet, loud) = (finish(quiet).await, finish(loud).await);

    assert_eq!(
        quiet.endings,
        [(Ending::Interrupted, 0)],
        "exactly one end, and it says why"
    );
    assert_eq!(loud.endings, [(Ending::Interrupted, 0)]);
    let after = harness
        .client
        .spawn(Spawn::program("true"))
        .await
        .unwrap_err();
    assert!(after.is_connection_lost(), "{after}");
    assert!(after.to_string().contains("connection closed"), "{after}");
    assert!(
        std::error::Error::source(&after).is_none(),
        "a lost connection has no deeper cause"
    );
}

#[tokio::test]
async fn dropping_the_client_ends_its_processes_and_the_executor() {
    let harness = start().await;
    let mut loud = harness.client.spawn(Spawn::program("yes")).await.unwrap();
    read_until(&mut loud, "y\n").await;

    drop(harness.client);
    let served = tokio::time::timeout(PATIENCE, harness.server)
        .await
        .expect("the executor ended");

    served.expect("the serve task ran to its end").unwrap();
    assert_eq!(finish(loud).await.endings, [(Ending::Interrupted, 0)]);
    drop(harness.scratch);
}

#[tokio::test]
async fn a_second_executor_on_a_bound_socket_is_refused() {
    let (_scratch, socket, root) = scratch();
    let first = serve(&socket, &root);
    while !socket.exists() {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    let second = afr_executor::serve(&socket, &root).await;

    assert!(second.is_err(), "one executor per socket");
    first.abort();
}

#[tokio::test]
async fn an_executor_without_its_workspace_does_not_start() {
    let (_scratch, socket, root) = scratch();
    std::fs::remove_dir(&root).unwrap();

    let refused = afr_executor::serve(&socket, &root).await.unwrap_err();

    assert!(!refused.is_connection_lost(), "{refused}");
}
