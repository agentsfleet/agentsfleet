#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot build"
)]

use afr_executor::{Ending, Spawn};

use super::{UnsandboxedEngine, permitted};
use std::sync::Arc;

use af_core::id::Uuid7;

use crate::engine::{Engine, Limits, ParkMode, SandboxSpec, SandboxState, fresh_sandbox_id};
use crate::error::EgressRefusal;
use crate::network::Allowlist;

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
async fn test_an_unsandboxed_runs_a_process_and_is_removed() {
    let base = tempfile::Builder::new()
        .prefix("afr")
        .tempdir_in("/tmp")
        .unwrap();
    let id = fresh_sandbox_id();
    let engine = UnsandboxedEngine::new(base.path().to_owned()).unwrap();

    let mut sandbox = engine
        .prepare(SandboxSpec::granted_by(&engine, &id, Limits::DEFAULT).unwrap())
        .await
        .unwrap();
    assert_eq!(
        sandbox.state(),
        SandboxState::Running,
        "its executor is serving"
    );
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
        !base.path().join(id.as_str()).exists(),
        "the run's directory is gone"
    );
}

/// Its processes run on the host under no cgroup, so nothing counts them or
/// stops them together: quiescence is refused saying so, and a park is
/// refused naming `park`, with the sandbox destroyed rather than kept.
#[tokio::test]
async fn test_an_unconfined_sandbox_neither_counts_nor_parks() {
    let base = tempfile::Builder::new()
        .prefix("afr")
        .tempdir_in("/tmp")
        .unwrap();
    let id = fresh_sandbox_id();
    let engine = UnsandboxedEngine::new(base.path().to_owned()).unwrap();
    let sandbox = engine
        .prepare(SandboxSpec::granted_by(&engine, &id, Limits::DEFAULT).unwrap())
        .await
        .unwrap();

    let counted = sandbox.quiesce().await.unwrap_err().detail();
    let parked = sandbox.park(ParkMode::Pause).await.unwrap_err().detail();

    assert!(counted.contains(super::NO_QUIESCENCE), "{counted}");
    assert_eq!(parked, "the unsandboxed engine cannot park");
    assert!(
        !base.path().join(id.as_str()).exists(),
        "a sandbox that will not park is destroyed"
    );
}

/// 100 builds released together for one identifier make one sandbox: one
/// answers it, and 99 are refused naming it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_a_second_prepare_for_one_id_is_refused_naming_it() {
    const CALLERS: usize = 100;
    let base = tempfile::Builder::new()
        .prefix("afr")
        .tempdir_in("/tmp")
        .unwrap();
    let id = fresh_sandbox_id();
    let engine = Arc::new(UnsandboxedEngine::new(base.path().to_owned()).unwrap());
    let barrier = Arc::new(tokio::sync::Barrier::new(CALLERS));
    let mut builds = tokio::task::JoinSet::new();
    for _ in 0..CALLERS {
        let (engine, barrier, id) = (Arc::clone(&engine), Arc::clone(&barrier), id.clone());
        builds.spawn(async move {
            let spec = SandboxSpec::granted_by(&*engine, &id, Limits::DEFAULT).unwrap();
            barrier.wait().await;
            engine.prepare(spec).await
        });
    }

    let mut built = Vec::new();
    let mut refused = Vec::new();
    while let Some(joined) = builds.join_next().await {
        match joined.unwrap() {
            Ok(sandbox) => built.push(sandbox),
            Err(error) => refused.push(error.detail()),
        }
    }
    let named = built
        .iter()
        .map(|sandbox| sandbox.id().clone())
        .collect::<Vec<Uuid7>>();
    for sandbox in built {
        sandbox.destroy().await.unwrap();
    }

    assert_eq!(
        named,
        std::slice::from_ref(&id),
        "one sandbox, under the asked identifier"
    );
    assert_eq!(refused.len(), CALLERS - 1);
    let live = format!("sandbox {id} is already live");
    assert!(refused.iter().all(|said| *said == live), "{refused:?}");
}

#[tokio::test]
async fn test_a_run_directory_that_cannot_be_made_is_refused() {
    let base = tempfile::Builder::new()
        .prefix("afr")
        .tempdir_in("/tmp")
        .unwrap();
    let id = fresh_sandbox_id();
    std::fs::write(base.path().join(id.as_str()), "").unwrap();
    let engine = UnsandboxedEngine::new(base.path().to_owned()).unwrap();

    let refused = engine
        .prepare(SandboxSpec::granted_by(&engine, &id, Limits::DEFAULT).unwrap())
        .await;

    refused.unwrap_err();
}

#[tokio::test]
async fn test_an_executor_that_never_answers_refuses_the_run_and_cleans_up() {
    // A base so long the executor's socket path passes the operating
    // system's limit on one: it cannot bind, so nothing ever answers.
    let base = tempfile::Builder::new()
        .prefix(&"afr".repeat(40))
        .tempdir_in("/tmp")
        .unwrap();
    let id = fresh_sandbox_id();
    let engine = UnsandboxedEngine::new(base.path().to_owned()).unwrap();

    let refused = engine
        .prepare(SandboxSpec::granted_by(&engine, &id, Limits::DEFAULT).unwrap())
        .await;

    // The executor's own failure, logged under the executor's code.
    let refused = refused.unwrap_err();
    assert_eq!(
        refused.code(),
        af_core::error_code::INTERNAL_OPERATION_FAILED
    );
    assert!(
        !base.path().join(id.as_str()).exists(),
        "the sandbox's directory is removed"
    );
}

#[tokio::test(start_paused = true)]
async fn test_destroy_aborts_a_lingering_server_and_removes_its_directory() {
    use crate::engine::Sandbox as _;
    let base = tempfile::Builder::new()
        .prefix("afr")
        .tempdir_in("/tmp")
        .unwrap();
    let id = fresh_sandbox_id();
    let dir = base.path().join(id.as_str());
    std::fs::create_dir(&dir).unwrap();
    let socket = dir.join(super::SOCKET_NAME);
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let client = super::connect(&socket, afr_executor::Sizes::DEFAULT)
        .await
        .unwrap();
    let (_peer, _) = listener.accept().await.unwrap();
    let server = tokio::spawn(std::future::pending());
    let abort = server.abort_handle();
    let sandbox = Box::new(super::Unconfined {
        id: id.clone(),
        granted: crate::engine::Granted::unenforced(Limits::DEFAULT),
        environment: crate::engine::Environment {
            engine: af_wire::runner::SandboxEngine::Unsandboxed,
            root: crate::engine::Root::Host,
        },
        dir: dir.clone(),
        client,
        server,
    });
    let started = tokio::time::Instant::now();
    sandbox.destroy().await.unwrap();
    tokio::task::yield_now().await;
    assert_eq!(started.elapsed(), super::SERVER_GRACE);
    assert!(abort.is_finished(), "the lingering task was aborted");
    assert!(!dir.exists(), "the run directory was removed");
}

/// An unsandboxed run shares the host's network and confines nothing to an
/// allowlist, so one asked to take new addresses refuses as having no scope,
/// and its run builds fresh: the refill is never taken as done.
#[tokio::test]
async fn should_refuse_new_addresses_it_has_no_scope_for() {
    let base = tempfile::Builder::new()
        .prefix("afr")
        .tempdir_in("/tmp")
        .unwrap();
    let id = fresh_sandbox_id();
    let engine = UnsandboxedEngine::new(base.path().to_owned()).unwrap();
    let mut sandbox = engine
        .prepare(SandboxSpec::granted_by(&engine, &id, Limits::DEFAULT).unwrap())
        .await
        .unwrap();

    let refused = sandbox.reallow(&Allowlist::new(Vec::new()).unwrap()).await;
    sandbox.destroy().await.unwrap();

    assert_eq!(
        refused.unwrap_err().egress_refusal(),
        Some(&EgressRefusal::NoScope)
    );
}

/// No cgroup holds an unsandboxed run's processes, so its kill proves
/// nothing about them: it is never settled.
#[tokio::test]
async fn test_an_unsandboxed_kill_is_never_settled() {
    let base = tempfile::Builder::new()
        .prefix("afr")
        .tempdir_in("/tmp")
        .unwrap();
    let id = fresh_sandbox_id();
    let engine = UnsandboxedEngine::new(base.path().to_owned()).unwrap();
    let sandbox = engine
        .prepare(SandboxSpec::granted_by(&engine, &id, Limits::DEFAULT).unwrap())
        .await
        .unwrap();

    let killed = sandbox.kill().await;
    sandbox.destroy().await.unwrap();

    assert!(matches!(killed, crate::engine::Killed::Unsettled(_)));
}
