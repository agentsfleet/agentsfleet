#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot build"
)]

use afr_executor::{Ending, Spawn};

use super::{UnsandboxedEngine, permitted};
use crate::engine::{Engine, Limits, SandboxRequest};

#[test]
fn test_release_build_refuses_unsandboxed_engine() {
    let refused = permitted(false).unwrap_err();

    assert!(
        refused
            .to_string()
            .contains("never runs a tool call unsandboxed"),
        "{refused}"
    );
    // A debug build, which every test lane is, may use it.
    permitted(true).unwrap();
}

#[tokio::test]
async fn test_an_unsandboxed_lease_runs_a_process_and_is_removed() {
    let base = tempfile::Builder::new()
        .prefix("afr")
        .tempdir_in("/tmp")
        .unwrap();
    let engine = UnsandboxedEngine::new(base.path().to_owned()).unwrap();

    let mut sandbox = engine
        .prepare(SandboxRequest {
            lease_id: "l1",
            limits: Limits::default(),
        })
        .await
        .unwrap();
    assert!(sandbox.is_running(), "its executor is serving");
    let process = sandbox
        .executor()
        .spawn(&Spawn::program("/bin/echo").arg("hi"))
        .await
        .unwrap();
    let mut output = Vec::new();
    let ending = process
        .ended(|_stream, data| output.extend_from_slice(&data))
        .await
        .unwrap();
    sandbox.destroy().await.unwrap();

    assert_eq!(
        (ending, output.as_slice()),
        (Ending::Exited(0), b"hi\n".as_slice())
    );
    assert!(
        !base.path().join("l1").exists(),
        "the lease's directory is gone"
    );
}

#[tokio::test]
async fn should_show_the_host_the_workspace_its_executor_serves() {
    let base = tempfile::Builder::new()
        .prefix("afr")
        .tempdir_in("/tmp")
        .unwrap();
    let engine = UnsandboxedEngine::new(base.path().to_owned()).unwrap();
    let sandbox = engine
        .prepare(SandboxRequest {
            lease_id: "host-view",
            limits: Limits::default(),
        })
        .await
        .unwrap();

    let workspace = sandbox.workspace().unwrap();
    std::fs::write(workspace.root.join("planted.txt"), "from the host").unwrap();
    let owner = workspace.owner;
    let read = sandbox
        .executor()
        .read_file("planted.txt", 64)
        .await
        .unwrap();
    sandbox.destroy().await.unwrap();

    assert_eq!(
        read.data, "from the host",
        "the executor reads what the host wrote"
    );
    assert_eq!(
        owner,
        (
            rustix::process::getuid().as_raw(),
            rustix::process::getgid().as_raw()
        ),
        "the files belong to whoever runs the engine"
    );
}

#[tokio::test]
async fn test_a_lease_directory_that_cannot_be_made_is_refused() {
    let base = tempfile::Builder::new()
        .prefix("afr")
        .tempdir_in("/tmp")
        .unwrap();
    std::fs::write(base.path().join("l2"), "").unwrap();
    let engine = UnsandboxedEngine::new(base.path().to_owned()).unwrap();

    let refused = engine
        .prepare(SandboxRequest {
            lease_id: "l2",
            limits: Limits::default(),
        })
        .await;

    refused.unwrap_err();
}

#[tokio::test]
async fn test_an_executor_that_never_answers_refuses_the_lease_and_cleans_up() {
    let base = tempfile::Builder::new()
        .prefix("afr")
        .tempdir_in("/tmp")
        .unwrap();
    // A directory where the socket goes: the executor cannot bind, so nothing
    // ever answers.
    std::fs::create_dir_all(base.path().join("l3").join("executor.sock")).unwrap();
    let engine = UnsandboxedEngine::new(base.path().to_owned()).unwrap();

    let refused = engine
        .prepare(SandboxRequest {
            lease_id: "l3",
            limits: Limits::default(),
        })
        .await;

    // The executor's own failure, logged under the executor's code.
    let refused = refused.unwrap_err();
    assert_eq!(
        refused.code(),
        afd_core::error_code::INTERNAL_OPERATION_FAILED
    );
    assert!(
        !base.path().join("l3").exists(),
        "the lease's directory is removed"
    );
}

#[tokio::test(start_paused = true)]
async fn test_destroy_aborts_a_lingering_server_and_removes_its_directory() {
    use crate::engine::Sandbox as _;
    let base = tempfile::Builder::new()
        .prefix("afr")
        .tempdir_in("/tmp")
        .unwrap();
    let dir = base.path().join("lingering");
    std::fs::create_dir(&dir).unwrap();
    let socket = dir.join(crate::bubblewrap::SOCKET_NAME);
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let client = super::connect(&socket).await.unwrap();
    let (_peer, _) = listener.accept().await.unwrap();
    let server = tokio::spawn(std::future::pending());
    let abort = server.abort_handle();
    let sandbox = Box::new(super::Unconfined {
        dir: dir.clone(),
        workspace: dir.join(super::WORKSPACE_DIR),
        client,
        server,
    });
    let started = tokio::time::Instant::now();
    sandbox.destroy().await.unwrap();
    tokio::task::yield_now().await;
    assert_eq!(started.elapsed(), super::SERVER_GRACE);
    assert!(abort.is_finished(), "the lingering task was aborted");
    assert!(!dir.exists(), "the lease directory was removed");
}
