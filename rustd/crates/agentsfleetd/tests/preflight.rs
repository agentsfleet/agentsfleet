//! Dimensions 8.1 and 7.4 — boot refuses loudly, and names everything at once.
//!
//! Driven through [`MapEnv`] rather than the process environment, because
//! `std::env::set_var` is `unsafe` in edition 2024 for a reason that bites
//! here: these tests run in parallel threads of one process, and a suite that
//! mutated shared environment state would be racing itself.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::env::MapEnv;
use agentsfleetd::preflight::{ENCRYPTION_MASTER_KEY_KNOB, Fault, preflight};

/// The API role's Postgres knob — the name an operator actually exports.
const DATABASE_KNOB: &str = "DATABASE_URL_API";

/// The API role's Redis knob.
const REDIS_KNOB: &str = "REDIS_URL_API";

/// A Postgres URL the resolver accepts.
const GOOD_DATABASE: &str = "postgres://afd:afd@127.0.0.1:5432/agentsfleet";

/// A Redis URL the resolver accepts.
const GOOD_REDIS: &str = "redis://127.0.0.1:6379";

/// Sixty-four hex characters: exactly one 32-byte key.
const GOOD_KEK: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// An environment with every knob this daemon refuses to boot without.
fn complete() -> MapEnv {
    MapEnv::from_pairs([
        (DATABASE_KNOB, GOOD_DATABASE),
        (REDIS_KNOB, GOOD_REDIS),
        (ENCRYPTION_MASTER_KEY_KNOB, GOOD_KEK),
    ])
}

/// Dimension 8.1 — three unset knobs are named in ONE failure, not the first.
///
/// This is the dimension's whole point. The Zig boot exits at the first check,
/// so an operator missing three knobs restarts three times to find that out;
/// the assertion below is that one run reports all three.
#[test]
fn test_preflight_lists_missing() {
    let refusal = preflight(&MapEnv::default()).expect_err("an empty environment cannot boot");

    let mut knobs = refusal.knobs();
    knobs.sort_unstable();
    let mut expected = vec![DATABASE_KNOB, REDIS_KNOB, ENCRYPTION_MASTER_KEY_KNOB];
    expected.sort_unstable();
    assert_eq!(
        knobs, expected,
        "every unset knob must be named in one refusal, not just the first"
    );

    assert!(
        refusal
            .faults()
            .iter()
            .all(|fault| matches!(*fault, Fault::Missing { .. })),
        "an unset knob is missing, not invalid: {refusal:?}"
    );

    // The rendered message is what the operator actually reads, so the names
    // have to survive into it — a report that knows three knobs and prints one
    // would pass every assertion above.
    let rendered = refusal.to_string();
    for knob in expected {
        assert!(
            rendered.contains(knob),
            "the refusal message must name {knob}, got: {rendered}"
        );
    }
}

/// A blank value is treated as unset, not as a value that fails later.
#[test]
fn test_preflight_treats_a_blank_value_as_unset() {
    let env = MapEnv::from_pairs([
        (DATABASE_KNOB, GOOD_DATABASE),
        (REDIS_KNOB, GOOD_REDIS),
        (ENCRYPTION_MASTER_KEY_KNOB, "   "),
    ]);

    let refusal = preflight(&env).expect_err("a blank master key cannot boot");

    assert_eq!(
        refusal.knobs(),
        vec![ENCRYPTION_MASTER_KEY_KNOB],
        "only the blank knob is at fault"
    );
    assert!(
        matches!(refusal.faults().first(), Some(&Fault::Missing { .. })),
        "an operator who exported an empty string meant to supply a value: {refusal:?}"
    );
}

/// Dimension 7.4 — a malformed master key is INVALID, and says so.
///
/// The distinction from "missing" is the operator's next action: supply a
/// value, versus correct the one you wrote. A refusal that collapsed them
/// would send someone looking for an unset variable that is plainly set.
#[test]
fn test_boot_refuses_bad_kek() {
    for bad in ["abcd", GOOD_KEK.replace('0', "zz").as_str()] {
        let env = MapEnv::from_pairs([
            (DATABASE_KNOB, GOOD_DATABASE),
            (REDIS_KNOB, GOOD_REDIS),
            (ENCRYPTION_MASTER_KEY_KNOB, bad),
        ]);

        let refusal = preflight(&env).expect_err("a malformed master key cannot boot");

        assert_eq!(
            refusal.knobs(),
            vec![ENCRYPTION_MASTER_KEY_KNOB],
            "a bad key is one fault, and it is this one"
        );
        assert!(
            matches!(refusal.faults().first(), Some(&Fault::Invalid { .. })),
            "a key that is set but unusable is invalid, not missing: {refusal:?}"
        );
        assert!(
            refusal.to_string().contains(ENCRYPTION_MASTER_KEY_KNOB),
            "the message must name the knob the operator has to fix"
        );
    }
}

/// A knob that is set to something unusable is invalid, for every knob.
///
/// Proves `classify` is reached on the set-but-wrong path for the datastores
/// too, not only for the key — the two use the same helper and would otherwise
/// have only one of their branches exercised.
#[test]
fn test_preflight_separates_unusable_from_unset() {
    let env = MapEnv::from_pairs([
        (DATABASE_KNOB, "mysql://not-postgres/afd"),
        (REDIS_KNOB, GOOD_REDIS),
        (ENCRYPTION_MASTER_KEY_KNOB, GOOD_KEK),
    ]);

    let refusal = preflight(&env).expect_err("a non-Postgres URL cannot boot the API role");

    assert_eq!(refusal.knobs(), vec![DATABASE_KNOB]);
    assert!(
        matches!(refusal.faults().first(), Some(&Fault::Invalid { .. })),
        "a URL that is set and wrong is invalid: {refusal:?}"
    );
    assert_eq!(
        refusal.faults().first().map(Fault::knob),
        Some(DATABASE_KNOB),
        "the fault knows which knob it is about"
    );
}

/// A complete environment resolves, and hands back what boot will open.
#[test]
fn test_preflight_resolves_a_complete_environment() {
    let config = preflight(&complete()).expect("a complete environment boots");

    assert_eq!(
        config.api_pool().role(),
        afd_db::config::DbRole::Api,
        "preflight resolves the API role, not the default one"
    );
    assert_eq!(
        config.redis().role(),
        afd_redis::config::RedisRole::Api,
        "preflight resolves the API Redis role"
    );
    // The KEK is redacted by construction, so the assertion is that it EXISTS
    // and does not print itself — checking the bytes would be checking
    // afd_crypto's job from the wrong crate.
    assert!(
        !format!("{:?}", config.kek()).contains(GOOD_KEK),
        "a resolved master key must not render its own material"
    );
}
