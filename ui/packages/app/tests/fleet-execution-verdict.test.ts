/**
 * The acceptance journeys' classifier, and the messages they send.
 *
 * The classifier decides what a red run MEANS — a defect to open, or an
 * environment condition to wait out — so every branch is made red here, with
 * the row that trips it.
 */
import { describe, expect, it } from "vitest";
import { probeMessage, probeReply, tokenMessage } from "./e2e/acceptance/fixtures/probe";
import { RunOutcome } from "@/tests/helpers/runner-control-types";
import {
  classifyApiFailure,
  classifyReportMissing,
  classifyStillRunning,
  classifyTerminalTurn,
  classifyIdle,
  ENVIRONMENT_FAILURE_CLASSES,
  TURN_STATUS,
  ExecutionJourneyFailure,
  FAILURE_CLASS,
  failWith,
  JOURNEY_LEG,
  runIsSettled,
  turnHasSettled,
  PRODUCT_FAILURE_CLASSES,
  VERDICT_KIND,
  type TerminalObservation,
} from "./e2e/acceptance/fixtures/execution";


// A row shape per verdict, so each case reads as the row that produced it.
function processed(reply: string | null): TerminalObservation {
  return { status: TURN_STATUS.processed, failure_label: null, failure_detail: null, response_text: reply };
}

function failed(label: string | null, detail: string | null = null): TerminalObservation {
  return { status: TURN_STATUS.fleetError, failure_label: label, failure_detail: detail, response_text: null };
}

describe("the terminal row is classified before it is trusted", () => {
  it("a processed row with a reply passes, and one without is a product defect", () => {
    expect(classifyTerminalTurn(processed("ACK hello")).kind).toBe(VERDICT_KIND.passed);
    for (const empty of [null, "", "   "]) {
      const verdict = classifyTerminalTurn(processed(empty));
      expect(verdict.kind, `reply ${JSON.stringify(empty)}`).toBe(VERDICT_KIND.product);
      expect(verdict).toMatchObject({ leg: JOURNEY_LEG.execute });
    }
  });

  it("every environment class is named as environment, every product class as product", () => {
    for (const label of ENVIRONMENT_FAILURE_CLASSES) {
      expect(classifyTerminalTurn(failed(label)).kind, label).toBe(VERDICT_KIND.environment);
    }
    for (const label of PRODUCT_FAILURE_CLASSES) {
      expect(classifyTerminalTurn(failed(label)).kind, label).toBe(VERDICT_KIND.product);
    }
    // The two lists partition the wire enum: nothing is on both, nothing on
    // neither. A class added to `FailureClass` without a home here fails the
    // next case, which is the point.
    const known = new Set(Object.values(FAILURE_CLASS));
    const named = new Set([...ENVIRONMENT_FAILURE_CLASSES, ...PRODUCT_FAILURE_CLASSES]);
    expect(named).toEqual(known);
    expect(ENVIRONMENT_FAILURE_CLASSES.filter((label) => PRODUCT_FAILURE_CLASSES.includes(label))).toEqual([]);
  });

  it("an unknown or absent failure class fails closed as a product defect", () => {
    for (const label of [null, "", "provider_hiccup"]) {
      const verdict = classifyTerminalTurn(failed(label));
      expect(verdict.kind, JSON.stringify(label)).toBe(VERDICT_KIND.product);
    }
    expect(classifyTerminalTurn({ ...failed(null), status: TURN_STATUS.gateBlocked }).kind).toBe(
      VERDICT_KIND.product,
    );
  });

  it("a turn that ended waiting on a person is a product defect, named as the wait it is", () => {
    // `request_access` and a question settle the asking turn `waiting`; a walk
    // that reads it as the delivery's end met no answer, not an unknown class.
    const verdict = classifyTerminalTurn({ ...failed(null), status: TURN_STATUS.waiting });
    expect(verdict).toMatchObject({ kind: VERDICT_KIND.product, leg: JOURNEY_LEG.execute });
    expect(verdict.kind === VERDICT_KIND.passed ? "" : verdict.detail).toContain("waiting");
  });

  it("a queued or running turn has not settled; every status a run ends in has", () => {
    for (const status of [TURN_STATUS.queued, TURN_STATUS.received]) {
      expect(turnHasSettled(status), status).toBe(false);
    }
    for (const status of [TURN_STATUS.processed, TURN_STATUS.fleetError, TURN_STATUS.gateBlocked, TURN_STATUS.waiting, "cancelled"]) {
      expect(turnHasSettled(status), status).toBe(true);
    }
  });

  it("the failure line carries the class and the runner's own detail", () => {
    const verdict = classifyTerminalTurn(failed(FAILURE_CLASS.runnerCrash, "provider returned 529"));
    expect(verdict).toMatchObject({
      kind: VERDICT_KIND.environment,
      detail: `${FAILURE_CLASS.runnerCrash}: provider returned 529`,
    });
    expect(() => failWith(classifyReportMissing("succeeded"))).toThrow(ExecutionJourneyFailure);
    expect(() => failWith(classifyReportMissing("succeeded"))).toThrow(
      `[${VERDICT_KIND.product}] ${JOURNEY_LEG.execute}: `,
    );
  });

  it("a run still running past the budget is the environment, not a scheduler regression", () => {
    // The confusion RULE ECL exists to prevent: this state and "never held"
    // are both "no terminal row", and only one of them is a defect. Reported as
    // product, a slow provider minute reads as a scheduler regression.
    const running = classifyStillRunning(RunOutcome.Running);
    expect(running).toMatchObject({
      kind: VERDICT_KIND.environment,
      leg: JOURNEY_LEG.execute,
    });
    expect(running.detail).toContain(RunOutcome.Running);
    expect(runIsSettled(RunOutcome.Running)).toBe(false);
    // The verdict it must NOT share.
    expect(classifyIdle(true)).toMatchObject({ kind: VERDICT_KIND.product });
    expect(running.kind).not.toBe(classifyIdle(true).kind);
  });

  it("a delivery nobody held is the runner when none is live, and the product when one is", () => {
    expect(classifyIdle(false)).toMatchObject({ kind: VERDICT_KIND.environment, leg: JOURNEY_LEG.run });
    expect(classifyIdle(true)).toMatchObject({ kind: VERDICT_KIND.product, leg: JOURNEY_LEG.run });
  });

  it("an API refusal is the environment on 5xx and transport loss, the product on 4xx", () => {
    const unavailable = new Error("GET /v1/admin/runners → 503: {}");
    const refused = new Error("POST /v1/workspaces/ws/fleets → 409: {}");
    const dropped = new TypeError("fetch failed");
    expect(classifyApiFailure(unavailable, JOURNEY_LEG.run).kind).toBe(VERDICT_KIND.environment);
    expect(classifyApiFailure(dropped, JOURNEY_LEG.run).kind).toBe(VERDICT_KIND.environment);
    expect(classifyApiFailure(refused, JOURNEY_LEG.install)).toMatchObject({
      kind: VERDICT_KIND.product,
      leg: JOURNEY_LEG.install,
    });
    expect(classifyApiFailure("not even an error", JOURNEY_LEG.execute).kind).toBe(VERDICT_KIND.environment);
  });

  it("a running run is not settled; every other outcome is", () => {
    expect(runIsSettled("running")).toBe(false);
    for (const outcome of ["succeeded", "failed", "expired", "unknown"]) {
      expect(runIsSettled(outcome), outcome).toBe(true);
    }
  });
});

describe("probe messages", () => {
  it("never spell the reply they ask for, so the person's own message cannot pass for it", () => {
    expect(probeReply("tok-7")).toBe("ACK tok-7");
    expect(probeMessage("tok-7")).toContain("tok-7");
    expect(probeMessage("tok-7")).not.toContain(probeReply("tok-7"));
    expect(probeMessage("tok-7")).toContain("Do not call any tool");
  });
  it("keep a fixture fleet's own words and add the token its reply must end with", () => {
    const sent = tokenMessage("Say hello to the team.", "tok-8");
    expect(sent.startsWith("Say hello to the team. ")).toBe(true);
    expect(sent).toContain("tok-8");
    expect(sent).not.toContain(probeReply("tok-8"));
  });
});
