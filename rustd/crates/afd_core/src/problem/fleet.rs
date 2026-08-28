//! What a client is told about the fleet plane — a runner's refusals, and a
//! fleet's own.
//!
//! The statuses on the `UZ-RUN-*` rows are part of the runner wire: the stock
//! runner classifies a refusal by BOTH status and code, and several entries
//! carry a note saying so where a plausible-looking change would break it.

use super::Problem;
use crate::error_code;

/// This family's entries, in `REGISTRY` order.
pub(super) const FLEET: &[Problem] = &[
    Problem {
        code: error_code::RUN_INVALID_RUNNER_TOKEN,
        status: 401,
        title: "Invalid runner token",
        hint: "The Bearer runner_token is missing, malformed, or not recognized. Re-register the runner.",
        user_message: None,
    },
    Problem {
        code: error_code::RUN_STALE_FENCING_TOKEN,
        // 409, matching the Zig entry's `.conflict`. The word is exact: two
        // runners each hold a lease they believe is live, and the fence is what
        // settles which one is. Not a 403 — nothing about the credential is
        // wrong — and not a 410, because the resource is very much still there,
        // owned by somebody else.
        status: 409,
        title: "Stale fencing token",
        hint: "The lease was reclaimed by a newer holder. This report is rejected; the current holder's result wins.",
        // Not dashboard-facing: this rides the runner-to-control-plane wire
        // contract, and the Zig entry carries the same reachability note.
        user_message: None,
    },
    Problem {
        code: error_code::RUN_LEASE_NOT_FOUND,
        status: 404,
        title: "Lease not found",
        hint: "No active lease matches this lease_id for the presenting runner; it may have expired, been reclaimed, or never existed.",
        user_message: None,
    },
    Problem {
        code: error_code::RUN_ADMIN_STATE_BLOCKED,
        status: 401,
        title: "Runner admin state blocks access",
        hint: "This runner is cordoned, draining, drained, or revoked and cannot call the runner plane. Re-enroll the host to mint a fresh runner token.",
        user_message: None,
    },
    Problem {
        code: error_code::RUN_LEASE_EXCEEDED_MAX_RUNTIME,
        // 409 like the lost verdict beside it, and the pair is the reason both
        // codes exist: the STATUS cannot tell a runner whether its result is
        // still wanted, so the code has to.
        status: 409,
        title: "Lease exceeded max runtime",
        hint: "The lease reached its maximum runtime and cannot renew. The runner stops the child and reports any result.",
        user_message: None,
    },
    Problem {
        code: error_code::RUN_LEASE_LOST,
        status: 409,
        title: "Lease lost",
        hint: "The lease moved to another runner before renewal. The former runner must stop its child.",
        user_message: None,
    },
    Problem {
        code: error_code::RUN_LEASE_RENEWAL_NO_CREDITS,
        // 402, and load-bearing for the reason the entry below it is: the stock
        // runner classifies a renew refusal by status AND code. Both 402s stop
        // the run; the code is what says which pool ran dry, and therefore
        // whether an operator tops up a balance or edits a ceiling.
        status: 402,
        title: "Lease renewal blocked: no credits",
        hint: "The tenant balance cannot cover another run slice. The lease does not renew, and the run stops cleanly.",
        user_message: None,
    },
    Problem {
        code: error_code::RUNNER_NOT_FOUND,
        status: 404,
        title: "Runner not found",
        hint: "No runner matches this runner_id. Verify the platform admin minted the runner before mutating it.",
        user_message: Some(
            "We couldn't find that runner. It may have been removed — refresh the list.",
        ),
    },
    Problem {
        code: error_code::RUN_BUDGET_EXCEEDED,
        // 402, and the status is load-bearing rather than decorative: the stock
        // runner classifies a renew refusal by BOTH status and code, and
        // `control_plane_client_test.zig` pins that a UZ-RUN-015 arriving on
        // any other terminal status is NOT a budget breach. A 403 here would
        // leave the runner treating an exhausted ceiling as an auth failure.
        status: 402,
        title: "Lease renewal blocked: fleet budget exhausted",
        hint: "The fleet reached its daily_dollars or monthly_dollars limit from `TRIGGER.md`, so the run stops. The tenant balance is fine; this is the fleet's own budget.",
        // Not dashboard-facing: this rides the runner-to-control-plane wire
        // protocol, and the Zig entry carries the same reachability note.
        user_message: None,
    },
    Problem {
        code: error_code::RUN_SELFTEST_REFUSED,
        status: 409,
        title: "Self-test refused: runner is revoked",
        hint: "A revoked runner never heartbeats again, so it cannot pick the request up. Enroll a replacement runner and test that one instead.",
        user_message: Some(
            "This runner is revoked, so it can't run a self-test. Enroll a new runner to test one.",
        ),
    },
    Problem {
        code: error_code::AGENTSFLEET_CREDENTIAL_MISSING,
        // 424, matching the Zig entry's `.failed_dependency`. The fleet's own
        // request is well-formed; what is missing is a credential it depends
        // on, which is the distinction this status exists to make.
        status: 424,
        title: "Fleet credential missing",
        hint: "A required credential is not in the vault. Add it with: `agentsfleet secret create <NAME>`",
        // Not dashboard-facing, and the Zig entry carries the same reachability
        // note: this is a CLI and API-key surface, and on the lease path it is
        // logged rather than rendered at all.
        user_message: None,
    },
    Problem {
        code: error_code::AGENTSFLEET_NAME_EXISTS,
        status: 409,
        title: "Fleet name already exists",
        hint: "A Fleet with this name already exists. Use `agentsfleet kill <name>` first, then deploy again.",
        // No dashboard sentence, and the Zig entry carries the same
        // reachability note: an explicit name is a command-line and API-key
        // surface, because the dashboard's one-step install names nothing and
        // takes the re-drawn suffix instead of ever seeing this.
        user_message: None,
    },
    Problem {
        code: error_code::AGENTSFLEET_INVALID_CONFIG,
        status: 400,
        title: "Invalid fleet config",
        hint: "Config JSON is malformed. Check the trigger, tools, credentials, and budget fields in TRIGGER.md frontmatter. See the [Authoring a fleet](/fleets/authoring) guide.",
        user_message: Some(
            "That fleet's config isn't valid. Check the trigger, tools, credentials, and budget fields, then try again.",
        ),
    },
    Problem {
        code: error_code::AGENTSFLEET_NOT_FOUND,
        // 404, and pinned as such on both sides: `error_registry_test.zig`
        // asserts it, because collapsing "no such fleet" and "another
        // workspace's fleet" into one status is what keeps the endpoint from
        // being an oracle for which identifiers are real.
        status: 404,
        title: "Fleet not found",
        hint: "Fleet not found. Verify the fleet_id and that it has not been killed.",
        user_message: Some(
            "We couldn't find that Fleet. It may have been deleted, or the identifier doesn't match one in this workspace.",
        ),
    },
    Problem {
        code: error_code::AGENTSFLEET_ALREADY_TERMINAL,
        status: 409,
        title: "Fleet state transition not allowed",
        hint: "That action is not valid from the fleet's current state. The error detail names the refused transition.",
        user_message: Some(
            "That action isn't available for this Fleet right now — check its current status and try again.",
        ),
    },
    Problem {
        code: error_code::AGENTSFLEET_NAME_MISMATCH,
        status: 400,
        title: "Fleet files disagree on `name:`",
        hint: "Top-level `name:` in `SKILL.md` must match `name:` in `TRIGGER.md`. Use one identity per Fleet Bundle.",
        user_message: Some(
            "This Fleet Bundle's files disagree on its name. `SKILL.md` and `TRIGGER.md` must match. Fix the source and try again.",
        ),
    },
    Problem {
        code: error_code::AGENTSFLEET_INSTALL_ROLLED_BACK,
        // 500 rather than 503, and the difference is what the caller is being
        // promised. A 503 says "come back later"; this says "nothing was kept,
        // so retrying is safe" — which is the fact the rollback earned and the
        // only one the caller can act on.
        status: 500,
        title: "Fleet install rolled back",
        hint: "Event-stream setup failed during create. Nothing was kept, so retry. If it continues, check queue connectivity.",
        user_message: Some(
            "We couldn't finish setting up your fleet. Nothing was created — try again.",
        ),
    },
    Problem {
        code: error_code::AGENTSFLEET_SOURCE_STALE,
        status: 412,
        title: "Fleet source is stale",
        hint: "Someone else saved first: `If-Match` names an old version. Re-read the fleet, re-apply your edit, and retry with the new `etag`.",
        user_message: Some(
            "Someone else edited this Fleet's source since you opened it. Reload to see their change, then re-apply your edit.",
        ),
    },
    Problem {
        code: error_code::AGENTSFLEET_PAUSED_INGRESS,
        status: 409,
        title: "Fleet is paused",
        hint: "This fleet is not active and refuses new work. Resume it with: `agentsfleet resume <fleet>`, then retry.",
        user_message: Some("This Fleet is paused. Resume it before sending new work."),
    },
    Problem {
        code: error_code::EVENT_NOT_FOUND,
        status: 404,
        title: "Event not found",
        hint: "This fleet has no event with that identifier in this workspace. An event in another workspace answers the same.",
        user_message: Some(
            "We couldn't find that event. It may have aged out, or the identifier doesn't match one on this Fleet.",
        ),
    },
    Problem {
        code: error_code::MEM_AGENTSFLEET_NOT_FOUND,
        status: 404,
        title: "Fleet not found for memory op",
        hint: "The fleet_id does not exist or is not in this workspace. Verify both.",
        // Not dashboard-facing in the Zig entries, and the reachability note
        // there says why: the code was authored for the runner's memory push.
        // The operator surface answers it too, and the sentence an integrator
        // reads is the right one for both.
        user_message: None,
    },
    Problem {
        code: error_code::MEM_UNAVAILABLE,
        // 503, and it is the status that carries the degrade-gracefully
        // promise: the fleet keeps running on ephemeral workspace memory while
        // this surface is down, so a client backs off rather than failing.
        status: 503,
        title: "Saved memory unavailable",
        hint: "The memory backend is unreachable; the fleet falls back to ephemeral workspace memory. Check MEMORY_RUNTIME_URL.",
        user_message: None,
    },
    Problem {
        code: error_code::MEM_ENTRY_NOT_FOUND,
        status: 404,
        title: "Memory entry not found",
        hint: "No entry with that key exists for this fleet. List the fleet's memories to confirm the exact key.",
        // The one memory code a person meets: forgetting is an operator's
        // action taken from the dashboard, so the mistyped key needs a sentence
        // written for whoever typed it.
        user_message: Some(
            "That memory entry is already gone — the fleet isn't holding anything under that key.",
        ),
    },
    Problem {
        code: error_code::FLEET_BUNDLE_INVALID,
        status: 400,
        title: "Invalid Fleet Bundle",
        hint: "The supplied Fleet Bundle is missing `SKILL.md` or contains unsafe, oversized, or malformed files.",
        user_message: Some(
            "That Fleet Bundle isn't valid. It's missing `SKILL.md`, or has an unsafe or oversized file. Check the source and try again.",
        ),
    },
    Problem {
        code: error_code::FLEET_BUNDLE_NOT_FOUND,
        status: 404,
        title: "Fleet Bundle not found",
        hint: "No installable library entry or stored snapshot matches the request in this workspace.",
        user_message: Some(
            "We couldn't find that Fleet Bundle. It may not be installed in this workspace yet — check the Fleet library.",
        ),
    },
    Problem {
        code: error_code::FLEET_BUNDLE_FETCH_FAILED,
        status: 502,
        title: "Fleet Bundle fetch failed",
        hint: "The Fleet Bundle source could not be fetched from GitHub. The repository may be missing or private, or GitHub may be unreachable. Verify the source reference and retry.",
        user_message: Some(
            "We couldn't fetch that Fleet Bundle from GitHub. Check the source and try again.",
        ),
    },
    Problem {
        code: error_code::FLEET_BUNDLE_STORAGE_UNAVAILABLE,
        status: 503,
        title: "Fleet Bundle storage unavailable",
        hint: "Snapshot storage is not configured or is unavailable, so the validated bundle could not be stored. Retry later or contact the operator.",
        user_message: Some("We couldn't store your Fleet Bundle right now. Try again shortly."),
    },
    Problem {
        code: error_code::CATALOG_NOT_FOUND,
        status: 404,
        title: "Fleet library entry not found",
        hint: "No catalog entry matches this id. It may already be deleted — refresh the catalog.",
        user_message: Some(
            "We couldn't find that fleet. It may have already been removed — refresh the page.",
        ),
    },
    Problem {
        code: error_code::CATALOG_PUBLISH_WITHOUT_BUNDLE,
        status: 409,
        title: "Cannot publish a fleet with no bundle",
        hint: "No bundle has been fetched for this entry, so there is nothing to publish. Fetch it from its repository first.",
        user_message: Some(
            "There's no bundle for this fleet yet. Fetch it from its repository first, then publish.",
        ),
    },
    Problem {
        code: error_code::CATALOG_DELETE_PUBLISHED,
        status: 409,
        title: "Cannot delete a published fleet",
        hint: "This fleet is published and installable. Unpublish it first, then delete it.",
        user_message: Some("This fleet is published. Unpublish it first, then delete it."),
    },
    Problem {
        code: error_code::CATALOG_ID_COLLISION,
        status: 409,
        title: "Catalog id already taken by another repository",
        hint: "This catalog id already belongs to a different source repository. Rename the bundle, or retry with replace to overwrite deliberately.",
        user_message: Some(
            "A different repository already owns this fleet's name. Rename the bundle, or confirm you want to replace it.",
        ),
    },
    Problem {
        code: error_code::CATALOG_ROW_STALE,
        status: 412,
        title: "Catalog entry changed since you loaded it",
        hint: "Another operator saved first: `If-Match` names an old version. Refetch the row, re-apply your edit, and retry with the new `etag`.",
        user_message: Some(
            "Someone else edited this catalog entry since you opened it. Refresh to see their change, then re-apply your edit.",
        ),
    },
];
