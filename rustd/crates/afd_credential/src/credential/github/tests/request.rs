//! What a mint asks for, against the installation it asks.
//!
//! A token cannot carry a permission its App was never granted, so what the
//! installation holds decides which evidence reads are asked for. The
//! binding's own reach does not bend to it.

use afd_fleet_runtime::config::Access;
use serde_json::json;

use super::{ScopedRequest, binding, installed, scoped};

/// What a request's permissions serialise to, which is what GitHub receives.
fn sent(request: &ScopedRequest) -> serde_json::Value {
    serde_json::to_value(request).expect("the request serialises")["permissions"].clone()
}

/// Dimension 1.1 — a read mint asks for the repository and the CI evidence,
/// all at read, and nothing else: no write, no `pull_requests`, no `workflows`.
#[test]
fn read_mint_requests_ci_evidence_reads() {
    let request = scoped(&binding(Access::Read));

    // The owner is stripped, because GitHub scopes by bare name.
    let body = serde_json::to_value(&request).expect("the request serialises");
    assert_eq!(body["repositories"], json!(["widgets"]));
    assert_eq!(
        sent(&request),
        json!({"actions": "read", "checks": "read", "contents": "read"})
    );
}

/// Dimension 1.3 — a write mint keeps the evidence reads, raises `contents` to
/// write and adds `pull_requests` write, and still asks for no `workflows`.
#[test]
fn write_mint_keeps_the_evidence_reads() {
    assert_eq!(
        sent(&scoped(&binding(Access::Write))),
        json!({
            "actions": "read",
            "checks": "read",
            "contents": "write",
            "pull_requests": "write",
        })
    );
}

/// Dimension 1.4 — an evidence read the installation does not hold is left out
/// of the request, so the mint still succeeds and only that read answers 403.
#[test]
fn evidence_reads_follow_what_the_installation_holds() {
    let read = binding(Access::Read);
    for (case, holds, asks) in [
        (
            "no checks",
            json!({"contents": "read", "actions": "read", "metadata": "read"}),
            json!({"actions": "read", "contents": "read"}),
        ),
        (
            "no evidence at all",
            json!({"contents": "read", "metadata": "read"}),
            json!({"contents": "read"}),
        ),
        (
            "checks at a level this daemon does not model",
            json!({"contents": "read", "actions": "read", "checks": "maintain"}),
            json!({"actions": "read", "contents": "read"}),
        ),
        (
            "evidence held above read is still asked for at read",
            json!({"contents": "read", "actions": "write", "checks": "write"}),
            json!({"actions": "read", "checks": "read", "contents": "read"}),
        ),
    ] {
        let request = ScopedRequest::for_binding(&read, &installed(holds));
        assert_eq!(sent(&request), asks, "{case}");
    }
}

/// Dimension 1.5 — the binding's own reach is asked for even where the
/// installation lacks it, so a repair it cannot make fails at the mint rather
/// than arriving as a narrower token.
#[test]
fn write_reach_is_asked_for_where_the_installation_lacks_it() {
    let lacking = installed(json!({"contents": "read", "metadata": "read"}));
    let request = ScopedRequest::for_binding(&binding(Access::Write), &lacking);

    assert_eq!(
        sent(&request),
        json!({"contents": "write", "pull_requests": "write"})
    );
}
