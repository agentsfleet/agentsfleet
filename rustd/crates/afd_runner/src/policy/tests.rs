//! The whole decode matrix, with no datastore anywhere near it.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use super::*;
use afd_wire::runner::{NetworkPolicy, SandboxTier};

/// Columns that spell a whole assignment decode to it.
#[test]
fn test_decode_resolves_a_fully_assigned_row() {
    let columns = AssignmentColumns {
        sandbox_tier: "landlock_full".to_owned(),
        network_policy: "allow_all".to_owned(),
        registry_allowlist_json: r#"["registry.npmjs.org","pypi.org"]"#.to_owned(),
        worker_count: 4,
        sandbox_config_json: None,
    };

    let decoded = columns
        .decode()
        .expect("every column is present and readable");

    assert_eq!(decoded.sandbox_tier, SandboxTier::LandlockFull);
    assert_eq!(decoded.network_policy, NetworkPolicy::AllowAll);
    assert_eq!(
        decoded.registry_allowlist,
        ["registry.npmjs.org", "pypi.org"]
    );
    assert_eq!(decoded.worker_count, 4);
    // Absent binds are the normal state, not a reason to void the row.
    assert!(decoded.extra_binds.is_empty(), "{:?}", decoded.extra_binds);
}

/// An unreadable policy column fails the read, naming the column, rather
/// than completing the assignment with a guess.
#[test]
fn test_decode_refuses_every_unreadable_column() {
    let sound = AssignmentColumns {
        sandbox_tier: "landlock_full".to_owned(),
        network_policy: "allow_all".to_owned(),
        registry_allowlist_json: "[]".to_owned(),
        worker_count: 1,
        sandbox_config_json: None,
    };
    assert!(sound.decode().is_ok(), "the control row must decode");

    let broken = [
        (
            AssignmentColumns {
                network_policy: "open_sesame".to_owned(),
                ..sound.clone()
            },
            COLUMN_NETWORK_POLICY,
        ),
        (
            AssignmentColumns {
                sandbox_tier: "quantum_cage".to_owned(),
                ..sound.clone()
            },
            COLUMN_SANDBOX_TIER,
        ),
        (
            AssignmentColumns {
                registry_allowlist_json: "not-json".to_owned(),
                ..sound.clone()
            },
            COLUMN_REGISTRY_ALLOWLIST,
        ),
    ];
    for (columns, column) in broken {
        let refused = columns
            .decode()
            .expect_err("an unreadable column must fail the read");
        assert_eq!(refused.code(), afd_core::error_code::INTERNAL_DB_QUERY);
        assert!(
            refused.to_string().contains(column),
            "the refusal names `{column}`: {refused}"
        );
    }
}

/// A stored count outside the shared bounds is clamped on the way out.
#[test]
fn test_decode_clamps_a_row_edited_out_of_band() {
    let out_of_range = |workers: i32| AssignmentColumns {
        sandbox_tier: "dev_none".to_owned(),
        network_policy: "allow_all".to_owned(),
        registry_allowlist_json: "[]".to_owned(),
        worker_count: workers,
        sandbox_config_json: None,
    };

    let below = out_of_range(-7);
    let above = out_of_range(i32::MAX);
    let floor = below.decode().expect("a clamped row still decodes");
    let ceiling = above.decode().expect("a clamped row still decodes");

    assert_eq!(floor.worker_count, afd_core::limits::MIN_WORKERS);
    assert_eq!(ceiling.worker_count, afd_core::limits::MAX_WORKERS);
}

/// A garbled bind list empties rather than voiding the assignment.
#[test]
fn test_extra_binds_degrade_to_the_baseline() {
    let columns = AssignmentColumns {
        sandbox_tier: "dev_none".to_owned(),
        network_policy: "allow_all".to_owned(),
        registry_allowlist_json: "[]".to_owned(),
        worker_count: 1,
        sandbox_config_json: Some("{{not-json".to_owned()),
    };

    let decoded = columns
        .decode()
        .expect("a garbled bind list must not void the row");

    assert!(decoded.extra_binds.is_empty(), "{:?}", decoded.extra_binds);
}

/// A steady verdict is recognised as steady; a moved one as moved.
#[test]
fn test_stored_verdict_detects_only_real_movement() {
    let healthy = StoredVerdict { reason: None };
    let degraded = StoredVerdict {
        reason: Some(crate::reconcile::REASON_NO_CAPABILITY_REPORT.to_owned()),
    };
    let fresh = Verdict::Degraded {
        reason: crate::reconcile::REASON_NO_CAPABILITY_REPORT,
    };

    assert!(healthy.differs_from(fresh), "healthy row, degraded verdict");
    assert!(
        !degraded.differs_from(fresh),
        "the same degradation is not movement"
    );
    assert!(
        degraded.differs_from(Verdict::Healthy),
        "cleared is movement"
    );
    assert!(
        !healthy.differs_from(Verdict::Healthy),
        "steady health writes nothing"
    );
}

/// What an edit writes into `sandbox_config` is what every reader decodes.
#[test]
fn test_sandbox_config_round_trips_through_its_column() {
    let bind = ExtraBind {
        path: std::borrow::Cow::Borrowed("/srv/models"),
        mode: afd_wire::runner::BindMode::ReadOnly,
        note: std::borrow::Cow::Borrowed("model cache"),
    };
    let rendered = SandboxConfig {
        extra_binds: vec![bind.clone()],
    }
    .render();
    let columns = AssignmentColumns {
        sandbox_tier: "dev_none".to_owned(),
        network_policy: "allow_all".to_owned(),
        registry_allowlist_json: "[]".to_owned(),
        worker_count: 1,
        sandbox_config_json: Some(rendered),
    };

    let decoded = columns.decode().expect("a rendered configuration decodes");

    assert_eq!(decoded.extra_binds, vec![bind]);
}

/// A configuration naming no binds reads as the baseline, not as a fault.
#[test]
fn test_a_configuration_without_binds_is_the_baseline() {
    let columns = AssignmentColumns {
        sandbox_tier: "dev_none".to_owned(),
        network_policy: "allow_all".to_owned(),
        registry_allowlist_json: "[]".to_owned(),
        worker_count: 1,
        sandbox_config_json: Some("{}".to_owned()),
    };

    let decoded = columns.decode().expect("an empty configuration decodes");

    assert!(decoded.extra_binds.is_empty(), "{:?}", decoded.extra_binds);
}
