import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

import {
  BUNDLE_CLASS,
  classifyAppPath,
  collectReferencedChunks,
  createRouteBundleReport,
  parseClientReferenceManifest,
  type BundleBudgets,
  type BundleReportInput,
  type ClientReferenceManifest,
  type RouteBundleReport,
} from "./route-bundle-report";

const KIBIBYTE = 1024;
const REPORT_SCHEMA_VERSION = 1;
const COMPRESSION = "gzip";
const TYPE_STRING = "string";
const TYPE_NUMBER = "number";
const TYPE_BOOLEAN = "boolean";
const TEXT_ENCODING = "utf8";
const FRAMEWORK_ENTRY = "framework_runtime";
const DASHBOARD_ENTRY = "authenticated_dashboard";
const LIMIT_SCOPE = {
  all: "all",
  shared: "shared",
  incremental: "incremental",
} as const;
const CLIENT_MANIFEST_SUFFIX = "_client-reference-manifest.js";
const BUILD_DIRECTORY = ".next";
const BUILD_ID_FILE = "BUILD_ID";
const BUILD_MANIFEST_FILE = "build-manifest.json";
const APP_ROUTES_MANIFEST_FILE = "app-path-routes-manifest.json";
const BUDGETS_FILE = "bundle-budgets.json";
const REPORT_FILE = "test-results/app-route-bundles.json";

export type LimitScope = (typeof LIMIT_SCOPE)[keyof typeof LIMIT_SCOPE];

function fail(message: string): never {
  throw new Error(`route bundle report: ${message}`);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function requireReport(value: unknown): RouteBundleReport {
  const numberFieldsAreValid = (item: Record<string, unknown>, fields: string[]) =>
    fields.every((field) => typeof item[field] === TYPE_NUMBER);
  if (
    !isRecord(value) ||
    value.schema_version !== REPORT_SCHEMA_VERSION ||
    value.compression !== COMPRESSION ||
    typeof value.build_id !== TYPE_STRING ||
    !Array.isArray(value.shared) ||
    !Array.isArray(value.routes) ||
    !isRecord(value.limits) ||
    typeof value.framework_bytes !== TYPE_NUMBER ||
    typeof value.pass !== TYPE_BOOLEAN
  ) {
    fail("saved report has an invalid shape");
  }
  const limits = value.limits;
  if (!isRecord(limits)) fail("saved report limits have an invalid shape");
  const sharedValid = value.shared.every(
    (item) =>
      isRecord(item) &&
      typeof item.entry === TYPE_STRING &&
      numberFieldsAreValid(item, ["bytes", "kib"]),
  );
  const routesValid = value.routes.every(
    (item) =>
      isRecord(item) &&
      typeof item.route === TYPE_STRING &&
      [BUNDLE_CLASS.auth, BUNDLE_CLASS.cliAuth, BUNDLE_CLASS.dashboard].includes(
        String(item.class) as (typeof BUNDLE_CLASS)[keyof typeof BUNDLE_CLASS],
      ) &&
      typeof item.pass === TYPE_BOOLEAN &&
      numberFieldsAreValid(item, [
        "initial_bytes",
        "incremental_bytes",
        "initial_kib",
        "incremental_kib",
        "limit_kib",
      ]),
  );
  const limitsValid = [
    "auth_total_kib",
    "cli_auth_total_kib",
    "dashboard_shared_total_kib",
    "route_incremental_kib",
  ].every((field) => typeof limits[field] === TYPE_NUMBER);
  if (!sharedValid || !routesValid || !limitsValid) {
    fail("saved report has invalid shared, route, or limit fields");
  }
  return value as unknown as RouteBundleReport;
}

export function assertSavedReport(
  value: unknown,
  currentBuildId: string,
  scope: LimitScope,
  requiredRoutes: string[] = [],
): void {
  const report = requireReport(value);
  if (report.build_id !== currentBuildId) {
    fail(`stale report: expected build ${currentBuildId}, observed ${report.build_id}`);
  }
  const shared = report.shared.find(
    ({ entry }) => entry === DASHBOARD_ENTRY,
  );
  const framework = report.shared.find(({ entry }) => entry === FRAMEWORK_ENTRY);
  if (
    !shared ||
    !framework ||
    framework.bytes !== report.framework_bytes ||
    shared.bytes > report.limits.dashboard_shared_total_kib * KIBIBYTE
  ) {
    fail("authenticated_dashboard shared entries are incomplete, inconsistent, or over limit");
  }
  for (const routeClass of Object.values(BUNDLE_CLASS)) {
    if (!report.routes.some((route) => route.class === routeClass)) {
      fail(`required route class ${routeClass} is absent`);
    }
  }
  for (const requiredRoute of requiredRoutes) {
    if (!report.routes.some(({ route }) => route === requiredRoute)) {
      fail(`required route ${requiredRoute} is absent`);
    }
  }
  const routeNames = report.routes.map(({ route }) => route);
  if (
    new Set(routeNames).size !== routeNames.length ||
    !routeNames.every((route, index) => route === [...routeNames].sort()[index])
  ) {
    fail("saved routes must be unique and sorted");
  }
  for (const route of report.routes) {
    const expectedLimit =
      route.class === BUNDLE_CLASS.auth
        ? report.limits.auth_total_kib
        : route.class === BUNDLE_CLASS.cliAuth
          ? report.limits.cli_auth_total_kib
          : report.limits.route_incremental_kib;
    const measured =
      route.class === BUNDLE_CLASS.dashboard
        ? route.incremental_bytes
        : route.initial_bytes;
    const expectedPass = measured <= expectedLimit * KIBIBYTE;
    if (route.limit_kib !== expectedLimit || route.pass !== expectedPass) {
      fail(`${route.route} has an inconsistent limit verdict`);
    }
  }
  const expectedReportPass = report.routes.every(({ pass }) => pass);
  if (report.pass !== expectedReportPass) {
    fail("saved report has an inconsistent top-level verdict");
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
  if (scope === LIMIT_SCOPE.all && !report.pass) {
    fail("saved report does not pass");
  }
}

async function readText(path: string): Promise<string> {
  try {
    return await readFile(path, TEXT_ENCODING);
  } catch {
    fail(`required input ${path} is absent or unreadable`);
  }
}

async function readJson(path: string): Promise<unknown> {
  try {
    return JSON.parse(await readText(path));
  } catch (error) {
    if (error instanceof SyntaxError) fail(`required JSON ${path} is malformed`);
    throw error;
  }
}

async function loadInput(
  appRoot: string,
): Promise<Omit<BundleReportInput, "gzipBytes">> {
  const buildRoot = resolve(appRoot, BUILD_DIRECTORY);
  const buildId = (await readText(resolve(buildRoot, BUILD_ID_FILE))).trim();
  const buildManifest = await readJson(resolve(buildRoot, BUILD_MANIFEST_FILE));
  const appPathRoutes = await readJson(
    resolve(buildRoot, APP_ROUTES_MANIFEST_FILE),
  );
  const budgets = await readJson(resolve(appRoot, BUDGETS_FILE));
  if (!isRecord(buildManifest) || !isRecord(appPathRoutes) || !isRecord(budgets)) {
    fail("build manifest, route manifest, or budgets have an invalid shape");
  }

  const clientManifests: Record<string, ClientReferenceManifest> = {};
  for (const appPath of Object.keys(appPathRoutes)) {
    if (classifyAppPath(appPath) === null) continue;
    const manifestPath = resolve(
      buildRoot,
      "server/app",
      `${appPath.slice(1)}${CLIENT_MANIFEST_SUFFIX}`,
    );
    clientManifests[appPath] = parseClientReferenceManifest(
      await readText(manifestPath),
    );
  }
  return {
    buildId,
    buildManifest: buildManifest as unknown as BundleReportInput["buildManifest"],
    appPathRoutes: appPathRoutes as Record<string, string>,
    clientManifests,
    budgets: budgets as unknown as BundleBudgets,
  };
}

async function measureChunks(
  buildRoot: string,
  chunks: string[],
): Promise<Record<string, number>> {
  const measurements = await Promise.all(
    chunks.map(async (chunk) => {
      try {
        const bytes = await readFile(resolve(buildRoot, chunk));
        return [chunk, gzipSync(bytes).byteLength] as const;
      } catch {
        fail(`referenced chunk ${chunk} is absent or unreadable`);
      }
    }),
  );
  return Object.fromEntries(measurements);
}

function printReport(report: RouteBundleReport): void {
  const framework = report.shared.find(({ entry }) => entry === FRAMEWORK_ENTRY);
  const dashboard = report.shared.find(
    ({ entry }) => entry === DASHBOARD_ENTRY,
  );
  if (!framework || !dashboard) fail("generated shared entries are incomplete");
  process.stdout.write(
    `framework_runtime ${framework.kib} KiB\n` +
      `authenticated_dashboard ${dashboard.kib} KiB / ` +
      `${report.limits.dashboard_shared_total_kib} KiB\n`,
  );
  for (const route of report.routes) {
    process.stdout.write(
      `${route.pass ? "PASS" : "FAIL"} ${route.route} ` +
        `${route.initial_kib} KiB total, ${route.incremental_kib} KiB incremental\n`,
    );
  }
}

async function main(args: string[]): Promise<void> {
  const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
  const reportPath = resolve(appRoot, "../../..", REPORT_FILE);
  const input = await loadInput(appRoot);
  const checkIndex = args.indexOf("--check");
  if (checkIndex >= 0) {
    const suppliedPath = args[checkIndex + 1];
    if (!suppliedPath) fail("--check requires a report path");
    const limitIndex = args.indexOf("--limit");
    const scope = (
      limitIndex >= 0 ? args[limitIndex + 1] : LIMIT_SCOPE.all
    ) as LimitScope;
    if (!Object.values(LIMIT_SCOPE).includes(scope)) {
      fail("--limit must be all, shared, or incremental");
    }
    const report = await readJson(resolve(process.cwd(), suppliedPath));
    assertSavedReport(report, input.buildId, scope, input.budgets.required_routes);
    process.stdout.write(`PASS ${scope} bundle check\n`);
    return;
  }
  if (args.length > 0) fail(`unknown arguments: ${args.join(" ")}`);

  const chunks = collectReferencedChunks(input);
  const gzipBytes = await measureChunks(resolve(appRoot, BUILD_DIRECTORY), chunks);
  const report = createRouteBundleReport({ ...input, gzipBytes });
  await mkdir(dirname(reportPath), { recursive: true });
  await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`, TEXT_ENCODING);
  printReport(report);
  if (!report.pass) fail(`one or more budgets failed; report written to ${reportPath}`);
}

if (import.meta.main) {
  main(process.argv.slice(2)).catch((error: unknown) => {
    const message = error instanceof Error ? error.message : String(error);
    process.stderr.write(`${message}\n`);
    process.exitCode = 1;
  });
}
