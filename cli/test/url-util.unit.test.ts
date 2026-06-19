import { describe, expect, test } from "bun:test";

import {
  DEFAULT_DASHBOARD_URL,
  dashboardUrlForApiUrl,
  normalizeDashboardUrl,
  resolveDashboardUrl,
} from "../src/util/url.ts";

describe("dashboard URL resolution", () => {
  test("maps the dev API host to the dev dashboard", () => {
    expect(dashboardUrlForApiUrl("https://api-dev.agentsfleet.net/")).toBe(
      "https://app-dev.agentsfleet.net",
    );
  });

  test("keeps the production dashboard for production and custom API hosts", () => {
    expect(dashboardUrlForApiUrl("https://api.agentsfleet.net")).toBe(
      DEFAULT_DASHBOARD_URL,
    );
    expect(dashboardUrlForApiUrl("http://localhost:3000")).toBe(
      DEFAULT_DASHBOARD_URL,
    );
  });

  test("uses an explicit dashboard override after trimming whitespace and slashes", () => {
    expect(
      resolveDashboardUrl(
        "https://api-dev.agentsfleet.net",
        "  https://dash.override.test//  ",
      ),
    ).toBe("https://dash.override.test");
  });

  test("normalizes empty dashboard values to the production dashboard", () => {
    expect(normalizeDashboardUrl("")).toBe(DEFAULT_DASHBOARD_URL);
    expect(resolveDashboardUrl("not a url", undefined)).toBe(
      DEFAULT_DASHBOARD_URL,
    );
  });
});
