/**
 * The execution journey's classifier and its place in the release gate.
 *
 * The classifier decides what a red run MEANS — a defect to open, or an
 * environment condition to wait out — so every branch is made red here, with
 * the row that trips it. The config pin is the other half of Dimension 1.5: the
 * journeys project takes the whole acceptance directory minus a named ignore
 * list, and this asserts the execution journey is not on that list, so a green
 * `acceptance-e2e` is a claim that includes it.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { describe, expect, it } from "vitest";
import acceptanceConfig from "../playwright.acceptance.config";
import { LEASE_OUTCOME } from "@/lib/api/runners-types";
import {
  classifyApiFailure,
  classifyReportMissing,
  classifyStillRunning,
  classifyTerminalEvent,
  classifyUnleased,
  ENVIRONMENT_FAILURE_CLASSES,
  EVENT_STATUS,
  ExecutionJourneyFailure,
  FAILURE_CLASS,
  failWith,
  JOURNEY_LEG,
  leaseIsSettled,
  PRODUCT_FAILURE_CLASSES,
  VERDICT_KIND,
  type TerminalObservation,
} from "./e2e/acceptance/fixtures/execution";
import { EXECUTION_REPLY_PREFIX, executionSkillMd } from "./e2e/acceptance/fixtures/seed";

const JOURNEYS_PROJECT = "journeys";
const EXECUTION_SPEC = "fleet-execution.spec.ts";
const EXECUTION_SPEC_PATH = path.join(__dirname, "e2e", "acceptance", EXECUTION_SPEC);

// A row shape per verdict, so each case reads as the row that produced it.
function processed(reply: string | null): TerminalObservation {
  return { status: EVENT_STATUS.processed, failure_label: null, failure_detail: null, response_text: reply };
}

function failed(label: string | null, detail: string | null = null): TerminalObservation {
  return { status: EVENT_STATUS.fleetError, failure_label: label, failure_detail: detail, response_text: null };
}

interface ProjectShape {
  name?: string;
  testIgnore?: unknown;
}

describe("the terminal row is classified before it is trusted", () => {
  it("a processed row with a reply passes, and one without is a product defect", () => {
    expect(classifyTerminalEvent(processed("ACK hello")).kind).toBe(VERDICT_KIND.passed);
    for (const empty of [null, "", "   "]) {
      const verdict = classifyTerminalEvent(processed(empty));
      expect(verdict.kind, `reply ${JSON.stringify(empty)}`).toBe(VERDICT_KIND.product);
      expect(verdict).toMatchObject({ leg: JOURNEY_LEG.execute });
    }
  });

  it("every environment class is named as environment, every product class as product", () => {
    for (const label of ENVIRONMENT_FAILURE_CLASSES) {
      expect(classifyTerminalEvent(failed(label)).kind, label).toBe(VERDICT_KIND.environment);
    }
    for (const label of PRODUCT_FAILURE_CLASSES) {
      expect(classifyTerminalEvent(failed(label)).kind, label).toBe(VERDICT_KIND.product);
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
      const verdict = classifyTerminalEvent(failed(label));
      expect(verdict.kind, JSON.stringify(label)).toBe(VERDICT_KIND.product);
    }
    expect(classifyTerminalEvent({ ...failed(null), status: EVENT_STATUS.gateBlocked }).kind).toBe(
      VERDICT_KIND.product,
    );
  });

  it("the failure line carries the class and the runner's own detail", () => {
    const verdict = classifyTerminalEvent(failed(FAILURE_CLASS.runnerCrash, "provider returned 529"));
    expect(verdict).toMatchObject({
      kind: VERDICT_KIND.environment,
      detail: `${FAILURE_CLASS.runnerCrash}: provider returned 529`,
    });
    expect(() => failWith(classifyReportMissing("succeeded"))).toThrow(ExecutionJourneyFailure);
    expect(() => failWith(classifyReportMissing("succeeded"))).toThrow(
      `[${VERDICT_KIND.product}] ${JOURNEY_LEG.execute}: `,
    );
  });

  it("a lease still running past the budget is the environment, not a scheduler regression", () => {
    // The confusion RULE ECL exists to prevent: this state and "never leased"
    // are both "no terminal row", and only one of them is a defect. Reported as
    // product, a slow provider minute reads as a scheduler regression.
    const running = classifyStillRunning(LEASE_OUTCOME.running);
    expect(running).toMatchObject({
      kind: VERDICT_KIND.environment,
      leg: JOURNEY_LEG.execute,
    });
    expect(running.detail).toContain(LEASE_OUTCOME.running);
    expect(leaseIsSettled(LEASE_OUTCOME.running)).toBe(false);
    // The verdict it must NOT share.
    expect(classifyUnleased(true)).toMatchObject({ kind: VERDICT_KIND.product });
    expect(running.kind).not.toBe(classifyUnleased(true).kind);
  });

  it("a delivery nobody leased is the runner when none is live, and the product when one is", () => {
    expect(classifyUnleased(false)).toMatchObject({ kind: VERDICT_KIND.environment, leg: JOURNEY_LEG.lease });
    expect(classifyUnleased(true)).toMatchObject({ kind: VERDICT_KIND.product, leg: JOURNEY_LEG.lease });
  });

  it("an API refusal is the environment on 5xx and transport loss, the product on 4xx", () => {
    const unavailable = new Error("GET /v1/fleets/runners → 503: {}");
    const refused = new Error("POST /v1/workspaces/ws/fleets → 409: {}");
    const dropped = new TypeError("fetch failed");
    expect(classifyApiFailure(unavailable, JOURNEY_LEG.lease).kind).toBe(VERDICT_KIND.environment);
    expect(classifyApiFailure(dropped, JOURNEY_LEG.lease).kind).toBe(VERDICT_KIND.environment);
    expect(classifyApiFailure(refused, JOURNEY_LEG.install)).toMatchObject({
      kind: VERDICT_KIND.product,
      leg: JOURNEY_LEG.install,
    });
    expect(classifyApiFailure("not even an error", JOURNEY_LEG.execute).kind).toBe(VERDICT_KIND.environment);
  });

  it("a running lease is not settled; every other outcome is", () => {
    expect(leaseIsSettled("running")).toBe(false);
    for (const outcome of ["succeeded", "failed", "expired", "unknown"]) {
      expect(leaseIsSettled(outcome), outcome).toBe(true);
    }
  });
});

describe("the execution journey is part of the required acceptance run", () => {
  it("the journeys project does not ignore it and the spec exists", () => {
    expect(fs.existsSync(EXECUTION_SPEC_PATH), EXECUTION_SPEC_PATH).toBe(true);
    const projects = (acceptanceConfig.projects ?? []) as ProjectShape[];
    const journeys = projects.find((project) => project.name === JOURNEYS_PROJECT);
    expect(journeys, `project ${JOURNEYS_PROJECT} must exist`).toBeDefined();
    const ignored = Array.isArray(journeys?.testIgnore) ? journeys.testIgnore : [journeys?.testIgnore];
    const ignoresExecution = ignored.some((pattern) => String(pattern).includes(EXECUTION_SPEC));
    expect(ignoresExecution, "the execution journey must run in the journeys project").toBe(false);
  });

  it("the execution bundle carries an instruction, unlike the empty-body probe", () => {
    const md = executionSkillMd("exec-check");
    expect(md).toContain("name: exec-check");
    const body = md.split("---\n")[2] ?? "";
    expect(body.trim().length).toBeGreaterThan(0);
    expect(body).toContain(EXECUTION_REPLY_PREFIX);
  });
});
