import { describe, expect, it } from "vitest";

import {
  assertSavedReport as validateSavedReport,
  type LimitScope,
} from "./saved-route-bundle-report";
import {
  createRouteBundleReport,
  parseClientReferenceManifest,
  type BundleBudgets,
  type BundleReportInput,
  type ClientReferenceManifest,
} from "./route-bundle-report";

const PROJECT_APP = "[project]/ui/packages/app/app";
const FRAMEWORK_CHUNK = "static/chunks/framework.js";
const ROOT_CLIENT_CHUNK = "static/chunks/root-client.js";
const SHELL_CHUNK = "static/chunks/shell.js";
const HOME_CHUNK = "static/chunks/home.js";
const MODELS_CHUNK = "static/chunks/models.js";
const AUTH_CHUNK = "static/chunks/auth.js";
const CLI_AUTH_CHUNK = "static/chunks/cli-auth.js";
const KIBIBYTE = 1024;

const APP_PATHS = {
  dashboard: "/(dashboard)/page",
  models: "/(dashboard)/admin/models/page",
  auth: "/(auth)/sign-in/[[...sign-in]]/page",
  cliAuth: "/cli-auth/[session_id]/page",
} as const;

const ROUTES = {
  dashboard: "/",
  models: "/admin/models",
  auth: "/sign-in/[[...sign-in]]",
  cliAuth: "/cli-auth/[session_id]",
} as const;

const BUDGETS: BundleBudgets = {
  auth_total_kib: 1,
  cli_auth_total_kib: 1,
  dashboard_shared_total_kib: 1,
  route_incremental_kib: 1,
  required_routes: [
    ROUTES.dashboard,
    ROUTES.models,
    ROUTES.auth,
    ROUTES.cliAuth,
  ],
};

function assertSavedReport(
  report: unknown,
  buildId: string,
  scope: LimitScope,
) {
  validateSavedReport(report, buildId, scope, BUDGETS);
}

function routeManifest(
  appPath: string,
  files: string[],
  includeDashboardLayout = false,
): ClientReferenceManifest {
  const entryJSFiles: Record<string, string[]> = {
    [`${PROJECT_APP}/layout`]: [ROOT_CLIENT_CHUNK],
    [`${PROJECT_APP}${appPath}`]: files,
  };
  if (includeDashboardLayout) {
    entryJSFiles[`${PROJECT_APP}/(dashboard)/layout`] = [
      ROOT_CLIENT_CHUNK,
      SHELL_CHUNK,
    ];
  }
  return { entryJSFiles };
}

function bundleInput(): BundleReportInput {
  return {
    buildId: "build-current",
    buildManifest: { rootMainFiles: [FRAMEWORK_CHUNK] },
    appPathRoutes: {
      [APP_PATHS.dashboard]: ROUTES.dashboard,
      [APP_PATHS.models]: ROUTES.models,
      [APP_PATHS.auth]: ROUTES.auth,
      [APP_PATHS.cliAuth]: ROUTES.cliAuth,
    },
    clientManifests: {
      [APP_PATHS.dashboard]: routeManifest(
        APP_PATHS.dashboard,
        [ROOT_CLIENT_CHUNK, SHELL_CHUNK, HOME_CHUNK],
        true,
      ),
      [APP_PATHS.models]: routeManifest(
        APP_PATHS.models,
        [ROOT_CLIENT_CHUNK, SHELL_CHUNK, MODELS_CHUNK],
        true,
      ),
      [APP_PATHS.auth]: routeManifest(
        APP_PATHS.auth,
        [ROOT_CLIENT_CHUNK, AUTH_CHUNK],
      ),
      [APP_PATHS.cliAuth]: routeManifest(
        APP_PATHS.cliAuth,
        [ROOT_CLIENT_CHUNK, CLI_AUTH_CHUNK],
      ),
    },
    gzipBytes: {
      [FRAMEWORK_CHUNK]: 100,
      [ROOT_CLIENT_CHUNK]: 40,
      [SHELL_CHUNK]: 60,
      [HOME_CHUNK]: 25,
      [MODELS_CHUNK]: 35,
      [AUTH_CHUNK]: 90,
      [CLI_AUTH_CHUNK]: 100,
    },
    budgets: BUDGETS,
  };
}

function reverseRecord<T>(value: Record<string, T>): Record<string, T> {
  return Object.fromEntries(Object.entries(value).reverse());
}

describe("authenticated route bundle report", () => {
  it("test_route_bundle_report_is_deterministic_and_fails_closed", () => {
    const input = bundleInput();
    const report = createRouteBundleReport(input);
    const reordered = createRouteBundleReport({
      ...input,
      appPathRoutes: reverseRecord(input.appPathRoutes),
      clientManifests: reverseRecord(input.clientManifests),
      gzipBytes: reverseRecord(input.gzipBytes),
    });

    expect(reordered).toEqual(report);
    expect(report.framework_bytes).toBe(100);
    expect(report.shared).toEqual([
      { entry: "framework_runtime", bytes: 100, kib: 0.1 },
      { entry: "authenticated_dashboard", bytes: 200, kib: 0.2 },
    ]);
    expect(report.routes.map(({ route }) => route)).toEqual([
      ROUTES.dashboard,
      ROUTES.models,
      ROUTES.cliAuth,
      ROUTES.auth,
    ]);
    expect(report.routes[0]).toMatchObject({
      class: "dashboard",
      initial_bytes: 225,
      incremental_bytes: 25,
      pass: true,
    });

    const missingChunk = bundleInput();
    delete missingChunk.gzipBytes[HOME_CHUNK];
    expect(() => createRouteBundleReport(missingChunk)).toThrow(HOME_CHUNK);

    expect(() => parseClientReferenceManifest("not a manifest")).toThrow(
      "assignment",
    );

    const duplicated = bundleInput();
    duplicated.clientManifests[APP_PATHS.dashboard] = routeManifest(
      APP_PATHS.dashboard,
      [ROOT_CLIENT_CHUNK, HOME_CHUNK, HOME_CHUNK],
      true,
    );
    expect(() => createRouteBundleReport(duplicated)).toThrow("duplicate");
  });

  it("test_route_bundle_report_rejects_stale_build", () => {
    const report = createRouteBundleReport(bundleInput());
    expect(() => assertSavedReport(report, "build-other", "all")).toThrow(
      /build-other.*build-current/,
    );
    expect(() => assertSavedReport(report, "build-current", "shared")).not.toThrow();

    const incomplete = {
      ...report,
      routes: report.routes.filter(({ class: routeClass }) => routeClass !== "dashboard"),
    };
    expect(() => assertSavedReport(incomplete, "build-current", "all")).toThrow(
      "required route",
    );
  });

  it("test_authenticated_route_budgets_are_enforced", () => {
    const oversizedRoute = bundleInput();
    oversizedRoute.gzipBytes[HOME_CHUNK] = KIBIBYTE + 1;
    const routeReport = createRouteBundleReport(oversizedRoute);
    expect(routeReport.pass).toBe(false);
    expect(routeReport.routes.find(({ route }) => route === ROUTES.dashboard)).toMatchObject({
      incremental_bytes: KIBIBYTE + 1,
      limit_kib: 1,
      pass: false,
    });
    expect(() =>
      assertSavedReport(routeReport, "build-current", "incremental"),
    ).toThrow(ROUTES.dashboard);
    const forgedRouteReport = {
      ...routeReport,
      pass: true,
      routes: routeReport.routes.map((route) => ({ ...route, pass: true })),
    };
    expect(() =>
      assertSavedReport(forgedRouteReport, "build-current", "all"),
    ).toThrow("inconsistent limit verdict");

    const oversizedShared = bundleInput();
    oversizedShared.gzipBytes[SHELL_CHUNK] = KIBIBYTE;
    const sharedReport = createRouteBundleReport(oversizedShared);
    expect(sharedReport.pass).toBe(false);
    expect(() =>
      assertSavedReport(sharedReport, "build-current", "shared"),
    ).toThrow("authenticated_dashboard");
  });
});
