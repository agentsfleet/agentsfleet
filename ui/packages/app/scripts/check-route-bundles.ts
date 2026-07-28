import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";
import { loadEnvConfig } from "@next/env";

import {
  classifyAppPath,
  DASHBOARD_ENTRY,
  collectReferencedChunks,
  createRouteBundleReport,
  FRAMEWORK_ENTRY,
  parseClientReferenceManifest,
  type BundleBudgets,
  type BundleReportInput,
  type ClientReferenceManifest,
  type RouteBundleReport,
} from "./route-bundle-report";
import {
  assertSavedReport,
  LIMIT_SCOPE,
  type LimitScope,
} from "./saved-route-bundle-report";
import { assertRouteBuildFresh } from "./route-build-provenance";

const TEXT_ENCODING = "utf8";
const CLIENT_MANIFEST_SUFFIX = "_client-reference-manifest.js";
const BUILD_DIRECTORY = ".next";
const BUILD_ID_FILE = "BUILD_ID";
const BUILD_MANIFEST_FILE = "build-manifest.json";
const APP_ROUTES_MANIFEST_FILE = "app-path-routes-manifest.json";
const BUDGETS_FILE = "bundle-budgets.json";
const REPORT_FILE = "test-results/app-route-bundles.json";

function fail(message: string): never {
  throw new Error(`route bundle report: ${message}`);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
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
  await assertRouteBuildFresh(appRoot);
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
  loadEnvConfig(appRoot);
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
    assertSavedReport(report, input.buildId, scope, input.budgets);
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
