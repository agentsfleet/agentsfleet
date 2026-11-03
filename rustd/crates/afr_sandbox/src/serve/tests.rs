#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot write"
)]

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

/// Past any descriptor this test process holds.
const NEVER_OPENED: i32 = 1_000_000;

/// What an entry refused at adoption says: the descriptor's number, never a
/// confinement step, since nothing was hardened yet.
fn refusal(inherited: crate::Inherited) -> Option<String> {
    super::serve_sandboxed(inherited)
        .err()
        .map(|error| error.to_string())
}

/// An entry never handed its link serves nothing and confines nothing.
#[test]
fn test_an_entry_without_its_link_is_refused_before_hardening() {
    let refused = refusal(crate::Inherited {
        tenant: crate::TenantDescriptors {
            tenant_procs: NEVER_OPENED + 1,
            tenant_events: NEVER_OPENED + 2,
        },
        executor: NEVER_OPENED,
    });

    assert!(
        refused
            .as_deref()
            .is_some_and(|text| text.contains(&NEVER_OPENED.to_string())),
        "{refused:?}"
    );
}

/// An entry handed its link but not its tenant leaf stops at the leaf, before
/// anything is hardened, so no process it could start runs outside the leaf.
#[test]
fn test_an_entry_without_its_tenant_leaf_is_refused_before_hardening() {
    use std::os::fd::IntoRawFd as _;
    let ends = std::os::unix::net::UnixStream::pair().ok();
    let executor = ends.map(|(_runner, entry)| entry.into_raw_fd());

    let refused = executor.and_then(|executor| {
        refusal(crate::Inherited {
            tenant: crate::TenantDescriptors {
                tenant_procs: NEVER_OPENED,
                tenant_events: NEVER_OPENED + 1,
            },
            executor,
        })
    });

    assert!(
        refused
            .as_deref()
            .is_some_and(|text| text.contains(&NEVER_OPENED.to_string())),
        "{refused:?}"
    );
}

/// An entry handed every descriptor it needs adopts all three and goes on to
/// harden, which refuses here: a second thread exists on Linux, and no other
/// kernel has Landlock.
#[test]
fn test_an_entry_handed_its_descriptors_goes_on_to_harden() {
    let leaf = tempfile::tempdir().unwrap();
    std::fs::write(
        leaf.path().join("memory.events"),
        "low 0\nhigh 0\nmax 0\noom 0\noom_kill 0\noom_group_kill 0\n",
    )
    .unwrap();
    let tenant = crate::tenant::TenantFiles::open(
        &leaf.path().join("cgroup.procs"),
        &leaf.path().join("memory.events"),
    )
    .unwrap();
    let (handover, _runner) = crate::tenant::Handover::with_link(tenant).unwrap();

    let refused = super::serve_sandboxed(handover.into_inherited()).unwrap_err();

    let expected = if cfg!(target_os = "linux") {
        "second thread"
    } else {
        "landlock"
    };
    let said = refused.to_string();
    assert!(said.contains(expected), "{said}");
}
