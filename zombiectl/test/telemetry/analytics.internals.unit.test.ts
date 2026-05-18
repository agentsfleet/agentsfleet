// analyticsInternals helper-function coverage. Pure synchronous helpers
// used by analyticsLayer — split out from analytics.layer.unit.test.ts
// to stay under the 350-line file gate. PostHog key/host resolution
// moved to CliConfig — see test/config.posthog-resolution.unit.test.ts.

import { describe, expect, it } from "bun:test";
import { analyticsInternals } from "../../src/services/telemetry/analytics.layer.ts";

describe("analyticsInternals", () => {
  it("stripUndefined drops undefined values only", () => {
    const out = analyticsInternals.stripUndefined({ a: 1, b: undefined, c: null, d: "" });
    expect(out).toEqual({ a: 1, c: null, d: "" });
  });

  it("contextProperties extracts command_run_id / command / flags_used / flag_values", () => {
    expect(
      analyticsInternals.contextProperties({
        command_run_id: "rid",
        command: "login",
        flags_used: ["a"],
        flag_values: { a: 1 },
        distinct_id: "ignored",
      }),
    ).toEqual({
      command_run_id: "rid",
      command: "login",
      flags_used: ["a"],
      flag_values: { a: 1 },
    });
  });

  it("contextProperties returns empty object when context is empty", () => {
    expect(analyticsInternals.contextProperties({})).toEqual({});
  });

  it("resolveGroups returns workspace key when present", () => {
    expect(analyticsInternals.resolveGroups({ groups: { workspace: "ws-1" } })).toEqual({
      workspace: "ws-1",
    });
  });

  it("resolveGroups returns undefined when no workspace group is present", () => {
    expect(analyticsInternals.resolveGroups({})).toBeUndefined();
    expect(analyticsInternals.resolveGroups({ groups: {} })).toBeUndefined();
    expect(
      analyticsInternals.resolveGroups({ groups: { organization: "o" } }),
    ).toBeUndefined();
  });
});
