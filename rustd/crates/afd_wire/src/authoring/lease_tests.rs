#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use super::{ContextMessage, ContextRole, Lease, ModelAccess};
use crate::redact::REDACTED;
use serde_json::{Value, json};

/// The capability token a lease carries; never printed.
const TOKEN: &str = "capability-token-never-printed";
/// The private conversation text a lease carries; never printed.
const TEXT: &str = "Investigate the private checkout";
/// The attempt's end, five minutes after the epoch.
const ATTEMPT_ENDS: &str = "1970-01-01T00:05:00Z";
/// The lease and its model credential expire together, thirty seconds in.
const EXPIRES: &str = "1970-01-01T00:00:30Z";
/// The key both the lease and its model credential carry their expiry under.
const EXPIRES_AT: &str = "expires_at";
/// A spooled report's stable identity.
const REPORT_ID: &str = "01890a5d-ac96-774b-bcce-b302099a8062";
/// The key naming the model, on the lease and inside its model block.
const MODEL: &str = "model";

fn wire() -> Value {
    json!({
        "lease_id": "lease-1", "attempt_id": "attempt-1", "draft_id": "draft-1",
        "sequence": "1", "fence": "1",
        EXPIRES_AT: EXPIRES, "deadline": ATTEMPT_ENDS,
        MODEL: {"ref":"owned", "provider":"openai", MODEL:"gpt-4.1",
            "tokenizer":"o200k_base", "pricing":{"input_nanos_per_million":"1",
                "output_nanos_per_million":"1", "runtime_nanos_per_second":"1"}},
        "model_access":{"token": TOKEN, EXPIRES_AT: EXPIRES,
            "invocation_path":"/v1/runners/me/authoring-leases/lease-1/model-calls"},
        "messages":[{"role":"user","text": TEXT}],
        "limits":{"input_per_call":1,"output_per_turn":1,"max_calls":1,
            "wall_seconds":300,"provider_timeout_seconds":30}
    })
}

fn lease() -> Lease<Value> {
    serde_json::from_value(wire()).expect("a well-formed authoring lease")
}

#[test]
fn the_attempt_deadline_is_an_rfc3339_instant_and_epoch_milliseconds_are_refused() {
    let lease = lease();
    assert_eq!(
        lease.deadline,
        jiff::Timestamp::from_second(300).expect("in range")
    );
    let written = serde_json::to_value(&lease).expect("a lease writes");
    assert_eq!(written.get("deadline"), Some(&json!(ATTEMPT_ENDS)));
    let mut millis = wire();
    let deadline = millis
        .get_mut("deadline")
        .expect("the fixture names a deadline");
    *deadline = json!(300_000);
    serde_json::from_value::<Lease<Value>>(millis).expect_err("epoch milliseconds");
}

#[test]
fn a_lease_prints_its_identity_and_never_its_capability_or_conversation() {
    let lease = lease();
    let printed = format!("{lease:?}");
    assert!(
        printed.contains("lease-1") && printed.contains("attempt-1"),
        "{printed}"
    );
    assert!(
        !printed.contains(TOKEN) && !printed.contains(TEXT),
        "{printed}"
    );
    let access: &ModelAccess = &lease.model_access;
    let printed = format!("{access:?}");
    assert!(
        printed.contains(REDACTED) && !printed.contains(TOKEN),
        "{printed}"
    );
    let message = ContextMessage {
        role: ContextRole::Assistant,
        text: TEXT.to_owned(),
    };
    let printed = format!("{message:?}");
    assert!(
        printed.contains("Assistant") && printed.contains(REDACTED),
        "{printed}"
    );
    assert!(!printed.contains(TEXT), "{printed}");
}

/// The runner protocol no longer carries a deletion generation (spec
/// Dimension 4.5): a lease decodes without one and never writes one, and a
/// renewal that still sends one is refused rather than silently accepted.
#[test]
fn test_runner_wire_drops_deletion_generation() {
    let written = serde_json::to_value(lease()).expect("a lease writes");
    assert!(written.get("deletion_generation").is_none(), "{written}");
    let usage = json!({"knowledge": "known", "calls": 0, "runtime_ms": 0});
    serde_json::from_value::<crate::authoring::Renew>(json!({"fence": "1", "usage": usage}))
        .expect("a renewal without a generation decodes");
    serde_json::from_value::<crate::authoring::Renew>(
        json!({"fence": "1", "deletion_generation": "0", "usage": usage}),
    )
    .expect_err("a renewal still carrying a generation is refused");

    // The accounting-only report a runner replays from its spool after
    // private-content expiry: decodes without a generation, refused with one.
    let report = json!({"kind": "accounting", "fence": "1", "report_id": REPORT_ID,
        "outcome": "completed", "failure_reason": null, "usage": usage});
    let decoded = serde_json::from_value::<crate::authoring::Report>(report.clone())
        .expect("a spooled accounting report without a generation decodes");
    assert!(matches!(
        decoded,
        crate::authoring::Report::Accounting { .. }
    ));
    assert_eq!(decoded.accounting().report_id, REPORT_ID);
    assert_eq!(decoded.accounting().fence, "1");
    let written = serde_json::to_value(&decoded).expect("a report writes");
    assert_eq!(written, report, "nothing added on the way back out");
    let mut stale = report;
    stale
        .as_object_mut()
        .expect("the fixture is an object")
        .insert("deletion_generation".to_owned(), json!("0"));
    serde_json::from_value::<crate::authoring::Report>(stale)
        .expect_err("a report still carrying a generation is refused");
}
