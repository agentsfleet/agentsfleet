//! The parity proof: names, attribution, and the properties each event carries.
//!
//! Every assertion here is against the Zig `telemetry_events.zig` this ports.
//! The bytes matter more than usual — a funnel on the other end matches on the
//! event name and groups by the property keys, so a rename that compiles
//! silently splits a dashboard in two.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use posthog_rs::Event;
use serde_json::Value;

use super::Telemetry;

/// The person a fixture event is attributed to.
const ACTOR: &str = "user_2telemetry";

/// The workspace a fixture event happens in.
const WORKSPACE: &str = "01924f4e-0000-7000-8000-000000000001";

/// The fleet a fixture run belongs to.
const FLEET: &str = "01924f4e-0000-7000-8000-00000000fee7";

/// The stream entry a fixture run was leased from.
const ENTRY: &str = "1700000000000-0";

/// The request a fixture event was answered on.
const REQUEST: &str = "req_2telemetry";

/// One event's properties, by key.
fn properties(telemetry: &Telemetry) -> std::collections::HashMap<String, Value> {
    telemetry.event().properties().clone()
}

/// One property, as text.
fn text(event: &Event, key: &str) -> String {
    let carried = event.properties().get(key).and_then(Value::as_str);
    carried
        .expect("the event carries the property this case reads")
        .to_owned()
}

/// Every event carries the name the analytics on the other end matches on.
#[test]
fn should_report_each_event_under_its_published_name() {
    let named = [
        (
            "entitlement_rejected",
            Telemetry::EntitlementRejected {
                actor: ACTOR.to_owned(),
                workspace_id: WORKSPACE.to_owned(),
                boundary: "fleet_count".to_owned(),
                reason_code: "plan_limit".to_owned(),
                request_id: REQUEST.to_owned(),
            },
        ),
        ("server_started", Telemetry::ServerStarted { port: 8080 }),
        (
            "worker_started",
            Telemetry::WorkerStarted { concurrency: 4 },
        ),
        (
            "startup_failed",
            Telemetry::StartupFailed {
                command: "serve".to_owned(),
                phase: "preflight".to_owned(),
                reason: "DATABASE_URL is not set".to_owned(),
                error_code: "UZ-STARTUP-005".to_owned(),
            },
        ),
        (
            "api_error",
            Telemetry::ApiError {
                actor: ACTOR.to_owned(),
                error_code: "UZ-REQ-001".to_owned(),
                message: "limit must be between 1 and 25".to_owned(),
                workspace_id: None,
                request_id: REQUEST.to_owned(),
            },
        ),
        (
            "workspace_created",
            Telemetry::WorkspaceCreated {
                actor: ACTOR.to_owned(),
                workspace_id: WORKSPACE.to_owned(),
                tenant_id: "tenant".to_owned(),
                request_id: REQUEST.to_owned(),
            },
        ),
        (
            "auth_login_completed",
            Telemetry::AuthLoginCompleted {
                actor: ACTOR.to_owned(),
                session_id: "sess".to_owned(),
                request_id: REQUEST.to_owned(),
            },
        ),
        (
            "auth_rejected",
            Telemetry::AuthRejected {
                reason: "expired".to_owned(),
                request_id: REQUEST.to_owned(),
            },
        ),
        (
            "fleet_triggered",
            Telemetry::FleetTriggered {
                actor: ACTOR.to_owned(),
                workspace_id: WORKSPACE.to_owned(),
                fleet_id: FLEET.to_owned(),
                event_id: ENTRY.to_owned(),
                source: "steer".to_owned(),
            },
        ),
        ("fleet_completed", completed()),
        (
            "signup_bootstrapped",
            Telemetry::SignupBootstrapped {
                actor: ACTOR.to_owned(),
                tenant_id: "tenant".to_owned(),
                workspace_id: WORKSPACE.to_owned(),
                workspace_name: "Personal".to_owned(),
                email_domain: "example.com".to_owned(),
                created: true,
                request_id: REQUEST.to_owned(),
            },
        ),
    ];
    assert_eq!(named.len(), 11, "the ported event set is eleven events");
    for (name, telemetry) in named {
        assert_eq!(telemetry.name(), name);
        assert_eq!(telemetry.event().event_name(), name);
    }
}

/// A finished run, as a fixture.
fn completed() -> Telemetry {
    Telemetry::FleetCompleted {
        actor: ACTOR.to_owned(),
        workspace_id: WORKSPACE.to_owned(),
        fleet_id: FLEET.to_owned(),
        event_id: ENTRY.to_owned(),
        tokens: 1_200,
        wall_ms: 4_500,
        exit_status: "succeeded".to_owned(),
        time_to_first_token_ms: 320,
    }
}

/// An instance-level event is attributed to nobody.
///
/// A server that came up did not come up FOR anyone, and giving it a person
/// would put a machine's lifecycle inside that person's funnel.
#[test]
fn should_attribute_instance_events_to_nobody() {
    for telemetry in [
        Telemetry::ServerStarted { port: 8080 },
        Telemetry::WorkerStarted { concurrency: 4 },
        Telemetry::AuthRejected {
            reason: "expired".to_owned(),
            request_id: REQUEST.to_owned(),
        },
    ] {
        assert_eq!(telemetry.actor(), None, "{}", telemetry.name());
    }
}

/// A person's event names them, both as the attribution and where the Zig
/// also wrote them as a property.
#[test]
fn should_attribute_a_persons_event_to_that_person() {
    let login = Telemetry::AuthLoginCompleted {
        actor: ACTOR.to_owned(),
        session_id: "sess".to_owned(),
        request_id: REQUEST.to_owned(),
    };
    assert_eq!(login.actor(), Some(ACTOR));
    let event = login.event();
    assert_eq!(event.distinct_id(), ACTOR);
    assert_eq!(
        text(&event, "distinct_id"),
        ACTOR,
        "the Zig writes it as a property too, and a dashboard groups by it"
    );
}

/// A refusal with no workspace OMITS the key rather than sending null.
///
/// The two Zig structs `ApiError` and `ApiErrorWithContext` are this one
/// variant, and the difference between them is exactly this key's presence. A
/// `null` would make every pre-workspace refusal a cohort under one filter.
#[test]
fn should_omit_the_workspace_when_a_refusal_happened_before_one_was_known() {
    let anonymous = Telemetry::ApiError {
        actor: ACTOR.to_owned(),
        error_code: "UZ-REQ-001".to_owned(),
        message: "malformed".to_owned(),
        workspace_id: None,
        request_id: REQUEST.to_owned(),
    };
    let carried = properties(&anonymous);
    assert!(!carried.contains_key("workspace_id"));
    assert_eq!(carried.len(), 3, "code, message, request");

    let contextual = Telemetry::ApiError {
        actor: ACTOR.to_owned(),
        error_code: "UZ-REQ-001".to_owned(),
        message: "malformed".to_owned(),
        workspace_id: Some(WORKSPACE.to_owned()),
        request_id: REQUEST.to_owned(),
    };
    let carried = properties(&contextual);
    assert_eq!(
        carried.get("workspace_id").and_then(Value::as_str),
        Some(WORKSPACE)
    );
    assert_eq!(carried.len(), 4);
}

/// A finished run carries its counters as numbers, not as text.
#[test]
fn should_report_a_runs_counters_as_numbers() {
    let carried = properties(&completed());
    assert_eq!(carried.get("tokens").and_then(Value::as_u64), Some(1_200));
    assert_eq!(carried.get("wall_ms").and_then(Value::as_u64), Some(4_500));
    assert_eq!(
        carried
            .get("time_to_first_token_ms")
            .and_then(Value::as_u64),
        Some(320)
    );
    assert_eq!(
        carried.get("exit_status").and_then(Value::as_str),
        Some("succeeded")
    );
}

/// The deduplication key is derived from the run, so a retry is dropped.
///
/// The property `PostHog` itself reads. Deriving it from the fleet and the event
/// means a report sent twice — a network failure, a redelivered webhook — is
/// discarded by ingestion rather than doubling the run count on every chart.
#[test]
fn should_derive_one_deduplication_key_per_run() {
    let first = properties(&completed());
    let again = properties(&completed());
    let key = first
        .get("$insert_id")
        .and_then(Value::as_str)
        .expect("a finished run carries a deduplication key")
        .to_owned();
    assert_eq!(
        again.get("$insert_id").and_then(Value::as_str),
        Some(key.as_str()),
        "the same run reports the same key however many times it is sent"
    );
    assert_eq!(key.len(), 64, "a SHA-256 digest, hex");
    assert!(key.chars().all(|digit| digit.is_ascii_hexdigit()));
}

/// Two different runs never share a deduplication key.
///
/// Including the pair that a separator-less hash would collide: `("ab", "c")`
/// and `("a", "bc")` concatenate to the same bytes, and ingestion would then
/// drop the second run as a duplicate of the first.
#[test]
fn should_never_give_two_runs_the_same_deduplication_key() {
    let key_of = |fleet: &str, entry: &str| {
        let run = Telemetry::FleetCompleted {
            actor: ACTOR.to_owned(),
            workspace_id: WORKSPACE.to_owned(),
            fleet_id: fleet.to_owned(),
            event_id: entry.to_owned(),
            tokens: 0,
            wall_ms: 0,
            exit_status: "succeeded".to_owned(),
            time_to_first_token_ms: 0,
        };
        properties(&run)
            .get("$insert_id")
            .and_then(Value::as_str)
            .expect("a finished run carries a deduplication key")
            .to_owned()
    };
    assert_ne!(key_of(FLEET, ENTRY), key_of(FLEET, "1700000000001-0"));
    assert_ne!(key_of(FLEET, ENTRY), key_of("other-fleet", ENTRY));
    assert_ne!(key_of("ab", "c"), key_of("a", "bc"));
}
