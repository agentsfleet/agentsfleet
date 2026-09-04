//! Every code a client can receive carries the status and prose it answers with.
//!
//! The Zig entries are the source of truth, as the codes themselves are. What
//! this file proves is that the table here is TOTAL over the registry and
//! byte-identical to that source — so §5's `application/problem+json` envelope
//! can be assembled from it without a second lookup that could disagree.
#![expect(
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::collections::BTreeSet;

use afd_core::error_code::{self, REGISTRY};
use afd_core::problem::{DOCS_BASE, Problem, entries};

/// One code's status, title and dashboard sentence, as the Zig entries paired
/// them.
struct ZigEntry {
    code: &'static str,
    /// The `std.http.Status` spelling, kept rather than the number: it is what
    /// the Zig source said, and translating on the way in would hide a
    /// mistranslation inside the expectation.
    status: &'static str,
    title: &'static str,
    /// The sentence `eu()` authored for a dashboard, where the entry had one.
    /// `None` records an `e()` entry, which is itself a fact worth pinning.
    user_message: Option<&'static str>,
}

/// Every entry `error_entries.zig` and `error_entries_runtime.zig` declared at
/// sunset, with each `const NAME = "value"` already substituted in.
///
/// FROZEN, not read. Both files were read from disk here and parsed at test
/// time — including a constant-substitution pass, because several entries name
/// a constant rather than a literal and matching raw text reported those codes
/// as undeclared. The tree is deleted in this milestone, so the pass ran once
/// against the standing tree and its output is pinned below. The assertions are
/// unchanged: a status, a title or a dashboard sentence that moves on the Rust
/// side is still caught, which is the whole reason this test exists — a code
/// answering 403 in one binary and 401 in the other sends a client round a
/// re-authentication loop that never terminates.
const ZIG_ENTRIES: &[ZigEntry] = &[
    ZigEntry {
        code: "UZ-UUIDV7-009",
        status: ".bad_request",
        title: "Invalid identifier shape",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-INTERNAL-001",
        status: ".service_unavailable",
        title: "Service unavailable",
        user_message: Some("A required service is unavailable. Try again shortly."),
    },
    ZigEntry {
        code: "UZ-INTERNAL-002",
        status: ".internal_server_error",
        title: "Request failed",
        user_message: Some("We couldn't finish that request. Try again shortly."),
    },
    ZigEntry {
        code: "UZ-INTERNAL-003",
        status: ".internal_server_error",
        title: "Request failed",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-REQ-001",
        status: ".bad_request",
        title: "Invalid request",
        user_message: Some(
            "That request wasn't valid. Double-check the values you entered and try again.",
        ),
    },
    ZigEntry {
        code: "UZ-REQ-002",
        status: ".payload_too_large",
        title: "Payload too large",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-WORKSPACE-001",
        status: ".conflict",
        title: "Workspace name already exists",
        user_message: Some(
            "A workspace with that name already exists. Check the refreshed list or choose another name.",
        ),
    },
    ZigEntry {
        code: "UZ-AUTH-001",
        status: ".forbidden",
        title: "Forbidden",
        user_message: Some(
            "You need operator access for that. Ask a tenant operator or admin to manage API keys.",
        ),
    },
    ZigEntry {
        code: "UZ-AUTH-002",
        status: ".unauthorized",
        title: "Unauthorized",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-AUTH-003",
        status: ".unauthorized",
        title: "Token expired",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-AUTH-004",
        status: ".service_unavailable",
        title: "Authentication service unavailable",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-AUTH-005",
        status: ".not_found",
        title: "Session not found",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-AUTH-006",
        status: ".unauthorized",
        title: "Session expired",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-AUTH-011",
        status: ".bad_request",
        title: "Verification code did not match",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-AUTH-012",
        status: ".gone",
        title: "Login session already consumed",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-AUTH-013",
        status: ".gone",
        title: "Login session aborted",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-AUTH-014",
        status: ".conflict",
        title: "Login session not approved",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-AUTH-015",
        status: ".conflict",
        title: "Login session already approved",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-AUTH-016",
        status: ".bad_request",
        title: "Invalid command-line public key",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-AUTH-017",
        status: ".bad_request",
        title: "Invalid token name",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-AUTH-018",
        status: ".bad_request",
        title: "Invalid verification code shape",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-AUTH-019",
        status: ".bad_request",
        title: "Invalid ciphertext",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-AUTH-020",
        status: ".bad_request",
        title: "Invalid nonce",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-AUTH-022",
        status: ".forbidden",
        title: "Insufficient scope",
        user_message: Some(
            "You need an additional scope for that. Ask an agentsfleet admin to grant the scope this action requires.",
        ),
    },
    ZigEntry {
        code: "UZ-AUTH-023",
        status: ".unauthorized",
        title: "Command-line credential revoked",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-AUTH-024",
        status: ".not_found",
        title: "Command-line credential not found",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-AUTH-025",
        status: ".unauthorized",
        title: "Credential exchange failed",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-API-001",
        status: ".too_many_requests",
        title: "Too many requests",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-API-002",
        status: ".service_unavailable",
        title: "Activity stream capacity reached",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-WH-001",
        status: ".not_found",
        title: "Fleet not found for webhook",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-WH-002",
        status: ".bad_request",
        title: "Malformed webhook",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-WH-010",
        status: ".unauthorized",
        title: "Invalid webhook signature",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-WH-011",
        status: ".unauthorized",
        title: "Stale webhook timestamp",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-WH-020",
        status: ".unauthorized",
        title: "Webhook credential not configured",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-WH-021",
        status: ".not_found",
        title: "Connector installation is not mapped",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-WH-022",
        status: ".not_found",
        title: "No fleet subscription matched",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-WH-030",
        status: ".payload_too_large",
        title: "Webhook payload too large",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-SLK-010",
        status: ".unauthorized",
        title: "Invalid Slack signature",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-SLK-011",
        status: ".unauthorized",
        title: "Stale Slack timestamp",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-SLK-020",
        status: ".ok",
        title: "Slack team not installed",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-SLK-022",
        status: ".bad_gateway",
        title: "Slack token exchange failed",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-SLK-030",
        status: ".bad_gateway",
        title: "Slack answer post failed",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-TOOL-005",
        status: ".bad_request",
        title: "Unknown tool",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-AGT-003",
        status: ".failed_dependency",
        title: "Fleet credential missing",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-AGT-004",
        status: ".internal_server_error",
        title: "Fleet unavailable",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-AGT-006",
        status: ".conflict",
        title: "Fleet name already exists",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-AGT-008",
        status: ".bad_request",
        title: "Invalid fleet config",
        user_message: Some(
            "That fleet's config isn't valid. Check the trigger, tools, credentials, and budget fields, then try again.",
        ),
    },
    ZigEntry {
        code: "UZ-AGT-009",
        status: ".not_found",
        title: "Fleet not found",
        user_message: Some(
            "We couldn't find that Fleet. It may have been deleted, or the identifier doesn't match one in this workspace.",
        ),
    },
    ZigEntry {
        code: "UZ-AGT-010",
        status: ".conflict",
        title: "Fleet state transition not allowed",
        user_message: Some(
            "That action isn't available for this Fleet right now — check its current status and try again.",
        ),
    },
    ZigEntry {
        code: "UZ-AGT-011",
        status: ".bad_request",
        title: "Fleet files disagree on `name:`",
        user_message: Some(
            "This Fleet Bundle's files disagree on its name. `SKILL.md` and `TRIGGER.md` must match. Fix the source and try again.",
        ),
    },
    ZigEntry {
        code: "UZ-AGT-012",
        status: ".conflict",
        title: "Fleet is paused",
        user_message: Some("This Fleet is paused. Resume it before sending new work."),
    },
    ZigEntry {
        code: "UZ-AGT-013",
        status: ".internal_server_error",
        title: "Fleet install rolled back",
        user_message: Some(
            "We couldn't finish setting up your fleet. Nothing was created — try again.",
        ),
    },
    ZigEntry {
        code: "UZ-AGT-014",
        status: ".precondition_failed",
        title: "Fleet source is stale",
        user_message: Some(
            "Someone else edited this Fleet's source since you opened it. Reload to see their change, then re-apply your edit.",
        ),
    },
    ZigEntry {
        code: "UZ-AGT-015",
        status: ".not_found",
        title: "Event not found",
        user_message: Some(
            "We couldn't find that event. It may have aged out, or the identifier doesn't match one on this Fleet.",
        ),
    },
    ZigEntry {
        code: "UZ-SCHED-001",
        status: ".unprocessable_entity",
        title: "Invalid schedule",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-SCHED-002",
        status: ".not_found",
        title: "Schedule not found",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-SCHED-003",
        status: ".conflict",
        title: "Schedule limit reached",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-SCHED-004",
        status: ".bad_gateway",
        title: "Schedule provider unavailable",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-SCHED-005",
        status: ".unauthorized",
        title: "Invalid schedule signature",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-SCHED-006",
        status: ".conflict",
        title: "Schedule update busy",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-SCHED-007",
        status: ".service_unavailable",
        title: "Schedule service unavailable",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-SCHED-008",
        status: ".conflict",
        title: "Schedule source already exists",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-BUNDLE-001",
        status: ".bad_request",
        title: "Invalid Fleet Bundle",
        user_message: Some(
            "That Fleet Bundle isn't valid. It's missing `SKILL.md`, or has an unsafe or oversized file. Check the source and try again.",
        ),
    },
    ZigEntry {
        code: "UZ-BUNDLE-002",
        status: ".not_found",
        title: "Fleet Bundle not found",
        user_message: Some(
            "We couldn't find that Fleet Bundle. It may not be installed in this workspace yet — check the Fleet library.",
        ),
    },
    ZigEntry {
        code: "UZ-BUNDLE-003",
        status: ".failed_dependency",
        title: "Fleet Bundle secrets missing",
        user_message: Some(
            "This Fleet Bundle needs secrets this workspace doesn't have yet. Add the missing secrets, then install again.",
        ),
    },
    ZigEntry {
        code: "UZ-BUNDLE-004",
        status: ".bad_gateway",
        title: "Fleet Bundle fetch failed",
        user_message: Some(
            "We couldn't fetch that Fleet Bundle from GitHub. Check the source and try again.",
        ),
    },
    ZigEntry {
        code: "UZ-BUNDLE-005",
        status: ".service_unavailable",
        title: "Fleet Bundle storage unavailable",
        user_message: Some("We couldn't store your Fleet Bundle right now. Try again shortly."),
    },
    ZigEntry {
        code: "UZ-VAULT-001",
        status: ".bad_request",
        title: "Secret data must be a non-empty JSON object",
        user_message: Some(
            "That secret needs at least one field. Enter it as a JSON object with one or more keys — not a bare string or list.",
        ),
    },
    ZigEntry {
        code: "UZ-VAULT-002",
        status: ".bad_request",
        title: "Secret data too large",
        user_message: Some(
            "That secret is too large. Keep it under 4 KiB. Trim or shorten the fields.",
        ),
    },
    ZigEntry {
        code: "UZ-VAULT-003",
        status: ".not_found",
        title: "Secret not found",
        user_message: Some(
            "We couldn't find that secret. It may have already been deleted — refresh the list.",
        ),
    },
    ZigEntry {
        code: "UZ-VAULT-004",
        status: ".conflict",
        title: "Secret still referenced by model entries",
        user_message: Some(
            "This key is used by one or more models in your registry. Remove those entries first, then delete the key.",
        ),
    },
    ZigEntry {
        code: "UZ-VAULT-005",
        status: ".conflict",
        title: "Secret name already taken",
        user_message: Some(
            "A secret with that name already exists. Rename this one, or open the existing secret and replace its value.",
        ),
    },
    ZigEntry {
        code: "UZ-PROVIDER-001",
        status: ".bad_request",
        title: "secret_ref required when mode=self_managed",
        user_message: Some(
            "Pick a secret to activate. Choose a stored secret before switching to a self-managed model.",
        ),
    },
    ZigEntry {
        code: "UZ-PROVIDER-002",
        status: ".bad_request",
        title: "Secret not found",
        user_message: Some(
            "We couldn't find that secret. Store it under Secrets & ENVs, then try again.",
        ),
    },
    ZigEntry {
        code: "UZ-PROVIDER-003",
        status: ".bad_request",
        title: "Secret JSON missing required field",
        user_message: Some(
            "That secret is missing required fields. It needs a provider set (and an API key for a named provider) — edit it under Secrets & ENVs and add them.",
        ),
    },
    ZigEntry {
        code: "UZ-PROVIDER-004",
        status: ".bad_request",
        title: "Model not in library",
        user_message: Some(
            "That model isn't in our library yet. Pick a listed model, or ask us to add support for it.",
        ),
    },
    ZigEntry {
        code: "UZ-PROVIDER-005",
        status: ".bad_request",
        title: "Custom endpoint base_url invalid or unsafe",
        user_message: Some(
            "That endpoint URL isn't allowed. Use a public https URL for your custom endpoint.",
        ),
    },
    ZigEntry {
        code: "UZ-PROVIDER-006",
        status: ".not_found",
        title: "Library model not found",
        user_message: Some(
            "We couldn't find that model in the library. Refresh the list and try again.",
        ),
    },
    ZigEntry {
        code: "UZ-PROVIDER-007",
        status: ".conflict",
        title: "Library model is the active platform default",
        user_message: Some(
            "This model is the active platform default — point the default at another model before deleting it.",
        ),
    },
    ZigEntry {
        code: "UZ-PROVIDER-008",
        status: ".conflict",
        title: "Library model already exists",
        user_message: Some(
            "That model is already in the library. Edit the existing entry instead of adding a duplicate.",
        ),
    },
    ZigEntry {
        code: "UZ-PROVIDER-009",
        status: ".internal_server_error",
        title: "Platform model key not configured",
        user_message: Some(
            "Platform defaults aren't set up on this deployment yet. Keep your current provider for now, or contact support.",
        ),
    },
    ZigEntry {
        code: "UZ-PROVIDER-010",
        status: ".internal_server_error",
        title: "Tenant has no primary workspace",
        user_message: Some(
            "Something's off with your account setup. Contact support with the request id below.",
        ),
    },
    ZigEntry {
        code: "UZ-PROVIDER-011",
        status: ".bad_request",
        title: "Source workspace not found",
        user_message: Some(
            "That workspace doesn't exist. Pick one from your workspace list and try again.",
        ),
    },
    ZigEntry {
        code: "UZ-MODELS-001",
        status: ".conflict",
        title: "Cannot delete the active model entry",
        user_message: Some(
            "This is your active model — switch to a different one first, then remove this entry.",
        ),
    },
    ZigEntry {
        code: "UZ-MODELS-002",
        status: ".not_found",
        title: "Referenced secret not found",
        user_message: Some(
            "We couldn't find that key. Store it under Secrets & ENVs first, or pick an existing key.",
        ),
    },
    ZigEntry {
        code: "UZ-MODELS-003",
        status: ".conflict",
        title: "Model entry already exists",
        user_message: Some(
            "You already have this model registered with that key. Edit the existing entry instead.",
        ),
    },
    ZigEntry {
        code: "UZ-MODELS-004",
        status: ".not_found",
        title: "Model entry not found",
        user_message: Some(
            "We couldn't find that model entry. It may have already been removed — refresh the list.",
        ),
    },
    ZigEntry {
        code: "UZ-PREFS-001",
        status: ".bad_request",
        title: "Unknown preference key",
        user_message: Some("That setting doesn't exist. Reload the page and try again."),
    },
    ZigEntry {
        code: "UZ-PREFS-002",
        status: ".bad_request",
        title: "Preference value too large",
        user_message: Some("That setting is too large to save. Reload the page and try again."),
    },
    ZigEntry {
        code: "UZ-CATALOG-001",
        status: ".not_found",
        title: "Fleet library entry not found",
        user_message: Some(
            "We couldn't find that fleet. It may have already been removed — refresh the page.",
        ),
    },
    ZigEntry {
        code: "UZ-CATALOG-002",
        status: ".conflict",
        title: "Cannot publish a fleet with no bundle",
        user_message: Some(
            "There's no bundle for this fleet yet. Fetch it from its repository first, then publish.",
        ),
    },
    ZigEntry {
        code: "UZ-CATALOG-003",
        status: ".conflict",
        title: "Cannot delete a published fleet",
        user_message: Some("This fleet is published. Unpublish it first, then delete it."),
    },
    ZigEntry {
        code: "UZ-CATALOG-004",
        status: ".conflict",
        title: "Catalog id already taken by another repository",
        user_message: Some(
            "A different repository already owns this fleet's name. Rename the bundle, or confirm you want to replace it.",
        ),
    },
    ZigEntry {
        code: "UZ-CATALOG-005",
        status: ".precondition_failed",
        title: "Catalog entry changed since you loaded it",
        user_message: Some(
            "Someone else edited this catalog entry since you opened it. Refresh to see their change, then re-apply your edit.",
        ),
    },
    ZigEntry {
        code: "UZ-LIBRARY-001",
        status: ".bad_request",
        title: "Pagination cursor is malformed",
        user_message: Some(
            "That page link is no longer valid. Go back to the first page and try again.",
        ),
    },
    ZigEntry {
        code: "UZ-LIBRARY-002",
        status: ".bad_request",
        title: "Pagination cursor does not match this request",
        user_message: Some(
            "The filters changed since that page was loaded. Start again from the first page.",
        ),
    },
    ZigEntry {
        code: "UZ-LIBRARY-003",
        status: ".bad_request",
        title: "Pagination or filter input out of bounds",
        user_message: Some(
            "That request asked for too much at once. Try a smaller page size or a shorter filter.",
        ),
    },
    ZigEntry {
        code: "UZ-LIBRARY-004",
        status: ".service_unavailable",
        title: "Catalogue version unavailable",
        user_message: Some("The model catalogue is temporarily unavailable. Try again shortly."),
    },
    ZigEntry {
        code: "UZ-LIBRARY-005",
        status: ".internal_server_error",
        title: "Response exceeded its size ceiling",
        user_message: Some(
            "We couldn't build that page. Try again shortly, and contact support with the request id if it continues.",
        ),
    },
    ZigEntry {
        code: "UZ-LIBRARY-006",
        status: ".service_unavailable",
        title: "Data service temporarily unavailable",
        user_message: Some("We couldn't reach the database. Try again shortly."),
    },
    ZigEntry {
        code: "UZ-LIBRARY-008",
        status: ".conflict",
        title: "Credential was deleted while it was being referenced",
        user_message: Some(
            "That key was deleted while you were saving. Refresh and pick another key.",
        ),
    },
    ZigEntry {
        code: "UZ-STARTUP-001",
        status: ".internal_server_error",
        title: "Required settings missing",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-STARTUP-002",
        status: ".internal_server_error",
        title: "Settings could not load",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-STARTUP-003",
        status: ".internal_server_error",
        title: "Data service unavailable",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-STARTUP-004",
        status: ".internal_server_error",
        title: "Event service unavailable",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-STARTUP-005",
        status: ".internal_server_error",
        title: "Stored data is not ready",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-STARTUP-006",
        status: ".internal_server_error",
        title: "Service could not start",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-RUN-001",
        status: ".unauthorized",
        title: "Invalid runner token",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-RUN-005",
        status: ".conflict",
        title: "Stale fencing token",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-RUN-006",
        status: ".not_found",
        title: "Lease not found",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-RUN-009",
        status: ".unauthorized",
        title: "Runner admin state blocks access",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-RUN-010",
        status: ".conflict",
        title: "Lease exceeded max runtime",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-RUN-011",
        status: ".conflict",
        title: "Lease lost",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-RUN-012",
        status: ".payment_required",
        title: "Lease renewal blocked: no credits",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-RUN-013",
        status: ".bad_request",
        title: "Renew body malformed",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-RUN-014",
        status: ".not_found",
        title: "Runner not found",
        user_message: Some(
            "We couldn't find that runner. It may have been removed — refresh the list.",
        ),
    },
    ZigEntry {
        code: "UZ-RUN-015",
        status: ".payment_required",
        title: "Lease renewal blocked: fleet budget exhausted",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-RUN-016",
        status: ".conflict",
        title: "Active runner must be revoked before deletion",
        user_message: Some("This runner is still live. Revoke it first, then delete it."),
    },
    ZigEntry {
        code: "UZ-RUN-017",
        status: ".bad_request",
        title: "Self-test verdict refused",
        user_message: Some(
            "This runner reported a self-test result that did not make sense, so it was not recorded. Run the test again.",
        ),
    },
    ZigEntry {
        code: "UZ-RUN-018",
        status: ".conflict",
        title: "Self-test refused: runner is revoked",
        user_message: Some(
            "This runner is revoked, so it can't run a self-test. Enroll a new runner to test one.",
        ),
    },
    ZigEntry {
        code: "UZ-EXEC-003",
        status: ".internal_server_error",
        title: "Run timed out",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-EXEC-004",
        status: ".internal_server_error",
        title: "Run memory limit reached",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-EXEC-005",
        status: ".internal_server_error",
        title: "Run resource limit reached",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-EXEC-006",
        status: ".internal_server_error",
        title: "Runner connection lost",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-EXEC-007",
        status: ".internal_server_error",
        title: "Run lease expired",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-EXEC-008",
        status: ".internal_server_error",
        title: "Run stopped during renewal",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-EXEC-009",
        status: ".internal_server_error",
        title: "Runner security check failed",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-EXEC-010",
        status: ".internal_server_error",
        title: "Run crashed",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-EXEC-011",
        status: ".forbidden",
        title: "Landlock policy deny",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-EXEC-012",
        status: ".internal_server_error",
        title: "Runner fleet init failed",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-EXEC-013",
        status: ".internal_server_error",
        title: "Runner fleet run failed",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-EXEC-014",
        status: ".bad_request",
        title: "Run settings invalid",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-EXEC-015",
        status: ".payment_required",
        title: "Run stopped: fleet limit reached",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-EXEC-016",
        status: ".unauthorized",
        title: "Runner token rejected",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-EXEC-017",
        status: ".conflict",
        title: "Assignment exceeds host capability",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-APPROVAL-001",
        status: ".bad_request",
        title: "Approval parse failed",
        user_message: Some(
            "That approval gate's config is invalid. Check the gates section in TRIGGER.md.",
        ),
    },
    ZigEntry {
        code: "UZ-APPROVAL-002",
        status: ".not_found",
        title: "Approval not found",
        user_message: Some(
            "That approval action wasn't found. It may have already timed out or been resolved elsewhere.",
        ),
    },
    ZigEntry {
        code: "UZ-APPROVAL-003",
        status: ".unauthorized",
        title: "Approval invalid signature",
        user_message: Some(
            "That approval callback couldn't be verified. Check the signing secret configuration.",
        ),
    },
    ZigEntry {
        code: "UZ-APPROVAL-004",
        status: ".service_unavailable",
        title: "Approval service unavailable",
        user_message: Some(
            "Approvals are temporarily unavailable. We deny requests while this service is down. Try again shortly.",
        ),
    },
    ZigEntry {
        code: "UZ-APPROVAL-005",
        status: ".bad_request",
        title: "Approval condition invalid",
        user_message: Some(
            "That approval gate's condition is invalid. Check the gate's condition expression for a supported operator.",
        ),
    },
    ZigEntry {
        code: "UZ-APPROVAL-006",
        status: ".conflict",
        title: "Approval already resolved",
        user_message: Some(
            "Someone already resolved this. Refresh to see the outcome and who resolved it.",
        ),
    },
    ZigEntry {
        code: "UZ-MEM-002",
        status: ".not_found",
        title: "Fleet not found for memory op",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-MEM-003",
        status: ".service_unavailable",
        title: "Saved memory unavailable",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-MEM-004",
        status: ".not_found",
        title: "Memory entry not found",
        user_message: Some(
            "That memory entry is already gone — the fleet isn't holding anything under that key.",
        ),
    },
    ZigEntry {
        code: "UZ-APIKEY-001",
        status: ".unauthorized",
        title: "Invalid API key",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-APIKEY-003",
        status: ".not_found",
        title: "API key not found",
        user_message: Some(
            "We couldn't find that API key. It may have already been deleted — refresh the list.",
        ),
    },
    ZigEntry {
        code: "UZ-APIKEY-004",
        status: ".unauthorized",
        title: "API key has been revoked",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-APIKEY-005",
        status: ".conflict",
        title: "Key name already exists in this tenant",
        user_message: Some(
            "An API key with that name already exists. Pick a different name for this tenant.",
        ),
    },
    ZigEntry {
        code: "UZ-APIKEY-006",
        status: ".conflict",
        title: "API key is already revoked",
        user_message: Some(
            "That API key is already revoked. Refresh the list to see its current state.",
        ),
    },
    ZigEntry {
        code: "UZ-APIKEY-007",
        status: ".conflict",
        title: "active cannot be set to true; mint a new key instead",
        user_message: Some("A revoked key can't be reactivated. Mint a new key instead."),
    },
    ZigEntry {
        code: "UZ-APIKEY-008",
        status: ".conflict",
        title: "Active API key must be revoked before deletion",
        user_message: Some("This key is still active. Revoke it first, then delete it."),
    },
    ZigEntry {
        code: "UZ-REPAIR-010",
        status: ".forbidden",
        title: "Write mint requires an approved gate",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-REPAIR-011",
        status: ".forbidden",
        title: "Fleet binding changed since approval",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-REPAIR-012",
        status: ".ok",
        title: "Duplicate repair link refused",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-REPAIR-013",
        status: ".forbidden",
        title: "Write request allowance exhausted",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-REPAIR-014",
        status: ".ok",
        title: "Repair provenance refused",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-GRANT-001",
        status: ".forbidden",
        title: "No integration grant for service",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-GRANT-002",
        status: ".not_found",
        title: "Integration grant not found",
        user_message: Some(
            "We couldn't find that grant request. It may have already been resolved — refresh the list.",
        ),
    },
    ZigEntry {
        code: "UZ-GRANT-003",
        status: ".conflict",
        title: "Grant already resolved",
        user_message: Some(
            "Someone already resolved this. Refresh to see the outcome and who resolved it.",
        ),
    },
    ZigEntry {
        code: "UZ-CRED-001",
        status: ".not_found",
        title: "Integration not connected",
        user_message: Some(
            "That integration isn't connected. Connect it from the Integrations page, then try again.",
        ),
    },
    ZigEntry {
        code: "UZ-CRED-002",
        status: ".service_unavailable",
        title: "Credential broker not configured",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-GH-001",
        status: ".conflict",
        title: "GitHub App reconnect required",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-GH-002",
        status: ".bad_gateway",
        title: "GitHub token mint failed",
        user_message: None,
    },
    ZigEntry {
        code: "UZ-CONN-001",
        status: ".service_unavailable",
        title: "Connector not configured",
        user_message: Some("This connector isn't set up yet. Contact your operator to enable it."),
    },
    ZigEntry {
        code: "UZ-CONN-002",
        status: ".bad_request",
        title: "Invalid connect state",
        user_message: Some(
            "That connection attempt expired or was already used. Start connecting again from the dashboard.",
        ),
    },
    ZigEntry {
        code: "UZ-CONN-003",
        status: ".bad_gateway",
        title: "Connector vendor call exceeded its deadline",
        user_message: Some("We couldn't reach that service right now. Try again shortly."),
    },
    ZigEntry {
        code: "UZ-CONN-004",
        status: ".not_found",
        title: "Unknown connector provider",
        user_message: Some(
            "We don't recognize that connector. Check the available connectors on the dashboard.",
        ),
    },
    ZigEntry {
        code: "UZ-CONN-006",
        status: ".bad_gateway",
        title: "Connector OAuth exchange failed",
        user_message: Some(
            "That connection didn't go through. Try connecting again from the dashboard.",
        ),
    },
    ZigEntry {
        code: "UZ-CONN-007",
        status: ".internal_server_error",
        title: "Connector catalog lookup failed",
        user_message: Some(
            "We couldn't load your connectors right now. Try refreshing — if it keeps failing, contact support.",
        ),
    },
    ZigEntry {
        code: "UZ-CONN-008",
        status: ".forbidden",
        title: "Connector installation ownership not verified",
        user_message: Some(
            "We couldn't verify that this GitHub installation belongs to you. Sign in with the owning GitHub account and try again.",
        ),
    },
];

/// The documentation base `error_entries.zig` declared at sunset.
const ZIG_DOCS_BASE: &str = "https://docs.agentsfleet.net/api-reference/error-codes#";

/// The table covers the registry exactly — no code without an entry, and no
/// entry for a code that is not declared.
///
/// The first half is what keeps [`Problem::UNKNOWN`] unreachable: a declared
/// code with no entry would answer 500 "Unknown error" to a client, which is
/// the failure this test exists to make impossible. The second half stops a
/// stale entry outliving the code it described.
#[test]
fn test_every_declared_code_has_an_entry_and_no_entry_is_orphaned() {
    for code in REGISTRY {
        let problem = Problem::of(*code);
        assert_eq!(
            problem.code(),
            *code,
            "{} has no entry, so it would answer as an unknown error",
            code.as_str()
        );
    }

    let declared: BTreeSet<_> = REGISTRY.iter().map(|code| code.as_str()).collect();
    for entry in entries() {
        assert!(
            declared.contains(entry.code().as_str()),
            "{} has an entry but is not declared in REGISTRY",
            entry.code().as_str()
        );
    }
    assert_eq!(entries().len(), REGISTRY.len());
}

/// Each entry's status, title and prose appear verbatim in the Zig entries.
///
/// A status is a property of the CODE, and this is what holds the two binaries
/// to the same one: `UZ-AUTH-022` answering 403 in one and 401 in the other
/// would send a client round a re-authentication loop that never terminates.
#[test]
fn test_entries_match_the_zig_registry() {
    for entry in entries() {
        let code = entry.code().as_str();
        let declaration = ZIG_ENTRIES
            .iter()
            .find(|row| row.code == code)
            .unwrap_or_else(|| panic!("{code} is not declared in either entries file"));

        assert_eq!(
            declaration.title,
            entry.title(),
            "{code}: title does not match the Zig entries"
        );
        assert_eq!(
            declaration.status,
            zig_status(entry.status()),
            "{code}: status {} does not match the Zig entries",
            entry.status()
        );
        // `eu()` authored a dashboard sentence; `e()` did not. Which one a code
        // used is itself a fact worth pinning: a sentence appearing on a
        // runner-plane code would be prose nobody reads, and one disappearing
        // from a dashboard code shows a person a hint written for an integrator.
        assert_eq!(
            entry.user_message(),
            declaration.user_message,
            "{code}: dashboard sentence does not match the Zig entries"
        );
    }
}

/// The Zig spelling of an HTTP status, as `std.http.Status` names it.
fn zig_status(status: u16) -> &'static str {
    match status {
        400 => ".bad_request",
        401 => ".unauthorized",
        402 => ".payment_required",
        403 => ".forbidden",
        404 => ".not_found",
        409 => ".conflict",
        410 => ".gone",
        412 => ".precondition_failed",
        413 => ".payload_too_large",
        424 => ".failed_dependency",
        429 => ".too_many_requests",
        500 => ".internal_server_error",
        502 => ".bad_gateway",
        503 => ".service_unavailable",
        other => panic!("no Zig spelling recorded for status {other}"),
    }
}

/// The documentation link is derived from the code, so it cannot point at
/// another code's anchor.
#[test]
fn test_the_docs_link_is_derived_from_the_code() {
    for entry in entries() {
        let uri = entry.docs_uri();
        assert!(uri.starts_with(DOCS_BASE), "{uri}");
        assert!(uri.ends_with(entry.code().as_str()), "{uri}");
    }
    assert_eq!(
        Problem::of(error_code::AUTH_INSUFFICIENT_SCOPE).docs_uri(),
        format!("{DOCS_BASE}UZ-AUTH-022")
    );
    // And the base is the one the documentation site actually serves.
    assert_eq!(
        DOCS_BASE, ZIG_DOCS_BASE,
        "the docs base does not match the Zig entries"
    );
}

/// The statuses the auth plane depends on, stated rather than inferred.
///
/// These four are load-bearing beyond the envelope: `docs/AUTH.md` rests on
/// 022 being a 403 (re-authenticating cannot help), and the runner client
/// classifies 004 as transport loss rather than an auth rejection — which is
/// what stops a datastore outage walking a healthy fleet to shutdown.
#[test]
fn test_the_auth_planes_statuses_are_the_documented_ones() {
    for (code, status) in [
        (error_code::AUTH_INSUFFICIENT_SCOPE, 403),
        (error_code::AUTH_UNAUTHORIZED, 401),
        (error_code::AUTH_TOKEN_EXPIRED, 401),
        (error_code::AUTH_UNAVAILABLE, 503),
        (error_code::AUTH_CLI_CREDENTIAL_REVOKED, 401),
        (error_code::APIKEY_REVOKED, 401),
        (error_code::RUN_INVALID_RUNNER_TOKEN, 401),
        (error_code::RUN_ADMIN_STATE_BLOCKED, 401),
    ] {
        assert_eq!(Problem::of(code).status(), status, "{}", code.as_str());
    }
}

/// An unregistered code degrades to an honest 500 rather than failing.
///
/// Unreachable for a code this workspace declares — the totality test above is
/// what makes that true — but a response is being written when this is reached,
/// and there is nothing better to do than answer.
#[test]
fn test_an_unregistered_code_degrades_to_the_unknown_entry() {
    let stranger = afd_core::error_code::ErrorCode::declare("UZ-NOSUCH-001");
    let problem = Problem::of(stranger);

    assert_eq!(problem, Problem::UNKNOWN);
    assert_eq!(problem.status(), 500);
    assert_eq!(problem.title(), "Unknown error");
    assert!(problem.user_message().is_none());
    assert!(!problem.hint().is_empty());
}
