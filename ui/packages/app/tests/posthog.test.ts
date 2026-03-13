import { describe, expect, it } from "vitest";

import { appAnalyticsInternals } from "../lib/analytics/posthog";

describe("app analytics", () => {
  it("resolves enabled config when key is present", () => {
    const cfg = appAnalyticsInternals.resolveConfig({
      NEXT_PUBLIC_POSTHOG_KEY: "phc_test",
    });
    expect(cfg.enabled).toBe(true);
    expect(cfg.host).toBe("https://us.i.posthog.com");
  });

  it("sanitizes payload to allowed keys and non-empty values", () => {
    const props = appAnalyticsInternals.sanitizeProps({
      source: " sidebar ",
      surface: "app",
      target: "",
      path: "/workspaces",
    });
    expect(props).toEqual({
      source: "sidebar",
      surface: "app",
      path: "/workspaces",
    });
  });
});
