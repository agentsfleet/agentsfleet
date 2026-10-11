/**
 * Release-gate suite configuration — the acceptance run's shape is itself
 * release-critical: the preflight group gates every journey, and recorded
 * browser traffic never carries the raw bypass secret.
 */
import { describe, expect, it } from "vitest";
import acceptanceConfig from "../playwright.acceptance.config";

const PREFLIGHT_PROJECT = "preflight";

interface ProjectShape {
  name?: string;
  dependencies?: string[];
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
