//! The reason-copy rule at each of its three caps.

use garde::Validate as _;
use serde_json::{Map, Value};

use super::{
    AdminLibraryPatch, REASON_CREDENTIAL_MAX_BYTES, REASON_MAX_BYTES, REASONS_MAX,
    REASONS_OUT_OF_BOUNDS,
};

/// A patch carrying `count` reasons, each filed under a distinct name of
/// `name_bytes` bytes, each reason `reason_bytes` long.
fn patch(count: usize, name_bytes: usize, reason_bytes: usize) -> AdminLibraryPatch<'static> {
    let reasons: Map<String, Value> = (0..count)
        .map(|index| {
            let name = format!("{index:0>name_bytes$}");
            (name, Value::String("r".repeat(reason_bytes)))
        })
        .collect();
    AdminLibraryPatch {
        required_credentials_reasons: Some(Value::Object(reasons)),
        ..AdminLibraryPatch::default()
    }
}

/// The messages a patch's report carries, empty when it passed.
fn reported(value: &AdminLibraryPatch<'_>) -> Vec<String> {
    value.validate().map_or_else(
        |report| {
            report
                .iter()
                .map(|(_path, error)| error.message().to_owned())
                .collect()
        },
        |()| Vec::new(),
    )
}

#[test]
fn test_library_reasons_are_bounded() {
    let refused = vec![REASONS_OUT_OF_BOUNDS.to_owned()];
    // Thirty-three entries: one past the most a library may explain.
    assert_eq!(reported(&patch(REASONS_MAX + 1, 8, 8)), refused);
    // A 501-byte reason, and a name one past its own cap.
    assert_eq!(reported(&patch(1, 8, REASON_MAX_BYTES + 1)), refused);
    assert_eq!(
        reported(&patch(1, REASON_CREDENTIAL_MAX_BYTES + 1, 8)),
        refused
    );
    // Thirty-two reasons of five hundred bytes each: every cap met exactly.
    assert_eq!(
        reported(&patch(
            REASONS_MAX,
            REASON_CREDENTIAL_MAX_BYTES,
            REASON_MAX_BYTES
        )),
        Vec::<String>::new()
    );
}

#[test]
fn a_value_that_is_not_an_object_of_strings_is_left_to_the_route() {
    // Shape is the route's sentence, not this bound's: the rule passes what
    // it cannot measure, and the route refuses it as the wrong shape.
    for shape in [
        Value::Array(Vec::new()),
        serde_json::json!({ "github": 42 }),
    ] {
        let value = AdminLibraryPatch {
            required_credentials_reasons: Some(shape),
            ..AdminLibraryPatch::default()
        };
        assert_eq!(reported(&value), Vec::<String>::new());
    }
}
