//! Each enrolment bound at its edge, answered with its own sentence.

#![expect(
    clippy::expect_used,
    reason = "validated boundary values are test fixture preconditions"
)]

use std::borrow::Cow;

use afd_core::limits::MAX_WORKERS;
use afd_wire::runner::{
    BindMode, ExtraBind, HOST_ID_MAX_BYTES, NetworkPolicy, REGISTRY_ENTRIES_MAX, SandboxTier,
};

use super::*;

fn bind(path: &str) -> ExtraBind<'_> {
    ExtraBind {
        path: Cow::Borrowed(path),
        mode: BindMode::ReadOnly,
        note: Cow::Borrowed("operator reason"),
    }
}

fn policy(registry_allowlist: Vec<Cow<'_, str>>, worker_count: u32) -> AssignedPolicy<'_> {
    AssignedPolicy {
        sandbox_tier: SandboxTier::LandlockFull,
        network_policy: NetworkPolicy::AllowListEgress,
        registry_allowlist,
        worker_count,
        extra_binds: Vec::new(),
    }
}

fn with_binds(extra_binds: Vec<ExtraBind<'_>>) -> AssignedPolicy<'_> {
    AssignedPolicy {
        extra_binds,
        ..policy(Vec::new(), 1)
    }
}

fn request(host_id: String, assigned_policy: AssignedPolicy<'static>) -> RegisterRequest<'static> {
    RegisterRequest {
        host_id: Cow::Owned(host_id),
        assigned_policy,
        labels: Vec::new(),
    }
}

/// The sentence a request earns, or `None` when it is accepted.
fn refused(value: &RegisterRequest<'_>) -> Option<&'static str> {
    registration(value).err().map(|error| error.detail())
}

fn binds_refusal(path: &str) -> Option<&'static str> {
    assignment(&with_binds(vec![bind(path)]))
        .err()
        .map(|error| error.detail())
}

#[test]
fn test_register_request_bounds_refuse_with_their_sentences() {
    let host = || "runner.example".to_owned();
    assert_eq!(refused(&request(host(), policy(Vec::new(), 1))), None);

    let long_host = "h".repeat(HOST_ID_MAX_BYTES + 1);
    assert_eq!(
        refused(&request(long_host, policy(Vec::new(), 1))),
        Some(DETAIL_HOST_ID_BOUNDS)
    );

    let entries = (0..=REGISTRY_ENTRIES_MAX)
        .map(|index| Cow::Owned(format!("registry-{index}.example")))
        .collect();
    assert_eq!(
        refused(&request(host(), policy(entries, 1))),
        Some(DETAIL_REGISTRY_ALLOWLIST)
    );
    let six_digit_port = vec![Cow::Borrowed("registry.example:123456")];
    assert_eq!(
        refused(&request(host(), policy(six_digit_port, 1))),
        Some(DETAIL_REGISTRY_ALLOWLIST)
    );

    let too_many = (0..=EXTRA_BINDS_MAX)
        .map(|index| ExtraBind {
            path: Cow::Owned(format!("/srv/models-{index}")),
            ..bind("/srv/models")
        })
        .collect();
    assert_eq!(
        refused(&request(host(), with_binds(too_many))),
        Some(DETAIL_EXTRA_BINDS_COUNT)
    );
    let long_note = ExtraBind {
        note: Cow::Owned("n".repeat(BIND_NOTE_MAX_BYTES + 1)),
        ..bind("/srv/models")
    };
    assert_eq!(
        refused(&request(host(), with_binds(vec![long_note]))),
        Some(DETAIL_EXTRA_BIND_NOTE)
    );
    assert_eq!(
        refused(&request(host(), with_binds(vec![bind("/")]))),
        Some(DETAIL_EXTRA_BINDS)
    );

    let labelled = |labels: Vec<Cow<'static, str>>| RegisterRequest {
        labels,
        ..request(host(), policy(Vec::new(), 1))
    };
    assert_eq!(
        refused(&labelled(vec![Cow::Borrowed("gpu"); LABELS_MAX])),
        None
    );
    assert_eq!(
        refused(&labelled(vec![Cow::Borrowed("gpu"); LABELS_MAX + 1])),
        Some(DETAIL_LABELS)
    );
    assert_eq!(
        refused(&labelled(vec![Cow::Owned("l".repeat(LABEL_MAX_BYTES + 1))])),
        Some(DETAIL_LABELS)
    );
}

#[test]
fn each_bind_sentence_names_its_own_bound() {
    // Three bounds used to share one sentence that named none of them.
    assert!(DETAIL_EXTRA_BINDS_COUNT.contains(&EXTRA_BINDS_MAX.to_string()));
    assert!(DETAIL_EXTRA_BIND_NOTE.contains(&BIND_NOTE_MAX_BYTES.to_string()));
    assert!(DETAIL_EXTRA_BINDS.contains(&BIND_PATH_MAX_BYTES.to_string()));
    assert!(DETAIL_LABELS.contains(&LABELS_MAX.to_string()));
    assert!(DETAIL_LABELS.contains(&LABEL_MAX_BYTES.to_string()));
}

#[test]
fn worker_counts_are_clamped_at_the_boundary() {
    assert_eq!(
        assignment(&policy(Vec::new(), 0))
            .expect("zero workers is clamped")
            .worker_count
            .get(),
        1
    );
    assert_eq!(
        registration(&request("h".to_owned(), policy(Vec::new(), u32::MAX)))
            .expect("an excessive worker count is clamped")
            .worker_count
            .get(),
        MAX_WORKERS
    );
}

#[test]
fn registry_entries_accept_only_bare_hosts_and_optional_decimal_ports() {
    for admitted in ["registry.example", "registry_1.example:443"] {
        let _stored = assignment(&policy(vec![Cow::Borrowed(admitted)], 1))
            .expect("a bare registry host is accepted");
    }
    for refused in [
        "",
        ":443",
        "registry.example:",
        "registry.example:123456",
        "registry.example:44x",
        "registry.example:443:extra",
        "https://registry.example",
        "bad host",
    ] {
        let error = assignment(&policy(vec![Cow::Borrowed(refused)], 1))
            .expect_err("the registry entry is not a host[:port]");
        assert_eq!(error.detail(), DETAIL_REGISTRY_ALLOWLIST, "{refused}");
    }
}

#[test]
fn test_extra_bind_validation_accepts_only_canonical_unprotected_paths() {
    assert_eq!(binds_refusal("/srv/models"), None);
    for refused in [
        "relative/path",
        "/srv/../root",
        "/srv/data/",
        "/",
        "/etc/ssl",
        "/run",
        "/var",
        "/etc/./ssl",
        "//etc",
        "/srv/nul\0byte",
    ] {
        assert_eq!(
            binds_refusal(refused),
            Some(DETAIL_EXTRA_BINDS),
            "accepted {refused}"
        );
    }
    assert_eq!(binds_refusal("/etcetera"), None);
    assert_eq!(
        binds_refusal(&format!("/{}", "a".repeat(BIND_PATH_MAX_BYTES))),
        Some(DETAIL_EXTRA_BINDS)
    );
}

#[test]
fn test_extra_bind_validation_enforces_list_path_and_note_bounds() {
    let at_cap = (0..EXTRA_BINDS_MAX)
        .map(|index| ExtraBind {
            path: Cow::Owned(format!("/srv/models-{index}")),
            mode: BindMode::ReadOnly,
            note: Cow::Borrowed(""),
        })
        .collect::<Vec<_>>();
    let _stored = assignment(&with_binds(at_cap.clone())).expect("sixteen binds fit");

    let mut over = at_cap;
    over.push(bind("/srv/one-too-many"));
    assert_eq!(
        assignment(&with_binds(over))
            .err()
            .map(|error| error.detail()),
        Some(DETAIL_EXTRA_BINDS_COUNT)
    );
}
