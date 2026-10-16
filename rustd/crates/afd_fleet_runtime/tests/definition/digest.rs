//! Content identity is stable across representation changes, sensitive to setup.

use afd_fleet_runtime::definition::canonical_digest;
use serde_json::json;

use super::{parse, set, value};

/// The context threshold whose integer and float spellings must digest alike.
const STAGE_CHUNK_THRESHOLD: &str = "/config/x-agentsfleet/context/stage_chunk_threshold";

#[test]
fn test_definition_digest_sorts_objects_and_normalizes_numbers() {
    let definition = parse(&value());
    let mut input = value();
    set(
        &mut input,
        "/config/x-agentsfleet/budget",
        json!({"monthly_nanos":"10000000001","daily_nanos":"0001000000001"}),
    );
    let reversed = parse(&input);
    assert_eq!(
        canonical_digest(&definition).expect("digest"),
        canonical_digest(&reversed).expect("digest")
    );
    set(&mut input, STAGE_CHUNK_THRESHOLD, json!(1.0));
    let float = parse(&input);
    set(&mut input, STAGE_CHUNK_THRESHOLD, json!(1));
    assert_eq!(
        canonical_digest(&float).expect("digest"),
        canonical_digest(&parse(&input)).expect("digest")
    );
}

#[test]
fn test_definition_digest_changes_with_text_policy_or_array_order() {
    let input = value();
    let original = canonical_digest(&parse(&input)).expect("digest");
    for pointer in [
        "/description",
        "/instructions_md",
        "/config/x-agentsfleet/model",
        "/config/x-agentsfleet/budget/daily_nanos",
    ] {
        let mut changed = input.clone();
        *changed.pointer_mut(pointer).expect("field") = if pointer.ends_with("daily_nanos") {
            json!("1000000002")
        } else {
            json!("changed")
        };
        assert_ne!(
            canonical_digest(&parse(&changed)).expect("digest"),
            original
        );
    }
    let mut reordered = input.clone();
    set(
        &mut reordered,
        "/config/x-agentsfleet/tools",
        json!(["shell", "http_request"]),
    );
    assert_ne!(
        canonical_digest(&parse(&reordered)).expect("digest"),
        original
    );
}

/// A description's line endings do not change content identity: it is stored
/// with one line ending, like instructions.
#[test]
fn test_definition_digest_ignores_description_line_endings() {
    let mut windows = value();
    set(
        &mut windows,
        "/description",
        json!("Watch checkout.\r\nExplain failures."),
    );
    let mut unix = value();
    set(
        &mut unix,
        "/description",
        json!("Watch checkout.\nExplain failures."),
    );
    assert_eq!(
        canonical_digest(&parse(&windows)).expect("digest"),
        canonical_digest(&parse(&unix)).expect("digest")
    );
}
