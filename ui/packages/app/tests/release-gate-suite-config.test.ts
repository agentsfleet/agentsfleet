/**
 * Release-gate suite configuration — the acceptance run's shape is itself
 * release-critical, so it is pinned here: the preflight group gates every
 * journey, deciders fail for each absent prerequisite, retries stay at zero,
 * and concurrency exists only where isolation earned it.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { describe, expect, it } from "vitest";
import acceptanceConfig from "../playwright.acceptance.config";
import {
  ReleasePreflightError,
  assertCliArtifactPresent,
  assertConnectorConfigured,
  assertRunnerOnline,
  assertRuntimeModelAvailable,
  assertServiceHealthy,
  diagnoseApiError,
} from "./e2e/acceptance/fixtures/preflight";

const PREFLIGHT_PROJECT = "preflight";
const JOURNEYS_PROJECT = "journeys";
const OPERATOR_CATALOG_PROJECT = "operator-catalog";
const OPERATOR_JOURNEY_PROJECT = "operator-journey";
const LIVE_COUNTER_PROJECT = "live-counter";
const PULSE_WALL_PROJECT = "pulse-wall";
const FETCH_AUDIT_PROJECT = "fetch-audit";

const PREFLIGHT_SOURCE_PATH = path.join(
  __dirname,
  "e2e/acceptance/fixtures/preflight.ts",
);

// The QStash-unseeded body fleet creation 503s with on an unready deployment.
const SCHEDULE_UNCONFIGURED_MESSAGE =
  'POST /v1/workspaces/ws/fleets → 503: {"title":"Schedule service unavailable",' +
  '"error_code":"UZ-SCHED-007","request_id":"req_test"}';

interface ProjectShape {
  name?: string;
  dependencies?: string[];
  testIgnore?: unknown;
  testMatch?: unknown;
}

function projects(): ProjectShape[] {
  return (acceptanceConfig.projects ?? []) as ProjectShape[];
}

function projectByName(name: string): ProjectShape {
  const found = projects().find((project) => project.name === name);
  expect(found, `project ${name} must exist`).toBeDefined();
  return found as ProjectShape;
}

function dependsOnPreflightTransitively(name: string, seen: Set<string> = new Set()): boolean {
  if (seen.has(name)) return false;
  seen.add(name);
  const deps = projectByName(name).dependencies ?? [];
  if (deps.includes(PREFLIGHT_PROJECT)) return true;
  return deps.some((dep) => dependsOnPreflightTransitively(dep, seen));
}

describe("release preflight gates every journey", () => {
  it("test_release_preflight_fails_before_browser_journeys", () => {
    // Each absent prerequisite produces a typed preflight failure…
    expect(() => assertRuntimeModelAvailable({ platform_default_available: false })).toThrow(
      ReleasePreflightError,
    );
    expect(() => assertConnectorConfigured([])).toThrow(ReleasePreflightError);
    expect(() =>
      assertConnectorConfigured([
        { id: "github", archetype: "app_install", display_name: "GitHub", configured: false, connected: false },
      ]),
    ).toThrow(ReleasePreflightError);
    expect(() => assertRunnerOnline({ items: [] })).toThrow(ReleasePreflightError);
    expect(() => assertCliArtifactPresent(false)).toThrow(ReleasePreflightError);
    expect(() => assertServiceHealthy(503, true)).toThrow(ReleasePreflightError);
    expect(() => assertServiceHealthy(200, false)).toThrow(ReleasePreflightError);
    const qstash = diagnoseApiError(new Error(SCHEDULE_UNCONFIGURED_MESSAGE), "fleet seed roundtrip");
    expect(qstash).toBeInstanceOf(ReleasePreflightError);
    expect(qstash.message).toContain("UZ-SCHED-007");
    expect(qstash.message).toContain("qstash_registration");

    // …and every non-preflight project depends on the preflight group, so a
    // failed prerequisite stops dependent journeys from ever registering.
    for (const project of projects()) {
      if (project.name === PREFLIGHT_PROJECT) continue;
      expect(
        dependsOnPreflightTransitively(project.name ?? ""),
        `${project.name} must (transitively) depend on ${PREFLIGHT_PROJECT}`,
      ).toBe(true);
    }
  });

  it("should surface an unknown api failure without crashing or inventing a hint", () => {
    // The diagnosis path must never mask the real failure: an unknown error
    // code keeps the status line, an unparseable body degrades gracefully,
    // and a non-Error input still produces a typed preflight failure.
    const unknown = diagnoseApiError(
      new Error('GET /v1/x → 500: {"error_code":"UZ-NEVER-999","request_id":"req_1"}'),
      "probe",
    );
    expect(unknown).toBeInstanceOf(ReleasePreflightError);
    expect(unknown.message).toContain("UZ-NEVER-999");
    expect(unknown.message).toContain("req_1");
    expect(unknown.message).not.toContain("playbook");

    const malformed = diagnoseApiError(new Error("GET /v1/x → 502: {not-json"), "probe");
    expect(malformed).toBeInstanceOf(ReleasePreflightError);
    expect(malformed.message).toContain("GET /v1/x → 502");

    const nonError = diagnoseApiError("plain failure string", "probe");
    expect(nonError).toBeInstanceOf(ReleasePreflightError);
    expect(nonError.message).toContain("probe failed");
  });

  it("should accept a busy runner as alive and reject a lapsed fleet", () => {
    // A runner mid-lease is as alive as an idle one; registered/offline rows
    // alone must still fail the gate — liveness, not mere registration.
    const busy = { items: [{ liveness: "busy" }] } as Parameters<typeof assertRunnerOnline>[0];
    expect(() => assertRunnerOnline(busy)).not.toThrow();
    const stale = {
      items: [{ liveness: "registered" }, { liveness: "offline" }],
    } as Parameters<typeof assertRunnerOnline>[0];
    expect(() => assertRunnerOnline(stale)).toThrow(ReleasePreflightError);
  });

  it("test_release_preflight_is_read_only_and_idempotent", () => {
    // The probe module performs no mutating request: no client mutation verbs
    // and no fetch method override (bare fetch is a GET).
    const source = fs.readFileSync(PREFLIGHT_SOURCE_PATH, "utf8");
    expect(source).not.toMatch(/\.(post|patch|delete)\(/);
    expect(source).not.toMatch(/method:\s*["'](POST|PATCH|PUT|DELETE)/i);

    // Deciders are pure: the same ready input passes twice, the same absent
    // input fails twice with the same diagnosis — nothing is recreated or
    // accumulated between calls.
    const ready = { platform_default_available: true };
    expect(() => assertRuntimeModelAvailable(ready)).not.toThrow();
    expect(() => assertRuntimeModelAvailable(ready)).not.toThrow();
    const firstFailure = captureMessage(() => assertRunnerOnline({ items: [] }));
    const secondFailure = captureMessage(() => assertRunnerOnline({ items: [] }));
    expect(firstFailure).toBe(secondFailure);
    expect(() => assertServiceHealthy(200, true)).not.toThrow();
  });
});

describe("concurrency is earned by isolation", () => {
  it("test_acceptance_does_not_retry_deterministic_failure", () => {
    // Zero retries regardless of environment: a deterministic failure runs
    // once and keeps its first useful trace instead of burning the budget.
    expect(acceptanceConfig.retries).toBe(0);
    expect(acceptanceConfig.use?.trace).toBe("retain-on-failure");
  });

  it("test_acceptance_parallel_groups_are_isolated", () => {
    // More than one worker exists…
    expect(acceptanceConfig.workers).toBeGreaterThan(1);
    // …and the parallel journey group explicitly excludes every spec that
    // mutates shared state: whole-wall counters, platform-operator flows,
    // the global fetch-audit reset, and the preflight itself.
    const ignored = projectByName(JOURNEYS_PROJECT).testIgnore;
    const ignoredList = Array.isArray(ignored) ? ignored.map(String) : [String(ignored)];
    for (const spec of [
      "_smoke.spec.ts",
      "platform-library-onboarding.spec.ts",
      "operator-journey.spec.ts",
      "fleet-count.spec.ts",
      "multi-fleet.spec.ts",
      "workspace-fetch-dedupe.spec.ts",
    ]) {
      expect(
        ignoredList.some((pattern) => pattern.includes(spec)),
        `journeys must not run ${spec} concurrently`,
      ).toBe(true);
    }
  });

  it("test_operator_acceptance_is_serialized", () => {
    // Platform-operator mutations never overlap: the operator journey runs
    // strictly after the catalog flow, and the whole-wall count specs run
    // strictly after the journey group and each other.
    expect(projectByName(OPERATOR_JOURNEY_PROJECT).dependencies).toEqual([
      OPERATOR_CATALOG_PROJECT,
    ]);
    expect(projectByName(LIVE_COUNTER_PROJECT).dependencies).toEqual([JOURNEYS_PROJECT]);
    expect(projectByName(PULSE_WALL_PROJECT).dependencies).toEqual([LIVE_COUNTER_PROJECT]);
    expect(projectByName(FETCH_AUDIT_PROJECT).dependencies).toEqual([
      PULSE_WALL_PROJECT,
      OPERATOR_JOURNEY_PROJECT,
    ]);
  });

  it("should keep the raw bypass secret out of recorded browser traffic", () => {
    // Failure traces record request headers; the bypass secret must reach the
    // browser only as its derived storage-state cookie, never a raw header.
    expect(acceptanceConfig.use?.extraHTTPHeaders).toBeUndefined();
    const storageState = acceptanceConfig.use?.storageState;
    if (typeof storageState !== "string") {
      throw new Error("storageState must be a file path, not an inline state object");
    }
    expect(storageState).toContain(".vercel-bypass-state.json");
    // Video is not release evidence and recording it per-test starves the
    // budget under parallel workers.
    expect(acceptanceConfig.use?.video).toBe("off");
  });
});

function captureMessage(fn: () => void): string {
  try {
    fn();
  } catch (error) {
    return error instanceof Error ? error.message : String(error);
  }
  throw new Error("expected the call to throw");
}
