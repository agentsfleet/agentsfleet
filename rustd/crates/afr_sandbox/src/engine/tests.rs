use std::path::Path;

use super::{Limits, SandboxRequest};

fn request(lease_id: &str) -> SandboxRequest<'_> {
    SandboxRequest {
        lease_id,
        limits: Limits::default(),
    }
}

#[test]
fn test_a_lease_directory_is_one_component_under_its_base() {
    let dir = request("0198f0c2-7a3e-7c1d-9b2e-4f6a8c0d1e2f")
        .name()
        .map(|name| name.dir_in(Path::new("/srv/leases")));

    assert_eq!(
        dir.ok().as_deref(),
        Some(Path::new(
            "/srv/leases/0198f0c2-7a3e-7c1d-9b2e-4f6a8c0d1e2f"
        ))
    );
}

#[test]
fn test_a_lease_identifier_that_could_leave_its_base_is_refused() {
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

    async fn destroy(self: Box<Self>) -> crate::Result<()> {
        Ok(())
    }
}

#[test]
fn test_a_sandbox_with_nothing_to_watch_counts_as_running() {
    let mut sandbox = Watchless;

    assert!(super::Sandbox::is_running(&mut sandbox));
}
