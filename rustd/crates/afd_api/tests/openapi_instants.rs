//! Every instant the API publishes is an RFC 3339 string, in the document and
//! on the wire (Dimension 4.7).
//!
//! Two halves, because either can drift alone. A field annotated with the
//! wrong `value_type` publishes an integer while the handler still writes a
//! string; a field that loses its `with = "afd_wire::instant"` writes jiff's
//! default while the document still says `date-time`. The document half reads
//! the generated document in memory, as the contract suite does; the wire half
//! serializes a published response type.
#![expect(
    clippy::expect_used,
    reason = "a document utoipa just built must serialize; a failure here is the
              generator broken, not a state under test"
)]
#![cfg(all(feature = "test-util", feature = "openapi"))]

use afd_api_wire::tenant::BillingResponse;
use jiff::Timestamp;
use serde_json::Value;

use crate::openapi_contract::document;

/// The suffix every published instant's name carries.
const INSTANT_SUFFIX: &str = "_at";

/// The string format an instant is published under.
const DATE_TIME: &str = "date-time";

/// The JSON type an instant is published as.
const STRING: &str = "string";

/// The JSON type a nullable instant adds.
const NULL: &str = "null";

/// A whole-second instant: 2025-10-17T18:26:49Z.
const WHOLE_SECOND: i64 = 1_760_725_609_000;

/// The same second plus 123 milliseconds.
const WITH_MILLIS: i64 = 1_760_725_609_123;

/// Where a schema names `(owner, field)` with the shape it publishes, for
/// every property and parameter whose name ends in [`INSTANT_SUFFIX`].
fn instant_fields(value: &Value, owner: &str, found: &mut Vec<(String, Value)>) {
    match value {
        Value::Object(fields) => {
            if let Some(Value::Object(properties)) = fields.get("properties") {
                for (name, schema) in properties {
                    if name.ends_with(INSTANT_SUFFIX) {
                        found.push((format!("{owner}.{name}"), schema.clone()));
                    }
                }
            }
            if let (Some(Value::String(name)), Some(schema)) =
                (fields.get("name"), fields.get("schema"))
                && name.ends_with(INSTANT_SUFFIX)
            {
                found.push((format!("{owner} parameter {name}"), schema.clone()));
            }
            for (key, nested) in fields {
                instant_fields(nested, &format!("{owner}/{key}"), found);
            }
        }
        Value::Array(items) => {
            for item in items {
                instant_fields(item, owner, found);
            }
        }
        _ => {}
    }
}

/// Whether `schema` publishes a string, or a string-or-null, in `date-time`.
fn is_rfc3339(schema: &Value) -> bool {
    let typed = match schema.get("type") {
        Some(Value::String(kind)) => kind == STRING,
        Some(Value::Array(kinds)) => {
            kinds.iter().any(|kind| kind == STRING)
                && kinds.iter().all(|kind| kind == STRING || kind == NULL)
        }
        _ => false,
    };
    typed && schema.get("format").and_then(Value::as_str) == Some(DATE_TIME)
}

/// The instant `millis` after the epoch.
fn at(millis: i64) -> Timestamp {
    Timestamp::from_millisecond(millis).expect("a fixture instant is in range")
}

#[test]
fn test_api_instants_are_rfc3339() {
    let mut found = Vec::new();
    instant_fields(&document(), "#", &mut found);
    assert!(
        found.len() > 1,
        "the document publishes no instant at all; this gate would pass against an empty document"
    );
    let wrong: Vec<String> = found
        .iter()
        .filter(|(_, schema)| !is_rfc3339(schema))
        .map(|(field, schema)| format!("{field}: {schema}"))
        .collect();
    assert!(
        wrong.is_empty(),
        "an instant is published as something other than `type: string, format: date-time` \
         ({} of {}):\n  {}",
        wrong.len(),
        found.len(),
        wrong.join("\n  "),
    );

    let written = serde_json::to_value(BillingResponse {
        balance_nanos: 0,
        updated_at: at(WHOLE_SECOND),
        is_exhausted: true,
        exhausted_at: Some(at(WITH_MILLIS)),
    })
    .expect("a wire response serializes");
    assert_eq!(
        written.get("updated_at"),
        Some(&Value::from("2025-10-17T18:26:49Z")),
        "a whole-second instant carries no fraction"
    );
    assert_eq!(
        written.get("exhausted_at"),
        Some(&Value::from("2025-10-17T18:26:49.123Z")),
        "a millisecond instant carries its milliseconds, in UTC"
    );
}
