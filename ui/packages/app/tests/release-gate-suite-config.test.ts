// @vitest-environment node
/**
 * Release-gate suite configuration — the acceptance run's shape is itself
 * release-critical: the preflight group gates every journey, a scope runs the
 * specs its name promises, and recorded browser traffic never carries the raw
 * bypass secret.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import acceptanceConfig from "../playwright.acceptance.config";

const PREFLIGHT_PROJECT = "preflight";
const JOURNEYS_PROJECT = "journeys";
const SCOPE_KNOB = "AGENTSFLEET_ACCEPTANCE_SCOPE";
const PRIME_SPECS = "**/prime-*.spec.ts";
const DEPLOYED_ONLY_SPECS = ["**/prime-watch-dog.spec.ts", "**/prime-chatty-parrot.spec.ts"];

interface ProjectShape {
  name?: string;
  dependencies?: string[];
  testMatch?: unknown;
  testIgnore?: unknown;
}

/** The journeys project as the config builds it under one scope; the scope is read when the config loads. */
async function journeysUnder(scope: string): Promise<ProjectShape> {
  vi.resetModules();
  vi.stubEnv(SCOPE_KNOB, scope);
  const { default: config } = await import("../playwright.acceptance.config");
  const found = ((config.projects ?? []) as ProjectShape[]).find((project) => project.name === JOURNEYS_PROJECT);
  if (!found) throw new Error(`project ${JOURNEYS_PROJECT} must exist`);
  return found;
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
  it("should make every acceptance project depend on the preflight group", () => {
    // A failed prerequisite stops dependent journeys from ever registering.
    for (const project of projects()) {
      if (project.name === PREFLIGHT_PROJECT) continue;
      expect(
        dependsOnPreflightTransitively(project.name ?? ""),
        `${project.name} must (transitively) depend on ${PREFLIGHT_PROJECT}`,
      ).toBe(true);
    }
  });
});

describe("acceptance scopes", () => {
  afterEach(() => {
    vi.unstubAllEnvs();
    vi.resetModules();
  });

  it("should run only the prime journeys in the post-deploy scope, the two a deployed stack alone can wake included", async () => {
    const journeys = await journeysUnder("post-deploy");
    expect(journeys.testMatch).toBe(PRIME_SPECS);
    for (const spec of DEPLOYED_ONLY_SPECS) expect(journeys.testIgnore).not.toContain(spec);
  });

  it("should run every journey in the full scope except the two a local stack cannot wake", async () => {
    const journeys = await journeysUnder("full");
    expect(journeys.testMatch).toBeUndefined();
    for (const spec of DEPLOYED_ONLY_SPECS) expect(journeys.testIgnore).toContain(spec);
  });

  it("should treat an unnamed scope as the full one", async () => {
    vi.resetModules();
    vi.stubEnv(SCOPE_KNOB, undefined);
    const { default: config } = await import("../playwright.acceptance.config");
    const journeys = ((config.projects ?? []) as ProjectShape[]).find((project) => project.name === JOURNEYS_PROJECT);
    expect(journeys?.testMatch).toBeUndefined();
    expect(journeys?.testIgnore).toEqual(expect.arrayContaining(DEPLOYED_ONLY_SPECS));
  });

  it("should give the soak scopes the preflight alone to wait for, and the full scope the whole ladder", async () => {
    const soakUnder = async (scope: string) => {
      vi.resetModules();
      vi.stubEnv(SCOPE_KNOB, scope);
      const { default: config } = await import("../playwright.acceptance.config");
      return ((config.projects ?? []) as ProjectShape[]).find((project) => project.name === "soak")?.dependencies;
    };
    expect(await soakUnder("soak-mini")).toEqual([PREFLIGHT_PROJECT]);
    expect(await soakUnder("ten-fleets")).toEqual([PREFLIGHT_PROJECT]);
    expect(await soakUnder("full")).toEqual(["fetch-audit"]);
  });
});

describe("recorded browser traffic", () => {
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
