import {
  BUNDLE_CLASS,
  COMPRESSION,
  DASHBOARD_ENTRY,
  displayKib,
  FRAMEWORK_ENTRY,
  KIBIBYTE,
  REPORT_SCHEMA_VERSION,
  type BundleBudgets,
  type RouteBundleReport,
} from "./route-bundle-report";

const TYPE_BOOLEAN = "boolean";

export const LIMIT_SCOPE = {
  all: "all",
  shared: "shared",
  incremental: "incremental",
} as const;

export type LimitScope = (typeof LIMIT_SCOPE)[keyof typeof LIMIT_SCOPE];

function fail(message: string): never {
  throw new Error(`route bundle report: ${message}`);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isString(value: unknown): value is string {
  return typeof value === "string";
}

function isSafeByteCount(value: unknown): value is number {
  return Number.isSafeInteger(value) && Number(value) >= 0;
}

function isValidKib(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value) && value >= 0;
}

function isPositiveInteger(value: unknown): value is number {
  return Number.isSafeInteger(value) && Number(value) > 0;
}

const LIMIT_FIELDS = [
  "auth_total_kib",
  "cli_auth_total_kib",
  "dashboard_shared_total_kib",
  "route_incremental_kib",
] as const;

function requireTrustedBudgets(value: BundleBudgets): BundleBudgets {
  if (
    !isRecord(value) ||
    !LIMIT_FIELDS.every((field) => isPositiveInteger(value[field])) ||
    !Array.isArray(value.required_routes) ||
    value.required_routes.length === 0 ||
    value.required_routes.some(
      (route) => !isString(route) || route.trim() === "",
    ) ||
    new Set(value.required_routes).size !== value.required_routes.length
  ) {
    fail("current bundle budgets have an invalid shape");
  }
  return value;
}

function requireReport(value: unknown): RouteBundleReport {
  if (
    !isRecord(value) ||
    value.schema_version !== REPORT_SCHEMA_VERSION ||
    value.compression !== COMPRESSION ||
    !isString(value.build_id) ||
    value.build_id.trim() === "" ||
    !Array.isArray(value.shared) ||
    !Array.isArray(value.routes) ||
    !isRecord(value.limits) ||
    !isSafeByteCount(value.framework_bytes) ||
    typeof value.pass !== TYPE_BOOLEAN
  ) {
    fail("saved report has an invalid shape");
  }
  const limits = value.limits;
  const sharedValid = value.shared.every(
    (item) =>
      isRecord(item) &&
      isString(item.entry) &&
      isSafeByteCount(item.bytes) &&
      isValidKib(item.kib) &&
      item.kib === displayKib(item.bytes),
  );
  const routesValid = value.routes.every(
    (item) => {
      if (
        !isRecord(item) ||
        !isString(item.route) ||
        item.route.trim() === "" ||
        ![
          BUNDLE_CLASS.auth,
          BUNDLE_CLASS.cliAuth,
          BUNDLE_CLASS.dashboard,
        ].includes(
          String(item.class) as (typeof BUNDLE_CLASS)[keyof typeof BUNDLE_CLASS],
        ) ||
        typeof item.pass !== TYPE_BOOLEAN ||
        !isSafeByteCount(item.initial_bytes) ||
        !isSafeByteCount(item.incremental_bytes) ||
        !isValidKib(item.initial_kib) ||
        !isValidKib(item.incremental_kib) ||
        !isPositiveInteger(item.limit_kib)
      ) {
        return false;
      }
      return (
        item.incremental_bytes <= item.initial_bytes &&
        item.initial_kib === displayKib(item.initial_bytes) &&
        item.incremental_kib === displayKib(item.incremental_bytes)
      );
    },
  );
  const limitsValid = LIMIT_FIELDS.every((field) =>
    isPositiveInteger(limits[field]),
  );
  if (!sharedValid || !routesValid || !limitsValid) {
    fail("saved report has invalid shared, route, or limit fields");
  }
  return value as unknown as RouteBundleReport;
}

export function assertSavedReport(
  value: unknown,
  currentBuildId: string,
  scope: LimitScope,
  currentBudgets: BundleBudgets,
): void {
  const report = requireReport(value);
  const trustedBudgets = requireTrustedBudgets(currentBudgets);
  if (report.build_id !== currentBuildId) {
    fail(
      `stale report: expected build ${currentBuildId}, observed ${report.build_id}`,
    );
  }
  const shared = report.shared.find(
    ({ entry }) => entry === DASHBOARD_ENTRY,
  );
  const framework = report.shared.find(
    ({ entry }) => entry === FRAMEWORK_ENTRY,
  );
  const sharedEntries = report.shared.map(({ entry }) => entry);
  if (
    report.shared.length !== 2 ||
    new Set(sharedEntries).size !== report.shared.length ||
    !shared ||
    !framework ||
    framework.bytes !== report.framework_bytes ||
    shared.bytes < framework.bytes
  ) {
    fail(
      "authenticated_dashboard shared entries are incomplete or inconsistent",
    );
  }
  const sharedPass =
    shared.bytes <= trustedBudgets.dashboard_shared_total_kib * KIBIBYTE;
  for (const field of LIMIT_FIELDS) {
    if (report.limits[field] !== trustedBudgets[field]) {
      fail(`${field} does not match the current bundle budget`);
    }
  }
  for (const routeClass of Object.values(BUNDLE_CLASS)) {
    if (!report.routes.some((route) => route.class === routeClass)) {
      fail(`required route class ${routeClass} is absent`);
    }
  }
  for (const requiredRoute of trustedBudgets.required_routes) {
    if (!report.routes.some(({ route }) => route === requiredRoute)) {
      fail(`required route ${requiredRoute} is absent`);
    }
  }
  const routeNames = report.routes.map(({ route }) => route);
  if (
    new Set(routeNames).size !== routeNames.length ||
    !routeNames.every(
      (route, index) => route === [...routeNames].sort()[index],
    )
  ) {
    fail("saved routes must be unique and sorted");
  }
  for (const route of report.routes) {
    const expectedLimit =
      route.class === BUNDLE_CLASS.auth
        ? trustedBudgets.auth_total_kib
        : route.class === BUNDLE_CLASS.cliAuth
          ? trustedBudgets.cli_auth_total_kib
          : trustedBudgets.route_incremental_kib;
    const measured =
      route.class === BUNDLE_CLASS.dashboard
        ? route.incremental_bytes
        : route.initial_bytes;
    const expectedPass = measured <= expectedLimit * KIBIBYTE;
    const minimumInitialBytes =
      route.class === BUNDLE_CLASS.dashboard
        ? shared.bytes
        : report.framework_bytes;
    if (
      route.initial_bytes !== minimumInitialBytes + route.incremental_bytes ||
      route.limit_kib !== expectedLimit ||
      route.pass !== expectedPass
    ) {
      fail(`${route.route} has an inconsistent limit verdict`);
    }
  }
  const expectedReportPass =
    sharedPass && report.routes.every(({ pass }) => pass);
  if (report.pass !== expectedReportPass) {
    fail("saved report has an inconsistent top-level verdict");
  }
  if (scope !== LIMIT_SCOPE.incremental && !sharedPass) {
    fail("authenticated_dashboard exceeds its shared limit");
  }
  if (scope !== LIMIT_SCOPE.shared) {
    const checkedRoutes =
      scope === LIMIT_SCOPE.incremental
        ? report.routes.filter(
            ({ class: routeClass }) => routeClass === BUNDLE_CLASS.dashboard,
          )
        : report.routes;
    const failed = checkedRoutes.find(({ pass }) => !pass);
    if (failed) fail(`${failed.route} exceeds its ${failed.limit_kib} KiB limit`);
  }
}
