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
    let dir = request("0198f0c2-7a3e-7c1d-9b2e-4f6a8c0d1e2f").lease_dir(Path::new("/srv/leases"));

    assert_eq!(
        dir.ok().as_deref(),
        Some(Path::new(
            "/srv/leases/0198f0c2-7a3e-7c1d-9b2e-4f6a8c0d1e2f"
        ))
    );
}

#[test]
fn test_a_lease_identifier_that_could_leave_its_base_is_refused() {
    for unsafe_id in ["", ".", "..", "../etc", "a/b", "/abs", "a/.."] {
        let refused = request(unsafe_id).lease_dir(Path::new("/srv/leases"));

        assert!(
            refused.is_err_and(|error| error.to_string().contains("single path component")),
            "{unsafe_id:?} was accepted"
        );
    }
}
