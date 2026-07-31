// The runner surface's rendered vocabulary, named once. Failure sentences are
// deliberately NOT here — they come from the shared `failureSentenceFor`
// vocabulary in lib/events/event-summary.ts, the single source every surface
// reads (a second failure vocabulary is a defect).

import { LEASE_OUTCOME, type LeaseOutcome } from "@/lib/api/runners";

export const RUNNERS_CRUMB_LABEL = "Runners";
export const RUNNER_BREADCRUMB_LABEL = "Breadcrumb";
export const RUNNER_ACTIONS_LABEL = "Runner admin actions";
export const COPY_RUNNER_ID_LABEL = "Copy runner ID";
export const OPEN_GRAFANA_LABEL = "Grafana";

export const RAIL_LABEL = "Runner sections";
export const RAIL_LEASES_LABEL = "Leases";
export const RAIL_ACTIVITY_LABEL = "Activity";

export const LEASES_TABLE_LABEL = "Runner leases";
export const ACTIVITY_TABLE_LABEL = "Runner activity";

export const LEASES_EMPTY_TITLE = "No leases yet";
export const LEASES_EMPTY_DESCRIPTION = "Work this host runs appears here, newest first.";
export const ACTIVITY_EMPTY_TITLE = "No lifecycle records yet";
export const ACTIVITY_EMPTY_DESCRIPTION = "Enrolment, liveness and administrative changes appear here.";

// A failed read is NOT an empty history. The empty states above say "this host
// has done nothing"; these say "we could not find out", which is the difference
// between an operator trusting a quiet page and an operator being misled by one.
export const LEASES_UNAVAILABLE = "Lease history is temporarily unavailable. Try refreshing the page.";
export const ACTIVITY_UNAVAILABLE = "Activity history is temporarily unavailable. Try refreshing the page.";

// The workspace filter's vocabulary, plus the URL search param it rides on.
// The param name lives here — the one module the Server Component page and the
// client table both already read — so the reader and the writer can never
// spell it apart.
export const WORKSPACE_FILTER_PARAM = "workspace";
export const WORKSPACE_LABEL = "Workspace";
export const FILTER_BY_WORKSPACE_LABEL = "Show only this workspace";
export const CLEAR_WORKSPACE_FILTER_LABEL = "Clear workspace filter";

export const IDLE_SENTENCE = "Idle. No active leases.";
export const NEVER_CONNECTED_SENTENCE = "Never connected.";
export const INSPECT_RUNNER_LABEL = "Inspect runner";

// The word "reclaimed" was rejected in review: the row says what happened in
// operator terms — this runner stopped renewing, the work went elsewhere.
export const EXPIRED_ROW_SENTENCE = "Lease not renewed";
export const EXPIRED_ROW_DETAIL = "This runner stopped renewing; the work was re-leased to another runner.";
export const UNKNOWN_OUTCOME_SENTENCE = "Outcome not recorded";

export const OUTCOME_LABELS: Record<LeaseOutcome, string> = {
  [LEASE_OUTCOME.running]: "RUNNING",
  [LEASE_OUTCOME.succeeded]: "succeeded",
  [LEASE_OUTCOME.failed]: "failed",
  [LEASE_OUTCOME.expired]: "expired",
  [LEASE_OUTCOME.unknown]: "unknown",
};

export const STRIP_LABEL = "Runner metrics";
export const STRIP_HEARTBEAT_LABEL = "Heartbeat";
export const STRIP_LEASES_NOW_LABEL = "Leases now";
export const STRIP_ACQUIRED_LABEL = "Acquired";
export const STRIP_SUCCEEDED_LABEL = "Succeeded";
export const STRIP_FAILED_LABEL = "Failed";
export const STRIP_EXPIRED_LABEL = "Expired";
export const STRIP_LIFETIME_DETAIL = "lifetime";
export const STRIP_FAILED_DETAIL = "ran, errored";
export const STRIP_EXPIRED_DETAIL = "not renewed";
export const STRIP_VALUE_UNKNOWN = "—";

export const REVIEW_LEASE_TITLE = "Review lease";
export const REVIEW_OUTCOME_LABEL = "Outcome";
export const REVIEW_LEASE_ID_LABEL = "Lease";
export const REVIEW_KIND_LABEL = "Kind";
export const REVIEW_FENCING_LABEL = "Fencing token";
export const REVIEW_EXPIRES_LABEL = "Expires";
export const REVIEW_PROVIDER_LABEL = "Provider";
export const REVIEW_MODEL_LABEL = "Model";
export const REVIEW_POSTURE_LABEL = "Posture";
export const REVIEW_TOKENS_LABEL = "Tokens metered";
export const REVIEW_EVENT_LABEL = "Fleet event";
export const OPEN_FLEET_LABEL = "Open Fleet";
