#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot build"
)]

use afr_executor::{Ending, ProcessEvent, Spawn};

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
    let mut process = sandbox
        .executor()
        .spawn(Spawn::program("/bin/echo").arg("hi"))
        .await
        .unwrap();
    let mut output = Vec::new();
    let ending = loop {
        match process.events.recv().await.unwrap() {
            ProcessEvent::Output { data, .. } => output.extend_from_slice(&data),
            ProcessEvent::Ended { ending, .. } => break ending,
        }
    };
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
