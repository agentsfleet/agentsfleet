import { describe, expect, it } from "vitest";

import {
  createRouteBundleReport,
  displayKib,
  type BundleReportInput,
  type RouteBundleReport,
} from "./route-bundle-report";
import {
  assertSavedReport as validateSavedReport,
  LIMIT_SCOPE,
  type LimitScope,
} from "./saved-route-bundle-report";

const FRAMEWORK_CHUNK = "static/chunks/framework.js";
const ROOT_CHUNK = "static/chunks/root.js";
const SHELL_CHUNK = "static/chunks/shell.js";
const HOME_CHUNK = "static/chunks/home.js";
const AUTH_CHUNK = "static/chunks/auth.js";
const CLI_CHUNK = "static/chunks/cli.js";
const DASHBOARD_PATH = "/(dashboard)/page";
const AUTH_PATH = "/(auth)/sign-in/page";
const CLI_PATH = "/cli-auth/[session_id]/page";
const TRUSTED_BUDGETS = {
  auth_total_kib: 1,
  cli_auth_total_kib: 1,
  dashboard_shared_total_kib: 1,
  route_incremental_kib: 1,
  required_routes: ["/", "/sign-in", "/cli-auth/[session_id]"],
};

function assertSavedReport(
  report: unknown,
  buildId: string,
  scope: LimitScope,
  requiredRoutes = TRUSTED_BUDGETS.required_routes,
) {
  validateSavedReport(report, buildId, scope, {
    ...TRUSTED_BUDGETS,
    required_routes: requiredRoutes,
  });
}

function validReport(): RouteBundleReport {
  const input: BundleReportInput = {
    buildId: "current",
    buildManifest: { rootMainFiles: [FRAMEWORK_CHUNK] },
    appPathRoutes: {
      [DASHBOARD_PATH]: "/",
      [AUTH_PATH]: "/sign-in",
      [CLI_PATH]: "/cli-auth/[session_id]",
    },
    clientManifests: {
      [DASHBOARD_PATH]: {
        entryJSFiles: {
          "[project]/ui/packages/app/app/layout": [ROOT_CHUNK],
          "[project]/ui/packages/app/app/(dashboard)/layout": [
            ROOT_CHUNK,
            SHELL_CHUNK,
          ],
          "[project]/ui/packages/app/app/(dashboard)/page": [HOME_CHUNK],
        },
      },
      [AUTH_PATH]: {
        entryJSFiles: {
          "[project]/ui/packages/app/app/layout": [ROOT_CHUNK],
          "[project]/ui/packages/app/app/(auth)/sign-in/page": [AUTH_CHUNK],
        },
      },
      [CLI_PATH]: {
        entryJSFiles: {
          "[project]/ui/packages/app/app/layout": [ROOT_CHUNK],
          "[project]/ui/packages/app/app/cli-auth/[session_id]/page": [
            CLI_CHUNK,
          ],
        },
      },
    },
    gzipBytes: {
      [FRAMEWORK_CHUNK]: 10,
      [ROOT_CHUNK]: 10,
      [SHELL_CHUNK]: 10,
      [HOME_CHUNK]: 10,
      [AUTH_CHUNK]: 10,
      [CLI_CHUNK]: 10,
    },
    budgets: TRUSTED_BUDGETS,
  };
  return createRouteBundleReport(input);
}

function changed(
  mutate: (report: RouteBundleReport) => void,
): RouteBundleReport {
  const report = structuredClone(validReport());
  mutate(report);
  return report;
}

describe("saved route bundle report validation", () => {
  it("accepts a current, complete, internally consistent report", () => {
    expect(() =>
      assertSavedReport(validReport(), "current", LIMIT_SCOPE.all, [
        "/",
        "/sign-in",
      ]),
    ).not.toThrow();
  });

  it("rejects malformed current bundle budgets", () => {
    expect(() =>
      validateSavedReport(
        validReport(),
        "current",
        LIMIT_SCOPE.all,
        { ...TRUSTED_BUDGETS, required_routes: [] },
      ),
    ).toThrow("current bundle budgets have an invalid shape");
  });

  it.each([
    null,
    [],
    {},
    { ...validReport(), schema_version: 2 },
    { ...validReport(), compression: "brotli" },
    { ...validReport(), build_id: 1 },
    { ...validReport(), build_id: "" },
    { ...validReport(), shared: {} },
    { ...validReport(), routes: {} },
    { ...validReport(), limits: [] },
    { ...validReport(), framework_bytes: "ten" },
    { ...validReport(), framework_bytes: -1 },
    { ...validReport(), pass: "yes" },
  ])("rejects an invalid top-level shape", (report) => {
    expect(() =>
      assertSavedReport(report, "current", LIMIT_SCOPE.all),
    ).toThrow("invalid shape");
  });

  it("rejects invalid shared, route, and limit records", () => {
    const invalidReports = [
      changed((report) => {
        report.shared = [null as never];
      }),
      changed((report) => {
        report.shared[0]!.entry = 7 as never;
      }),
      changed((report) => {
        report.shared[0]!.bytes = "ten" as never;
      }),
      changed((report) => {
        report.routes = [null as never];
      }),
      changed((report) => {
        report.routes[0]!.route = 7 as never;
      }),
      changed((report) => {
        report.routes[0]!.class = "other" as never;
      }),
      changed((report) => {
        report.routes[0]!.pass = "yes" as never;
      }),
      changed((report) => {
        report.routes[0]!.initial_bytes = "ten" as never;
      }),
      changed((report) => {
        report.limits.auth_total_kib = "one" as never;
      }),
      changed((report) => {
        report.limits.auth_total_kib = 0;
      }),
      changed((report) => {
        report.shared[0]!.bytes = -1;
      }),
      changed((report) => {
        report.shared[0]!.bytes = Number.MAX_SAFE_INTEGER + 1;
      }),
      changed((report) => {
        report.shared[0]!.kib = Number.NaN;
      }),
      changed((report) => {
        report.routes[0]!.initial_bytes = Number.POSITIVE_INFINITY;
      }),
      changed((report) => {
        report.routes[0]!.incremental_bytes = -1;
      }),
      changed((report) => {
        report.routes[0]!.initial_kib = Number.NaN;
      }),
      changed((report) => {
        report.routes[0]!.incremental_kib = Number.POSITIVE_INFINITY;
      }),
      changed((report) => {
        report.routes[0]!.limit_kib = 1.5;
      }),
      changed((report) => {
        report.routes[0]!.initial_kib += 0.1;
      }),
    ];
    for (const report of invalidReports) {
      expect(() =>
        assertSavedReport(report, "current", LIMIT_SCOPE.all),
      ).toThrow("invalid shared, route, or limit fields");
    }
  });

  it("rejects stale, incomplete, and inconsistent shared entries", () => {
    expect(() =>
      assertSavedReport(validReport(), "other", LIMIT_SCOPE.all),
    ).toThrow("stale report");
    for (const report of [
      changed((value) => {
        value.shared = value.shared.filter(
          ({ entry }) => entry !== "authenticated_dashboard",
        );
      }),
      changed((value) => {
        value.shared = value.shared.filter(
          ({ entry }) => entry !== "framework_runtime",
        );
      }),
      changed((value) => {
        value.framework_bytes += 1;
      }),
      changed((value) => {
        value.shared.push(structuredClone(value.shared[0]!));
      }),
    ]) {
      expect(() =>
        assertSavedReport(report, "current", LIMIT_SCOPE.all),
      ).toThrow("shared entries");
    }
  });

  it("applies the shared limit independently from incremental route limits", () => {
    const report = changed((value) => {
      const shared = value.shared.find(
        ({ entry }) => entry === "authenticated_dashboard",
      )!;
      shared.bytes = 1025;
      shared.kib = displayKib(shared.bytes);
      const dashboard = value.routes.find(
        ({ class: routeClass }) => routeClass === "dashboard",
      )!;
      dashboard.initial_bytes = shared.bytes + dashboard.incremental_bytes;
      dashboard.initial_kib = displayKib(dashboard.initial_bytes);
      value.pass = false;
    });
    expect(() =>
      assertSavedReport(report, "current", LIMIT_SCOPE.incremental),
    ).not.toThrow();
    expect(() =>
      assertSavedReport(report, "current", LIMIT_SCOPE.shared),
    ).toThrow("authenticated_dashboard exceeds its shared limit");
  });

  it("requires every route class and named route", () => {
    for (const route of ["/sign-in", "/cli-auth/[session_id]", "/"]) {
      const report = changed((value) => {
        value.routes = value.routes.filter((candidate) => candidate.route !== route);
        value.pass = value.routes.every(({ pass }) => pass);
      });
      expect(() =>
        assertSavedReport(report, "current", LIMIT_SCOPE.all),
      ).toThrow("required route class");
    }
    expect(() =>
      assertSavedReport(validReport(), "current", LIMIT_SCOPE.all, [
        "/missing",
      ]),
    ).toThrow("required route /missing");
  });

  it("requires unique, sorted routes", () => {
    const duplicated = changed((report) => {
      report.routes.push(structuredClone(report.routes[0]!));
    });
    expect(() =>
      assertSavedReport(duplicated, "current", LIMIT_SCOPE.all),
    ).toThrow("unique and sorted");

    const unsorted = changed((report) => {
      report.routes.reverse();
    });
    expect(() =>
      assertSavedReport(unsorted, "current", LIMIT_SCOPE.all),
    ).toThrow("unique and sorted");
  });

  it("rejects forged route and top-level verdicts", () => {
    const inflatedLimits = changed((report) => {
      report.limits = {
        auth_total_kib: 999_999,
        cli_auth_total_kib: 999_999,
        dashboard_shared_total_kib: 999_999,
        route_incremental_kib: 999_999,
      };
      for (const route of report.routes) {
        route.limit_kib = 999_999;
      }
    });
    expect(() =>
      validateSavedReport(
        inflatedLimits,
        "current",
        LIMIT_SCOPE.all,
        TRUSTED_BUDGETS,
      ),
    ).toThrow("does not match the current bundle budget");

    const wrongInitialTotal = changed((report) => {
      report.routes[0]!.initial_bytes += 1;
      report.routes[0]!.initial_kib = displayKib(
        report.routes[0]!.initial_bytes,
      );
    });
    expect(() =>
      assertSavedReport(wrongInitialTotal, "current", LIMIT_SCOPE.all),
    ).toThrow("inconsistent limit verdict");

    const wrongLimit = changed((report) => {
      report.routes[0]!.limit_kib += 1;
    });
    expect(() =>
      assertSavedReport(wrongLimit, "current", LIMIT_SCOPE.all),
    ).toThrow("inconsistent limit verdict");

    const wrongRoutePass = changed((report) => {
      report.routes[0]!.pass = false;
      report.pass = false;
    });
    expect(() =>
      assertSavedReport(wrongRoutePass, "current", LIMIT_SCOPE.all),
    ).toThrow("inconsistent limit verdict");

    const wrongTopLevelPass = changed((report) => {
      report.pass = false;
    });
    expect(() =>
      assertSavedReport(wrongTopLevelPass, "current", LIMIT_SCOPE.shared),
    ).toThrow("inconsistent top-level verdict");
  });

  it("applies shared, incremental, and all scopes independently", () => {
    const authFailure = changed((report) => {
      const auth = report.routes.find(({ class: value }) => value === "auth")!;
      auth.initial_bytes = 1025;
      auth.initial_kib = displayKib(auth.initial_bytes);
      auth.incremental_bytes = auth.initial_bytes - report.framework_bytes;
      auth.incremental_kib = displayKib(auth.incremental_bytes);
      auth.pass = false;
      report.pass = false;
    });
    expect(() =>
      assertSavedReport(authFailure, "current", LIMIT_SCOPE.shared),
    ).not.toThrow();
    expect(() =>
      assertSavedReport(authFailure, "current", LIMIT_SCOPE.incremental),
    ).not.toThrow();
    expect(() =>
      assertSavedReport(authFailure, "current", LIMIT_SCOPE.all),
    ).toThrow("/sign-in");

    const dashboardFailure = changed((report) => {
      const dashboard = report.routes.find(
        ({ class: value }) => value === "dashboard",
      )!;
      dashboard.incremental_bytes = 1025;
      dashboard.incremental_kib = displayKib(dashboard.incremental_bytes);
      const shared = report.shared.find(
        ({ entry }) => entry === "authenticated_dashboard",
      )!;
      dashboard.initial_bytes = shared.bytes + dashboard.incremental_bytes;
      dashboard.initial_kib = displayKib(dashboard.initial_bytes);
      dashboard.pass = false;
      report.pass = false;
    });
    expect(() =>
      assertSavedReport(
        dashboardFailure,
        "current",
        LIMIT_SCOPE.incremental,
      ),
    ).toThrow("/");
  });
});
