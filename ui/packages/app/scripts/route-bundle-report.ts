export const KIBIBYTE = 1024;
export const REPORT_SCHEMA_VERSION = 1;
export const COMPRESSION = "gzip";
export const FRAMEWORK_ENTRY = "framework_runtime";
export const DASHBOARD_ENTRY = "authenticated_dashboard";
const TYPE_STRING = "string";
const TYPE_NUMBER = "number";
const REQUIRED_ROUTES_FIELD = "required_routes";
const isNumber = (value: unknown): value is number => typeof value === TYPE_NUMBER;
const CLIENT_CHUNK_PREFIX = "/_next/";
const STATIC_CHUNK_PREFIX = "static/chunks/";
const CLIENT_MANIFEST_ASSIGNMENT = "] = ";
const DASHBOARD_APP_PATH = "/(dashboard)/";
const DASHBOARD_LAYOUT_SUFFIX = "/ui/packages/app/app/(dashboard)/layout";
const AUTH_APP_PATH = "/(auth)/";
const CLI_AUTH_APP_PATH = "/cli-auth/";
export const BUNDLE_CLASS = {
  auth: "auth",
  cliAuth: "cli_auth",
  dashboard: "dashboard",
} as const;
export type BundleClass = (typeof BUNDLE_CLASS)[keyof typeof BUNDLE_CLASS];
export interface BundleBudgets {
  auth_total_kib: number;
  cli_auth_total_kib: number;
  dashboard_shared_total_kib: number;
  route_incremental_kib: number;
  required_routes: string[];
}
export interface ClientReferenceManifest {
  entryJSFiles: Record<string, string[]>;
}

export interface BundleReportInput {
  buildId: string;
  buildManifest: { rootMainFiles: string[] };
  appPathRoutes: Record<string, string>;
  clientManifests: Record<string, ClientReferenceManifest>;
  gzipBytes: Record<string, number>;
  budgets: BundleBudgets;
}

interface RouteSource {
  route: string;
  class: BundleClass;
  files: string[];
}

interface DiscoveredBundles {
  frameworkFiles: string[];
  dashboardSharedFiles: string[];
  routes: RouteSource[];
}

interface RouteReport {
  route: string;
  class: BundleClass;
  initial_bytes: number;
  incremental_bytes: number;
  initial_kib: number;
  incremental_kib: number;
  limit_kib: number;
  pass: boolean;
}

export interface RouteBundleReport {
  schema_version: typeof REPORT_SCHEMA_VERSION;
  build_id: string;
  compression: typeof COMPRESSION;
  framework_bytes: number;
  limits: Omit<BundleBudgets, typeof REQUIRED_ROUTES_FIELD>;
  shared: Array<{ entry: string; bytes: number; kib: number }>;
  routes: RouteReport[];
  pass: boolean;
}

function fail(message: string): never {
  throw new Error(`route bundle report: ${message}`);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function requireStringArray(value: unknown, label: string): string[] {
  if (!Array.isArray(value) || value.some((item) => typeof item !== TYPE_STRING)) {
    fail(`${label} must be an array of strings`);
  }
  return value;
}

function normalizeChunkPath(value: string): string {
  const normalized = value.startsWith(CLIENT_CHUNK_PREFIX)
    ? value.slice(CLIENT_CHUNK_PREFIX.length)
    : value;
  if (
    !normalized.startsWith(STATIC_CHUNK_PREFIX) ||
    !normalized.endsWith(".js") ||
    normalized.includes("..") ||
    normalized.includes("\\")
  ) {
    fail(`unsafe or unsupported client chunk ${JSON.stringify(value)}`);
  }
  return normalized;
}

function uniqueChunks(values: string[], label: string): string[] {
  const chunks = values.map(normalizeChunkPath);
  if (new Set(chunks).size !== chunks.length) {
    fail(`${label} contains duplicate chunk attribution`);
  }
  return [...chunks].sort();
}

function findEntryFiles(
  manifest: ClientReferenceManifest,
  suffix: string,
  label: string,
): string[] {
  const matches = Object.entries(manifest.entryJSFiles).filter(([entry]) =>
    entry.endsWith(suffix),
  );
  if (matches.length !== 1) {
    fail(`${label} expected one ${suffix} entry, observed ${matches.length}`);
  }
  const match = matches[0] as [string, string[]];
  return uniqueChunks(match[1], label);
}

function allEntryFiles(
  manifest: ClientReferenceManifest,
  label: string,
): string[] {
  const entries = Object.entries(manifest.entryJSFiles);
  if (entries.length === 0) fail(`${label} has no client entries`);
  return union(
    ...entries.map(([entry, files]) => uniqueChunks(files, `${label} ${entry}`)),
  );
}

export function classifyAppPath(appPath: string): BundleClass | null {
  if (appPath.startsWith(AUTH_APP_PATH)) return BUNDLE_CLASS.auth;
  if (appPath.startsWith(CLI_AUTH_APP_PATH)) return BUNDLE_CLASS.cliAuth;
  if (appPath.startsWith(DASHBOARD_APP_PATH)) return BUNDLE_CLASS.dashboard;
  return null;
}

function equalSets(left: string[], right: string[]): boolean {
  return left.length === right.length && left.every((value) => right.includes(value));
}

function union(...groups: string[][]): string[] {
  return [...new Set(groups.flat())].sort();
}

function intersection(groups: string[][]): string[] {
  const first = groups[0] as string[];
  return first.filter((file) => groups.every((group) => group.includes(file)));
}

type ReportCalculationInput = Omit<BundleReportInput, "gzipBytes">;

function discoverBundles(input: ReportCalculationInput): DiscoveredBundles {
  if (input.buildId.trim() === "") fail("build ID is empty");
  for (const [name, value] of Object.entries(input.budgets)) {
    if (name === REQUIRED_ROUTES_FIELD) continue;
    if (!isNumber(value) || !Number.isSafeInteger(value) || value <= 0) {
      fail(`${name} must be a positive integer`);
    }
  }
  const requiredRoutes = requireStringArray(
    input.budgets.required_routes,
    REQUIRED_ROUTES_FIELD,
  );
  if (new Set(requiredRoutes).size !== requiredRoutes.length) {
    fail("required_routes contains a duplicate");
  }
  const frameworkFiles = uniqueChunks(
    requireStringArray(input.buildManifest.rootMainFiles, "rootMainFiles"),
    "framework runtime",
  );
  if (frameworkFiles.length === 0) fail("framework runtime is empty");

  let dashboardLayoutFiles: string[] | undefined;
  const routes: RouteSource[] = [];
  for (const [appPath, route] of Object.entries(input.appPathRoutes)) {
    if (typeof route !== TYPE_STRING || route.trim() === "") {
      fail(`${appPath} has no valid public route`);
    }
    const routeClass = classifyAppPath(appPath);
    if (routeClass === null) continue;
    const manifest = input.clientManifests[appPath];
    if (!manifest) fail(`${route} is missing its client-reference manifest`);
    const files = allEntryFiles(manifest, `${route} route`);
    if (routeClass === BUNDLE_CLASS.dashboard) {
      const layoutFiles = findEntryFiles(
        manifest,
        DASHBOARD_LAYOUT_SUFFIX,
        `${route} dashboard layout`,
      );
      if (dashboardLayoutFiles && !equalSets(dashboardLayoutFiles, layoutFiles)) {
        fail(`${route} disagrees with the dashboard shared entry`);
      }
      dashboardLayoutFiles = layoutFiles;
    }
    routes.push({ route, class: routeClass, files });
  }

  const discoveredRoutes = new Set(routes.map(({ route }) => route));
  for (const requiredRoute of requiredRoutes) {
    if (!discoveredRoutes.has(requiredRoute)) {
      fail(`required route ${requiredRoute} is absent`);
    }
  }
  if (!dashboardLayoutFiles) fail("dashboard shared entry is absent");
  const dashboardFiles = routes
    .filter(({ class: routeClass }) => routeClass === BUNDLE_CLASS.dashboard)
    .map(({ files }) => files);
  const dashboardCommonFiles = intersection(dashboardFiles);
  return {
    frameworkFiles,
    dashboardSharedFiles: union(frameworkFiles, dashboardCommonFiles),
    routes: routes.sort((left, right) => left.route.localeCompare(right.route)),
  };
}

export function parseClientReferenceManifest(
  source: string,
): ClientReferenceManifest {
  const markerIndex = source.indexOf(CLIENT_MANIFEST_ASSIGNMENT);
  if (markerIndex < 0) fail("client-reference manifest assignment is absent");
  const encoded = source
    .slice(markerIndex + CLIENT_MANIFEST_ASSIGNMENT.length)
    .trim()
    .replace(/;$/, "");
  let parsed: unknown;
  try {
    parsed = JSON.parse(encoded);
  } catch {
    fail("client-reference manifest payload is malformed");
  }
  if (!isRecord(parsed) || !isRecord(parsed.entryJSFiles)) {
    fail("client-reference manifest has no entryJSFiles object");
  }
  for (const [entry, files] of Object.entries(parsed.entryJSFiles)) {
    requireStringArray(files, `${entry} chunks`);
  }
  return parsed as unknown as ClientReferenceManifest;
}

export function collectReferencedChunks(
  input: ReportCalculationInput,
): string[] {
  const discovered = discoverBundles(input);
  return union(
    discovered.frameworkFiles,
    discovered.dashboardSharedFiles,
    ...discovered.routes.map(({ files }) => files),
  );
}

function bytesFor(files: string[], gzipBytes: Record<string, number>): number {
  return files.reduce((total, file) => {
    const bytes = gzipBytes[file];
    if (!isNumber(bytes) || !Number.isSafeInteger(bytes) || bytes < 0) {
      fail(`${file} has no valid gzip measurement`);
    }
    return total + bytes;
  }, 0);
}

export function displayKib(bytes: number): number {
  return Math.round((bytes / KIBIBYTE) * 10) / 10;
}

export function createRouteBundleReport(
  input: BundleReportInput,
): RouteBundleReport {
  const discovered = discoverBundles(input);
  const frameworkSet = new Set(discovered.frameworkFiles);
  const sharedSet = new Set(discovered.dashboardSharedFiles);
  const frameworkBytes = bytesFor(discovered.frameworkFiles, input.gzipBytes);
  const sharedBytes = bytesFor(discovered.dashboardSharedFiles, input.gzipBytes);
  const sharedPass =
    sharedBytes <= input.budgets.dashboard_shared_total_kib * KIBIBYTE;

  const routes = discovered.routes.map<RouteReport>((source) => {
    const initialFiles = union(discovered.frameworkFiles, source.files);
    const baseline =
      source.class === BUNDLE_CLASS.dashboard ? sharedSet : frameworkSet;
    const incrementalFiles = initialFiles.filter((file) => !baseline.has(file));
    const initialBytes = bytesFor(initialFiles, input.gzipBytes);
    const incrementalBytes = bytesFor(incrementalFiles, input.gzipBytes);
    const limitKib =
      source.class === BUNDLE_CLASS.auth
        ? input.budgets.auth_total_kib
        : source.class === BUNDLE_CLASS.cliAuth
          ? input.budgets.cli_auth_total_kib
          : input.budgets.route_incremental_kib;
    const measuredBytes =
      source.class === BUNDLE_CLASS.dashboard ? incrementalBytes : initialBytes;
    return {
      route: source.route,
      class: source.class,
      initial_bytes: initialBytes,
      incremental_bytes: incrementalBytes,
      initial_kib: displayKib(initialBytes),
      incremental_kib: displayKib(incrementalBytes),
      limit_kib: limitKib,
      pass: measuredBytes <= limitKib * KIBIBYTE,
    };
  });

  return {
    schema_version: REPORT_SCHEMA_VERSION,
    build_id: input.buildId,
    compression: COMPRESSION,
    framework_bytes: frameworkBytes,
    limits: {
      auth_total_kib: input.budgets.auth_total_kib,
      cli_auth_total_kib: input.budgets.cli_auth_total_kib,
      dashboard_shared_total_kib: input.budgets.dashboard_shared_total_kib,
      route_incremental_kib: input.budgets.route_incremental_kib,
    },
    shared: [
      {
        entry: FRAMEWORK_ENTRY,
        bytes: frameworkBytes,
        kib: displayKib(frameworkBytes),
      },
      {
        entry: DASHBOARD_ENTRY,
        bytes: sharedBytes,
        kib: displayKib(sharedBytes),
      },
    ],
    routes,
    pass: sharedPass && routes.every(({ pass }) => pass),
  };
}
