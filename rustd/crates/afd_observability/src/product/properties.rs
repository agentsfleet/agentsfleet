//! What each event carries, key by key.
//!
//! Split from the enum beside it for length, and the cut is the natural one:
//! `telemetry.rs` answers what the events ARE, and this answers what each one
//! says. Both halves are the same contract with the analytics on the other end,
//! so the property keys live here with the arms that write them rather than
//! being named in one file and used in another.

use posthog_rs::Event;
use sha2::{Digest as _, Sha256};

use super::Telemetry;

/// The workspace an event happened in.
const KEY_WORKSPACE_ID: &str = "workspace_id";

/// The tenant that workspace belongs to.
const KEY_TENANT_ID: &str = "tenant_id";

/// The fleet an event is about.
const KEY_FLEET_ID: &str = "fleet_id";

/// The stream entry the run was leased from.
const KEY_EVENT_ID: &str = "event_id";

/// The request this daemon answered, for joining a log line to a funnel step.
const KEY_REQUEST_ID: &str = "request_id";

/// Why something was refused, in this daemon's own words.
const KEY_REASON: &str = "reason";

/// The registry code the refusal carried.
const KEY_ERROR_CODE: &str = "error_code";

/// The operator-readable sentence beside that code.
const KEY_MESSAGE: &str = "message";

/// `PostHog`'s own deduplication key.
///
/// Not ours to rename: ingestion drops a second event carrying an `$insert_id`
/// it has already seen, which is what makes a retried report harmless.
const KEY_INSERT_ID: &str = "$insert_id";

/// The byte separating the two halves of a run's deduplication key.
///
/// A NUL, so a fleet and event id pair cannot be re-cut anywhere else — without
/// it, `("ab", "c")` and `("a", "bc")` would hash alike.
const HASH_SEPARATOR: &[u8] = &[0];

impl Telemetry {
    /// Every property this event carries, in the order the Zig writes them.
    #[expect(
        clippy::too_many_lines,
        reason = "one arm per event, each a flat list of keys: splitting it would put half an event's contract in another file, which is exactly the drift the property-key constants above exist to prevent"
    )]
    pub(super) fn describe(&self, event: &mut Event) {
        // `insert_prop` fails only on a value that will not serialize, and
        // every value below is a string, an integer or a bool. Dropping the
        // result rather than raising keeps a reporting call infallible for the
        // request path that makes it.
        let mut put = |key: &'static str, value: serde_json::Value| {
            let _unserializable = event.insert_prop(key, value);
        };
        match self {
            Self::EntitlementRejected {
                workspace_id,
                boundary,
                reason_code,
                request_id,
                ..
            } => {
                put(KEY_WORKSPACE_ID, workspace_id.as_str().into());
                put("boundary", boundary.as_str().into());
                put("reason_code", reason_code.as_str().into());
                put(KEY_REQUEST_ID, request_id.as_str().into());
            }
            Self::ServerStarted { port } => put("port", (*port).into()),
            Self::WorkerStarted { concurrency } => put("concurrency", (*concurrency).into()),
            Self::StartupFailed {
                command,
                phase,
                reason,
                error_code,
            } => {
                put("command", command.as_str().into());
                put("phase", phase.as_str().into());
                put(KEY_REASON, reason.as_str().into());
                put(KEY_ERROR_CODE, error_code.as_str().into());
            }
            Self::ApiError {
                error_code,
                message,
                workspace_id,
                request_id,
                ..
            } => {
                put(KEY_ERROR_CODE, error_code.as_str().into());
                put(KEY_MESSAGE, message.as_str().into());
                // Omitted rather than sent null when the refusal happened
                // before a workspace was resolved: a `null` property makes a
                // dashboard filter on `workspace_id` count these as a cohort.
                if let Some(workspace_id) = workspace_id.as_deref() {
                    put(KEY_WORKSPACE_ID, workspace_id.into());
                }
                put(KEY_REQUEST_ID, request_id.as_str().into());
            }
            Self::WorkspaceCreated {
                workspace_id,
                tenant_id,
                request_id,
                ..
            } => {
                put(KEY_WORKSPACE_ID, workspace_id.as_str().into());
                put(KEY_TENANT_ID, tenant_id.as_str().into());
                put(KEY_REQUEST_ID, request_id.as_str().into());
            }
            Self::AuthLoginCompleted {
                actor,
                session_id,
                request_id,
            } => {
                put("session_id", session_id.as_str().into());
                put(KEY_REQUEST_ID, request_id.as_str().into());
                // Also a property, not only the attribution: the Zig writes it
                // both ways, and a dashboard that groups by it reads the
                // property rather than the person.
                put("distinct_id", actor.as_str().into());
            }
            Self::AuthRejected { reason, request_id } => {
                put(KEY_REASON, reason.as_str().into());
                put(KEY_REQUEST_ID, request_id.as_str().into());
            }
            Self::FleetTriggered {
                workspace_id,
                fleet_id,
                event_id,
                source,
                ..
            } => {
                put(KEY_WORKSPACE_ID, workspace_id.as_str().into());
                put(KEY_FLEET_ID, fleet_id.as_str().into());
                put(KEY_EVENT_ID, event_id.as_str().into());
                put("source", source.as_str().into());
            }
            Self::FleetCompleted {
                workspace_id,
                fleet_id,
                event_id,
                tokens,
                wall_ms,
                exit_status,
                time_to_first_token_ms,
                ..
            } => {
                put(KEY_WORKSPACE_ID, workspace_id.as_str().into());
                put(KEY_FLEET_ID, fleet_id.as_str().into());
                put(KEY_EVENT_ID, event_id.as_str().into());
                put("tokens", (*tokens).into());
                put("wall_ms", (*wall_ms).into());
                put("exit_status", exit_status.as_str().into());
                put("time_to_first_token_ms", (*time_to_first_token_ms).into());
                put(KEY_INSERT_ID, insert_id(fleet_id, event_id).into());
            }
            Self::SignupBootstrapped {
                tenant_id,
                workspace_id,
                workspace_name,
                email_domain,
                created,
                request_id,
                ..
            } => {
                put(KEY_TENANT_ID, tenant_id.as_str().into());
                put(KEY_WORKSPACE_ID, workspace_id.as_str().into());
                put("workspace_name", workspace_name.as_str().into());
                put("email_domain", email_domain.as_str().into());
                put("created", (*created).into());
                put(KEY_REQUEST_ID, request_id.as_str().into());
            }
        }
    }
}

/// The deduplication key a finished run reports under.
///
/// Derived from the run rather than random, so a report retried after a network
/// failure is dropped by ingestion instead of doubling the run count. The pair
/// is what identifies a run: one fleet's event ids are unique to it, and the
/// fleet is what makes the key unique across the deployment.
fn insert_id(fleet_id: &str, event_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(fleet_id.as_bytes());
    hasher.update(HASH_SEPARATOR);
    hasher.update(event_id.as_bytes());
    hex::encode(hasher.finalize())
}
