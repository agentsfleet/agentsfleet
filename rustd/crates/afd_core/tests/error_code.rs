//! The registry is a checked subset of the Zig one, not a second source of truth.
#![expect(
    clippy::unwrap_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::collections::BTreeSet;

use afd_core::error_code::{self, ErrorCode, REGISTRY};

/// Every code `src/agentsfleetd/errors/error_registry.zig` declared at sunset.
///
/// FROZEN, not read. The Zig registry was the registry of record and this test
/// read it from disk; the tree is deleted in this milestone, so the values it
/// carried are pinned here and the assertion below is unchanged. That the Zig
/// file was where the expectation came from is not what made it worth
/// asserting: a Rust code outside this set is one the product never shipped,
/// and adding it is a public-contract decision, not a typo to wave through.
///
/// Recorded from the tree while it still stood (169 codes).
const ZIG_DECLARED: &[&str] = &[
    "UZ-AGT-003",
    "UZ-AGT-004",
    "UZ-AGT-006",
    "UZ-AGT-008",
    "UZ-AGT-009",
    "UZ-AGT-010",
    "UZ-AGT-011",
    "UZ-AGT-012",
    "UZ-AGT-013",
    "UZ-AGT-014",
    "UZ-AGT-015",
    "UZ-API-001",
    "UZ-API-002",
    "UZ-APIKEY-001",
    "UZ-APIKEY-003",
    "UZ-APIKEY-004",
    "UZ-APIKEY-005",
    "UZ-APIKEY-006",
    "UZ-APIKEY-007",
    "UZ-APIKEY-008",
    "UZ-APPROVAL-001",
    "UZ-APPROVAL-002",
    "UZ-APPROVAL-003",
    "UZ-APPROVAL-004",
    "UZ-APPROVAL-005",
    "UZ-APPROVAL-006",
    "UZ-AUTH-001",
    "UZ-AUTH-002",
    "UZ-AUTH-003",
    "UZ-AUTH-004",
    "UZ-AUTH-005",
    "UZ-AUTH-006",
    "UZ-AUTH-011",
    "UZ-AUTH-012",
    "UZ-AUTH-013",
    "UZ-AUTH-014",
    "UZ-AUTH-015",
    "UZ-AUTH-016",
    "UZ-AUTH-017",
    "UZ-AUTH-018",
    "UZ-AUTH-019",
    "UZ-AUTH-020",
    "UZ-AUTH-022",
    "UZ-AUTH-023",
    "UZ-AUTH-024",
    "UZ-AUTH-025",
    "UZ-BUNDLE-001",
    "UZ-BUNDLE-002",
    "UZ-BUNDLE-003",
    "UZ-BUNDLE-004",
    "UZ-BUNDLE-005",
    "UZ-CATALOG-001",
    "UZ-CATALOG-002",
    "UZ-CATALOG-003",
    "UZ-CATALOG-004",
    "UZ-CATALOG-005",
    "UZ-CONN-001",
    "UZ-CONN-002",
    "UZ-CONN-003",
    "UZ-CONN-004",
    "UZ-CONN-006",
    "UZ-CONN-007",
    "UZ-CONN-008",
    "UZ-CRED-001",
    "UZ-CRED-002",
    "UZ-EXEC-003",
    "UZ-EXEC-004",
    "UZ-EXEC-005",
    "UZ-EXEC-006",
    "UZ-EXEC-007",
    "UZ-EXEC-008",
    "UZ-EXEC-009",
    "UZ-EXEC-010",
    "UZ-EXEC-011",
    "UZ-EXEC-012",
    "UZ-EXEC-013",
    "UZ-EXEC-014",
    "UZ-EXEC-015",
    "UZ-EXEC-016",
    "UZ-EXEC-017",
    "UZ-GH-001",
    "UZ-GH-002",
    "UZ-GRANT-001",
    "UZ-GRANT-002",
    "UZ-GRANT-003",
    "UZ-INTERNAL-001",
    "UZ-INTERNAL-002",
    "UZ-INTERNAL-003",
    "UZ-LIBRARY-001",
    "UZ-LIBRARY-002",
    "UZ-LIBRARY-003",
    "UZ-LIBRARY-004",
    "UZ-LIBRARY-005",
    "UZ-LIBRARY-006",
    "UZ-LIBRARY-008",
    "UZ-MEM-002",
    "UZ-MEM-003",
    "UZ-MEM-004",
    "UZ-MODELS-001",
    "UZ-MODELS-002",
    "UZ-MODELS-003",
    "UZ-MODELS-004",
    "UZ-PREFS-001",
    "UZ-PREFS-002",
    "UZ-PROVIDER-001",
    "UZ-PROVIDER-002",
    "UZ-PROVIDER-003",
    "UZ-PROVIDER-004",
    "UZ-PROVIDER-005",
    "UZ-PROVIDER-006",
    "UZ-PROVIDER-007",
    "UZ-PROVIDER-008",
    "UZ-PROVIDER-009",
    "UZ-PROVIDER-010",
    "UZ-REPAIR-010",
    "UZ-REPAIR-011",
    "UZ-REPAIR-012",
    "UZ-REPAIR-013",
    "UZ-REPAIR-014",
    "UZ-REQ-001",
    "UZ-REQ-002",
    "UZ-RUN-001",
    "UZ-RUN-005",
    "UZ-RUN-006",
    "UZ-RUN-009",
    "UZ-RUN-010",
    "UZ-RUN-011",
    "UZ-RUN-012",
    "UZ-RUN-013",
    "UZ-RUN-014",
    "UZ-RUN-015",
    "UZ-RUN-016",
    "UZ-RUN-017",
    "UZ-RUN-018",
    "UZ-SCHED-001",
    "UZ-SCHED-002",
    "UZ-SCHED-003",
    "UZ-SCHED-004",
    "UZ-SCHED-005",
    "UZ-SCHED-006",
    "UZ-SCHED-007",
    "UZ-SCHED-008",
    "UZ-SLK-010",
    "UZ-SLK-011",
    "UZ-SLK-020",
    "UZ-SLK-022",
    "UZ-SLK-030",
    "UZ-STARTUP-001",
    "UZ-STARTUP-002",
    "UZ-STARTUP-003",
    "UZ-STARTUP-004",
    "UZ-STARTUP-005",
    "UZ-STARTUP-006",
    "UZ-TOOL-005",
    "UZ-UUIDV7-009",
    "UZ-VAULT-001",
    "UZ-VAULT-002",
    "UZ-VAULT-003",
    "UZ-VAULT-004",
    "UZ-VAULT-005",
    "UZ-WH-001",
    "UZ-WH-002",
    "UZ-WH-010",
    "UZ-WH-011",
    "UZ-WH-020",
    "UZ-WH-021",
    "UZ-WH-022",
    "UZ-WH-030",
    "UZ-WORKSPACE-001",
];

/// Catches a code declared twice under two names, or a typo'd spelling that
/// would reach a client as an unmatched code.
#[test]
fn test_error_registry_unique() {
    let mut seen = BTreeSet::new();
    for code in REGISTRY {
        assert!(
            seen.insert(code.as_str()),
            "code {} is declared more than once",
            code.as_str()
        );
        assert_registry_spelling(*code);
    }
    assert_eq!(
        seen.len(),
        REGISTRY.len(),
        "REGISTRY length disagrees with its distinct codes"
    );
    // Every named constant is reachable from REGISTRY. A code added above the
    // list but not into it is invisible to every check in this file.
    for named in [
        error_code::UUIDV7_INVALID_ID_SHAPE,
        error_code::INVALID_REQUEST,
        error_code::PAYLOAD_TOO_LARGE,
        error_code::VAULT_DATA_INVALID,
        error_code::INTERNAL_OPERATION_FAILED,
        error_code::INTERNAL_DB_UNAVAILABLE,
        error_code::INTERNAL_DB_QUERY,
        error_code::STARTUP_MIGRATION_CHECK,
        error_code::STARTUP_REDIS_CONNECT,
        error_code::AUTH_INSUFFICIENT_SCOPE,
        error_code::AUTH_UNAUTHORIZED,
        error_code::AUTH_TOKEN_EXPIRED,
        error_code::AUTH_UNAVAILABLE,
        error_code::AUTH_CLI_CREDENTIAL_REVOKED,
        error_code::APIKEY_REVOKED,
        error_code::RUN_INVALID_RUNNER_TOKEN,
        error_code::RUN_STALE_FENCING_TOKEN,
        error_code::RUN_LEASE_NOT_FOUND,
        error_code::RUN_ADMIN_STATE_BLOCKED,
        error_code::RUN_LEASE_EXCEEDED_MAX_RUNTIME,
        error_code::RUN_LEASE_LOST,
        error_code::RUN_LEASE_RENEWAL_NO_CREDITS,
        error_code::RUN_BUDGET_EXCEEDED,
        // Was omitted when it was declared, which is the exact gap this list
        // exists to close — a code reachable from no check is a code that can
        // quietly lose its entry.
        error_code::AGENTSFLEET_CREDENTIAL_MISSING,
        error_code::FLEET_BUNDLE_NOT_FOUND,
        error_code::FLEET_BUNDLE_STORAGE_UNAVAILABLE,
        error_code::API_BACKPRESSURE,
    ] {
        assert!(
            REGISTRY.contains(&named),
            "{} is declared but missing from REGISTRY",
            named.as_str()
        );
    }
}

/// The property the grammar exists to guarantee, checked rather than restated:
/// three segments, `UZ`, a non-empty upper-case-alphanumeric family, and
/// exactly three digits.
fn assert_registry_spelling(code: ErrorCode) {
    let text = code.as_str();
    let mut parts = text.split('-');
    assert_eq!(parts.next(), Some("UZ"), "{text} must start with UZ");

    let family = parts.next().unwrap_or_default();
    assert!(!family.is_empty(), "{text} has an empty family");
    assert!(
        family
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()),
        "{text} family must be upper-case alphanumeric"
    );

    let number = parts.next().unwrap_or_default();
    assert_eq!(number.len(), 3, "{text} must end in exactly three digits");
    assert!(
        number.chars().all(|c| c.is_ascii_digit()),
        "{text} must end in exactly three digits"
    );
    assert_eq!(parts.next(), None, "{text} has more than three segments");
}

/// Catches the drift this port is most exposed to: the Rust daemon reporting a
/// code the Zig daemon never emitted, or the same failure under two spellings.
#[test]
fn should_declare_only_codes_the_zig_registry_also_declares() {
    for code in REGISTRY {
        assert!(
            ZIG_DECLARED.contains(&code.as_str()),
            "{} is outside the registry the product shipped; adding a code is a \
             public-contract decision — declare it in ZIG_DECLARED deliberately",
            code.as_str()
        );
    }
}

#[test]
fn should_render_the_code_as_its_wire_string() {
    let code = error_code::INVALID_REQUEST;
    assert_eq!(code.as_str(), "UZ-REQ-001");
    assert_eq!(code.to_string(), "UZ-REQ-001");
    assert_eq!(serde_json::to_string(&code).unwrap(), "\"UZ-REQ-001\"");
}

/// The const asserts only prove the two DECLARED codes are well formed; nothing
/// there exercises the reject branches. Calling `declare` outside a const
/// context runs the same grammar at runtime, so a validator that accepted
/// anything would be caught here rather than shipping unnoticed.
#[test]
fn should_accept_well_formed_codes_at_declaration() {
    for good in [
        "UZ-REQ-001",
        "UZ-UUIDV7-009",
        "UZ-A-000",
        "UZ-INTERNAL-999",
        "UZ-0-123",
    ] {
        assert_eq!(ErrorCode::declare(good).as_str(), good);
    }
}

macro_rules! rejects {
    ($name:ident, $code:literal) => {
        #[test]
        #[should_panic(expected = "UZ-<FAMILY>-<NNN>")]
        fn $name() {
            let _ = ErrorCode::declare($code);
        }
    };
}

rejects!(should_reject_an_empty_code, "");
rejects!(should_reject_a_missing_prefix, "XY-REQ-001");
rejects!(should_reject_a_lowercase_prefix, "uz-REQ-001");
rejects!(should_reject_an_empty_family, "UZ--001");
rejects!(should_reject_a_lowercase_family, "UZ-req-001");
rejects!(should_reject_two_digits, "UZ-REQ-01");
rejects!(should_reject_four_digits, "UZ-REQ-0001");
rejects!(should_reject_a_non_digit_number, "UZ-REQ-00A");
rejects!(should_reject_a_missing_number, "UZ-REQ");
rejects!(should_reject_a_trailing_segment, "UZ-REQ-001-X");
rejects!(
    should_reject_a_family_separator_in_the_family,
    "UZ-RE_Q-001"
);
