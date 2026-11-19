#![expect(
    clippy::expect_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::path::Path;

use super::{Limits, SandboxRequest};
use crate::network::{Allowlist, Network};

fn request(run_id: &str) -> SandboxRequest<'_> {
    SandboxRequest::new(run_id, Limits::default())
}

/// A request names no network until it is told one, and then names that one,
/// keeping its run and limits.
#[test]
fn test_a_request_is_isolated_until_told_otherwise() {
    let allowlist = Allowlist::new(Vec::new()).expect("an empty allowlist is within the cap");
    let isolated = request("run-a");

    let shared = isolated.with_network(Network::Host);
    let allowed = isolated.with_network(Network::Allowed(&allowlist));

    assert_eq!(isolated.network, Network::Isolated);
    assert_eq!(shared.network, Network::Host);
    assert_eq!(allowed.network, Network::Allowed(&allowlist));
    assert_eq!(
        (allowed.run_id, allowed.limits),
        (isolated.run_id, isolated.limits)
    );
}

#[test]
fn test_a_run_directory_is_one_component_under_its_base() {
    let dir = request("0198f0c2-7a3e-7c1d-9b2e-4f6a8c0d1e2f")
        .name()
        .map(|name| name.dir_in(Path::new("/srv/runs")));

    assert_eq!(
        dir.ok().as_deref(),
        Some(Path::new("/srv/runs/0198f0c2-7a3e-7c1d-9b2e-4f6a8c0d1e2f"))
    );
}

#[test]
fn test_a_run_identifier_that_could_leave_its_base_is_refused() {
    for unsafe_id in [
        "", ".", "..", "../etc", "a/b", "/abs", "a/..", "x/", "x/.", "a\0b",
    ] {
        let refused = request(unsafe_id).name();

        assert!(
            refused.is_err_and(|error| error.to_string().contains("single path component")),
            "{unsafe_id:?} was accepted"
        );
    }
}

/// A sandbox with no process to watch, keeping every default.
#[derive(Debug)]
struct Watchless;

#[async_trait::async_trait]
impl super::Sandbox for Watchless {
    fn executor(&self) -> &dyn afr_executor::Executor {
        unreachable!("never driven")
    }

    async fn freeze(&self) -> crate::Result<()> {
        Ok(())
    }

    async fn thaw(&self) -> crate::Result<()> {
        Ok(())
    }

    async fn reallow(&mut self, _allowlist: &Allowlist) -> crate::Result<()> {
        Ok(())
    }

    async fn kill(&self) -> super::Killed {
        unreachable!("never driven")
    }

    async fn destroy(self: Box<Self>) -> crate::Result<()> {
        Ok(())
    }
}

#[test]
fn test_a_sandbox_with_nothing_to_watch_counts_as_running() {
    let mut sandbox = Watchless;

    assert!(super::Sandbox::is_running(&mut sandbox));
}

#[tokio::test]
async fn an_engine_without_recovery_proof_refuses_capture() {
    let sandbox = Watchless;
    assert!(
        super::Sandbox::recovery_quiescent(&sandbox)
            .await
            .is_err_and(|failure| matches!(
                failure.kind(),
                crate::error::ErrorKind::Unconfined { .. }
            ))
    );
}
