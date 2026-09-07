/**
 * execution.ts — what the fleet-execution journey observes, and how it names
 * what it saw.
 *
 * Two halves. The polls read the operator plane and the tenant thread until a
 * delivery has been leased and has reached a terminal row. The classifier then
 * says, from that row alone, whether the walk PASSED, met a PRODUCT defect, or
 * met an ENVIRONMENT condition — a provider outage, a runner that died, a lease
 * that ran out of clock. RULE ECL: those are different failures with different
 * next moves, and a lane that collapses them reports a regression every time
 * GitHub or the model endpoint has a bad minute.
 *
 * The classifier is a pure function over the row, so the unit lane proves each
 * branch red and green without a runner in reach; the journey only calls it.
 */
import type { RunnerLeaseResponse, RunnerListResponse } from "@/lib/api/runners";
import type { EventDetail, ThreadPage } from "@/lib/api/events";
import { LEASE_OUTCOME } from "@/lib/api/runners-types";
import { clientFor, type ClientHandle } from "./api-client";
import { FIXTURE_KEY } from "./constants";

// One spelling each for the statuses and failure classes this module reads
// (RULE UFS). The event statuses mirror `lib/api/events.ts:EventStatus`; the
// failure labels mirror `afd_wire::report::FailureClass`, serialised
// snake_case, and the runner's `execution_result.zig` agrees by test.
export const EVENT_STATUS = {
  received: "received",
  processed: "processed",
  fleetError: "fleet_error",
  gateBlocked: "gate_blocked",
} as const;

export const EVENT_TYPE_CHAT = "chat";

export const FAILURE_CLASS = {
  startupPosture: "startup_posture",
  policyDeny: "policy_deny",
  timeoutKill: "timeout_kill",
  oomKill: "oom_kill",
  resourceKill: "resource_kill",
  runnerCrash: "runner_crash",
  transportLoss: "transport_loss",
  landlockDeny: "landlock_deny",
  leaseExpired: "lease_expired",
  renewalTerminate: "renewal_terminate",
  budgetBreach: "budget_breach",
} as const;

// The product decided these: the fleet's own instructions, a policy the
// author wrote, the sandbox the platform assigned, the budget the author set.
// Each one is a defect or a configuration the journey must not paper over.
export const PRODUCT_FAILURE_CLASSES: readonly string[] = [
  FAILURE_CLASS.startupPosture,
  FAILURE_CLASS.policyDeny,
  FAILURE_CLASS.landlockDeny,
  FAILURE_CLASS.budgetBreach,
];

// The host, the provider path or the clock decided these. A runner that
// crashed mid-call, a transport that dropped, a lease that ran out while the
// provider stalled: none of them is a fleet that cannot execute.
export const ENVIRONMENT_FAILURE_CLASSES: readonly string[] = [
  FAILURE_CLASS.runnerCrash,
  FAILURE_CLASS.transportLoss,
  FAILURE_CLASS.leaseExpired,
  FAILURE_CLASS.renewalTerminate,
  FAILURE_CLASS.timeoutKill,
  FAILURE_CLASS.oomKill,
  FAILURE_CLASS.resourceKill,
];

export const VERDICT_KIND = {
  passed: "passed",
  product: "product",
  environment: "environment",
} as const;
export type VerdictKind = (typeof VERDICT_KIND)[keyof typeof VERDICT_KIND];

// The legs of the walk, so a failure line names WHERE it broke.
export const JOURNEY_LEG = {
  install: "install",
  lease: "lease",
  execute: "execute",
  observe: "observe",
} as const;
export type JourneyLeg = (typeof JOURNEY_LEG)[keyof typeof JOURNEY_LEG];

export type FailedVerdict =
  | { kind: typeof VERDICT_KIND.product; leg: JourneyLeg; detail: string }
  | { kind: typeof VERDICT_KIND.environment; leg: JourneyLeg; detail: string };

export type ExecutionVerdict = { kind: typeof VERDICT_KIND.passed } | FailedVerdict;

// The subset of a terminal row the classifier reads. Narrow on purpose: a
// test hands it four fields, and the journey hands it the thread row.
export type TerminalObservation = Pick<
  EventDetail,
  "status" | "failure_label" | "failure_detail" | "response_text"
>;

const HTTP_SERVER_ERROR_FLOOR = 500;
// `api-client.ts` renders a refused request as `METHOD /path → 503: body`; the
// status is the one fact the classifier needs out of that line.
const API_STATUS_PATTERN = /→ (\d{3}):/;

const DETAIL_NO_REPLY = "the lease finished without a reply";
const DETAIL_GATE_BLOCKED = "the delivery is waiting on an approval no journey granted";
const DETAIL_RUNNER_OFFLINE = "no runner was online to lease the delivery";
const DETAIL_NEVER_LEASED = "an online runner never leased the delivery";
const DETAIL_REPORT_MISSING = "the lease settled and no terminal row followed";
const DETAIL_UNKNOWN_CLASS = "an unrecognised failure class";
const DETAIL_API_UNREACHABLE = "the API could not be reached";

const FLEET_RUNNERS_PATH = "/v1/fleets/runners";
const LEASE_PAGE_SIZE = 50;
const LIVE_RUNNER_STATES: readonly string[] = ["online", "busy"];

/**
 * The one line a failed run prints. Its shape is the contract: the kind in
 * brackets, the leg, then the detail — so a reader knows from the first word
 * whether to open a bug or check the provider status page.
 */
export class ExecutionJourneyFailure extends Error {
  readonly verdict: FailedVerdict;

  constructor(verdict: FailedVerdict) {
    super(`[${verdict.kind}] ${verdict.leg}: ${verdict.detail}`);
    this.name = "ExecutionJourneyFailure";
    this.verdict = verdict;
  }
}

/** Ends the walk on a classified verdict. Typed `never` so a caller narrows. */
export function failWith(verdict: FailedVerdict): never {
  throw new ExecutionJourneyFailure(verdict);
}

/** Throws the verdict as a journey failure unless the walk passed. */
export function assertPassed(verdict: ExecutionVerdict): void {
  if (verdict.kind === VERDICT_KIND.passed) return;
  failWith(verdict);
}

/**
 * What a terminal row says about the execute leg.
 *
 * A processed row with a reply passed. A processed row with NO reply is a
 * product defect — the runner reported success and the fleet said nothing. A
 * failed row is classified by its label, and a label this build does not know
 * is a defect until somebody names it: failing closed here is what stops a new
 * failure class from shipping as "environment".
 */
export function classifyTerminalEvent(row: TerminalObservation): ExecutionVerdict {
  if (row.status === EVENT_STATUS.processed) {
    const reply = row.response_text ?? "";
    if (reply.trim().length > 0) return { kind: VERDICT_KIND.passed };
    return { kind: VERDICT_KIND.product, leg: JOURNEY_LEG.execute, detail: DETAIL_NO_REPLY };
  }
  if (row.status === EVENT_STATUS.gateBlocked) {
    return { kind: VERDICT_KIND.product, leg: JOURNEY_LEG.execute, detail: DETAIL_GATE_BLOCKED };
  }
  const label = row.failure_label ?? "";
  const detail = row.failure_detail?.trim() || label || DETAIL_UNKNOWN_CLASS;
  if (ENVIRONMENT_FAILURE_CLASSES.includes(label)) {
    return { kind: VERDICT_KIND.environment, leg: JOURNEY_LEG.execute, detail: `${label}: ${detail}` };
  }
  return { kind: VERDICT_KIND.product, leg: JOURNEY_LEG.execute, detail: `${label || DETAIL_UNKNOWN_CLASS}: ${detail}` };
}

/**
 * What a delivery that never reached a terminal row says. With no live runner
 * that is the environment; with one, the scheduler or the lease path lost it.
 */
export function classifyUnleased(runnerLive: boolean): FailedVerdict {
  return runnerLive
    ? { kind: VERDICT_KIND.product, leg: JOURNEY_LEG.lease, detail: DETAIL_NEVER_LEASED }
    : { kind: VERDICT_KIND.environment, leg: JOURNEY_LEG.lease, detail: DETAIL_RUNNER_OFFLINE };
}

/**
 * What a lease that settled with no terminal row behind it says: the runner
 * finished, and the report that should have closed the row never landed. The
 * daemon's, whichever way the lease went.
 */
export function classifyReportMissing(outcome: string): FailedVerdict {
  return {
    kind: VERDICT_KIND.product,
    leg: JOURNEY_LEG.execute,
    detail: `${DETAIL_REPORT_MISSING} (lease ${outcome})`,
  };
}

/**
 * What a refused or unreachable API call says. A 5xx and a transport failure
 * are the environment; a 4xx is this daemon refusing a request the journey
 * composed, which is a defect on one side or the other.
 */
export function classifyApiFailure(error: unknown, leg: JourneyLeg): FailedVerdict {
  const message = error instanceof Error ? error.message : String(error);
  const status = Number(API_STATUS_PATTERN.exec(message)?.[1] ?? Number.NaN);
  if (Number.isNaN(status)) {
    return { kind: VERDICT_KIND.environment, leg, detail: `${DETAIL_API_UNREACHABLE}: ${message}` };
  }
  if (status >= HTTP_SERVER_ERROR_FLOOR) {
    return { kind: VERDICT_KIND.environment, leg, detail: message };
  }
  return { kind: VERDICT_KIND.product, leg, detail: message };
}

export interface LeaseLocation {
  runnerId: string;
  hostId: string;
  outcome: string;
}

/**
 * The lease an online runner took for this fleet, wherever the scheduler put
 * it. The read is per runner, so the walk visits every runner's first lease
 * page; the fleet is fresh, so its first lease is the one.
 */
export async function findLeaseFor(fleetId: string): Promise<LeaseLocation | null> {
  const operator = clientFor(FIXTURE_KEY.operator);
  const runners = await operator.get<RunnerListResponse>(FLEET_RUNNERS_PATH);
  for (const runner of runners.items) {
    const leases = await operator.get<RunnerLeaseResponse>(
      `${FLEET_RUNNERS_PATH}/${runner.id}/leases?limit=${LEASE_PAGE_SIZE}`,
    );
    const lease = leases.items.find((item) => item.fleet_id === fleetId);
    if (lease) return { runnerId: runner.id, hostId: runner.host_id, outcome: lease.outcome };
  }
  return null;
}

/** Whether any runner reads online or busy right now. */
export async function anyRunnerLive(): Promise<boolean> {
  const runners = await clientFor(FIXTURE_KEY.operator).get<RunnerListResponse>(FLEET_RUNNERS_PATH);
  return runners.items.some((item) => LIVE_RUNNER_STATES.includes(item.liveness));
}

/** Whether a lease outcome is one the runner has finished reporting. */
export function leaseIsSettled(outcome: string): boolean {
  return outcome !== LEASE_OUTCOME.running;
}

/**
 * The newest chat turn on the fleet's thread that has left `received`, or
 * `null` while the delivery is still in flight. Reads the messages route
 * because it is the one read that carries `response_text`.
 */
export async function readTerminalChatTurn(
  handle: ClientHandle,
  workspaceId: string,
  fleetId: string,
): Promise<EventDetail | null> {
  const page = await clientFor(handle).get<ThreadPage>(
    `/v1/workspaces/${workspaceId}/fleets/${fleetId}/messages`,
  );
  const turn = page.items.find(
    (item) => item.event_type === EVENT_TYPE_CHAT && item.status !== EVENT_STATUS.received,
  );
  return turn ?? null;
}
