//! The registry is a checked subset of the Zig one, not a second source of truth.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::collections::BTreeSet;
use std::path::PathBuf;

use afd_core::error_code::{self, ErrorCode, REGISTRY};

/// The Zig file that is the registry of record for the whole product.
const ZIG_REGISTRY: &str = "src/agentsfleetd/errors/error_registry.zig";

fn repo_root() -> PathBuf {
    // <repo>/rustd/crates/afd_core -> <repo>
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_path_buf()
}

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
/// code the Zig daemon never emits, or the same failure under two spellings.
/// Zig stays the registry of record; this asserts the subset relation holds.
#[test]
fn should_declare_only_codes_the_zig_registry_also_declares() {
    let path = repo_root().join(ZIG_REGISTRY);
    let zig = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));

    for code in REGISTRY {
        let declaration = format!("\"{}\"", code.as_str());
        assert!(
            zig.contains(&declaration),
            "{} is not declared in {ZIG_REGISTRY}; Zig is the registry of record",
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
