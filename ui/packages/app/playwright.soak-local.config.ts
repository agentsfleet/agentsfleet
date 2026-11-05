// Local-only run of the final soak journeys against a deployed build.
// Differs from playwright.acceptance.config.ts in one way: no global teardown,
// so a failed fleet and its drafts stay live for inspection. Passing journeys
// delete their own fixtures unless SOAK_KEEP=1; the operator runs
// global-teardown.ts by hand once every failure has been examined.
import { defineConfig } from "@playwright/test";
import base from "./playwright.acceptance.config";

const PREFLIGHT = "preflight";
const SOAK = "soak";

function project(name: string) {
  const found = base.projects?.find(candidate => candidate.name === name);
  if (!found) throw new Error(`playwright.acceptance.config.ts has no ${name} project`);
  return found;
}

// A disabled control otherwise waits out the whole journey deadline; bound it so the stall is recorded.
const ACTION_LIMIT_MS = 120_000;
const found = project(SOAK);
const soak = {...found, use: {...found.use, actionTimeout: ACTION_LIMIT_MS, navigationTimeout: ACTION_LIMIT_MS}};

export default defineConfig({
  ...base,
  globalTeardown: undefined,
  projects: [
    project(PREFLIGHT),
    {...soak, dependencies: [PREFLIGHT]},
    {name: "cli-smoke", testMatch: "**/soak-cli-smoke.spec.ts", dependencies: [PREFLIGHT], use: soak.use},
    {name: "hold", testMatch: "**/soak-hold.spec.ts", use: {...soak.use, headless: false}},
  ],
});
