use std::sync::mpsc;

use super::serve_confined;

/// A process that cannot be confined never serves: its socket is bound, then
/// on Linux a second thread exists, so the single-thread check refuses before
/// anything is applied to this test process; elsewhere there is no Landlock.
#[test]
fn test_a_process_that_cannot_be_confined_never_serves() {
    let refused = std::thread::scope(|scope| {
        // Held open across the call, so a second thread exists whatever the
        // test harness does with its own.
        let (hold, wait) = mpsc::channel::<()>();
        scope.spawn(move || wait.recv());
        let dir = tempfile::tempdir().ok();
        let socket = dir.as_ref().map(|dir| dir.path().join("executor.sock"));
        let refused = socket
            .as_deref()
            .and_then(|socket| serve_confined(socket, socket).err())
            .map(|error| error.to_string());
        drop(hold);
        refused
    });

    let expected = if cfg!(target_os = "linux") {
        "second thread"
    } else {
        "landlock"
    };
    assert!(
        refused
            .as_deref()
            .is_some_and(|text| text.contains(expected)),
        "{refused:?}"
    );
}

/// Outside a sandbox there is no socket directory, so nothing is served and
/// nothing is confined: the socket is bound before anything else.
#[test]
fn test_outside_a_sandbox_there_is_no_socket_to_bind() {
    let tenant = crate::TenantDescriptors {
        tenant_procs: 3,
        tenant_events: 4,
    };
    let refused = super::serve_sandboxed(tenant)
        .err()
        .map(|error| error.to_string());

    assert!(
        refused
            .as_deref()
            .is_some_and(|text| text.contains("executor")),
        "{refused:?}"
    );
}
