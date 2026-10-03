//! What happens to processes and calls when either end goes away.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::time::Duration;

use afr_executor::{Ending, Executor as _, Spawn};

use crate::support::{PATIENCE, finish, is_lost, read_until, scratch, serve, start};

#[tokio::test]
async fn test_sandbox_death_interrupts_open_calls() {
    let harness = start().await;
    let quiet = harness
        .client
        .spawn(&Spawn::program("sleep").arg("30"))
        .await
        .unwrap();
    let mut loud = harness.client.spawn(&Spawn::program("yes")).await.unwrap();
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
        .spawn(&Spawn::program("true"))
        .await
        .unwrap_err();
    assert!(is_lost(&after), "{after}");
    assert!(
        std::error::Error::source(&after).is_none(),
        "a lost connection has no deeper cause"
    );
}

#[tokio::test]
async fn dropping_the_client_ends_its_processes_and_the_executor() {
    let harness = start().await;
    let mut loud = harness.client.spawn(&Spawn::program("yes")).await.unwrap();
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

    let second = afr_executor::bind(&socket).unwrap_err();

    assert!(
        second.to_string().contains("input/output"),
        "one executor per socket: {second}"
    );
    first.abort();
}

#[test]
fn a_socket_is_bound_with_no_runtime_and_served_once_one_runs() {
    let (_scratch, socket, root) = scratch();

    // Bound outside any runtime, as the sandbox binds before it hardens.
    let listener = afr_executor::bind(&socket).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let ended = runtime.block_on(async {
        let server = tokio::spawn(async move { listener.serve(&root).await });
        let client = crate::support::connect(&socket).await;
        let echoed = finish(
            client
                .spawn(&Spawn::program("echo").arg("bound"))
                .await
                .unwrap(),
        )
        .await;
        drop(client);
        (echoed, tokio::time::timeout(PATIENCE, server).await)
    });

    assert_eq!(ended.0.stdout, b"bound\n");
    ended.1.expect("the server ended").expect("it ran").unwrap();
}

#[tokio::test]
async fn a_client_waits_for_an_executor_that_binds_late() {
    let (_scratch, socket, root) = scratch();
    let late = {
        let (socket, root) = (socket.clone(), root.clone());
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            afr_executor::serve(&socket, &root).await
        })
    };

    let client = afr_executor::Client::connect_within(&socket, PATIENCE)
        .await
        .unwrap();

    drop(client);
    tokio::time::timeout(PATIENCE, late)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn a_client_gives_up_on_a_socket_that_never_listens() {
    let (_scratch, socket, _root) = scratch();
    let started = std::time::Instant::now();

    let refused = afr_executor::Client::connect_within(&socket, Duration::from_millis(100))
        .await
        .unwrap_err();

    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    assert!(!is_lost(&refused), "{refused}");
}

#[tokio::test]
async fn an_executor_without_its_workspace_does_not_start() {
    let (_scratch, socket, root) = scratch();
    std::fs::remove_dir(&root).unwrap();

    let refused = afr_executor::serve(&socket, &root).await.unwrap_err();

    assert!(!is_lost(&refused), "{refused}");
}

/// The interpreter that leaves a stale socket behind.
const PERL: &str = "perl";

/// Binds and listens on the path it is given, then exits.
const LEAVE_SOCKET: &str =
    "IO::Socket::UNIX->new(Type => SOCK_STREAM(), Local => $ARGV[0], Listen => 1) or die";

#[tokio::test]
async fn a_client_retries_a_socket_left_behind_with_no_one_listening() {
    let (_scratch, socket, _root) = scratch();
    // A process that binds, listens and exits leaves the socket file behind
    // with no one listening. Binding in this process instead would race the
    // suite's other tests on macOS, where close-on-exec is set only after the
    // socket exists: a test spawning a child in that window hands it the
    // listener, which then keeps accepting.
    let left = std::process::Command::new(PERL)
        .args(["-MIO::Socket::UNIX", "-e", LEAVE_SOCKET])
        .arg(&socket)
        .status()
        .unwrap();
    assert!(
        left.success() && socket.exists(),
        "the socket file was left behind"
    );

    let refused = afr_executor::Client::connect_within(&socket, Duration::from_millis(100))
        .await
        .unwrap_err();

    assert!(refused.to_string().contains("input/output"), "{refused}");
}

#[tokio::test]
async fn a_client_does_not_wait_out_a_failure_that_waiting_cannot_fix() {
    let (scratch, _socket, _root) = scratch();
    let locked = scratch.path().join("locked");
    std::fs::create_dir(&locked).unwrap();
    std::fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o000)).unwrap();
    if rustix::process::geteuid().is_root() {
        // Root searches through mode bits, so there is no refusal to see.
        return;
    }
    let started = std::time::Instant::now();

    let refused = afr_executor::Client::connect_within(&locked.join("executor.sock"), PATIENCE)
        .await
        .unwrap_err();

    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    assert!(refused.to_string().contains("input/output"), "{refused}");
    std::fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
}
