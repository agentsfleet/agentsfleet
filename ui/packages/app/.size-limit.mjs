import { access, readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const APP_ROOT = dirname(fileURLToPath(import.meta.url));
const BUILD_DIRECTORY = ".next";
const BUILD_ROOT = resolve(APP_ROOT, BUILD_DIRECTORY);
const BUILD_MANIFEST_FILE = "build-manifest.json";
const APP_ROUTES_MANIFEST_FILE = "app-path-routes-manifest.json";
const SERVER_APP_PATHS_MANIFEST_FILE = "server/app-paths-manifest.json";
const CLIENT_MANIFEST_SUFFIX = "_client-reference-manifest.js";
const CLIENT_MANIFEST_ASSIGNMENT = "] = ";
const CLIENT_CHUNK_PREFIX = "/_next/";
const STATIC_CHUNK_PREFIX = "static/chunks/";
const DASHBOARD_LAYOUT_SUFFIX = "/app/(dashboard)/layout";
const AUTH_APP_PATH = "/(auth)/";
const CLI_AUTH_APP_PATH = "/cli-auth/";
const DASHBOARD_APP_PATH = "/(dashboard)/";
const PAGE_APP_PATH_SUFFIX = "/page";
const FRAMEWORK_APP_PATH_PREFIX = "/_";
const GZIP = true;
const BUDGETS = {
  auth: "225 KiB",
  cliAuth: "240 KiB",
  dashboardShared: "250 KiB",
  routeIncremental: "100 KiB",
};
const ROUTE_CLASS = {
  auth: "auth",
  cliAuth: "cli_auth",
  dashboard: "dashboard",
};

function fail(message) {
  throw new Error(`route size config: ${message}`);
}

async function readJson(path) {
  try {
    return JSON.parse(await readFile(path, "utf8"));
  } catch (error) {
    fail(`${path} is absent or malformed: ${error.message}`);
  }
}

function normalizeChunk(value) {
  const chunk = value.startsWith(CLIENT_CHUNK_PREFIX)
    ? value.slice(CLIENT_CHUNK_PREFIX.length)
    : value;
  if (
    !chunk.startsWith(STATIC_CHUNK_PREFIX) ||
    !chunk.endsWith(".js") ||
    chunk.includes("..") ||
    chunk.includes("\\")
  ) {
    fail(`unsupported client chunk ${JSON.stringify(value)}`);
  }
  return chunk;
}

function uniqueChunks(values, label) {
  if (!Array.isArray(values) || values.some((value) => typeof value !== "string")) {
    fail(`${label} must be an array of chunk paths`);
  }
  const chunks = values.map(normalizeChunk);
  if (new Set(chunks).size !== chunks.length) {
    fail(`${label} contains duplicate chunk attribution`);
  }
  return chunks.sort((left, right) => left.localeCompare(right));
}

function requiredChunks(values, label) {
  const chunks = uniqueChunks(values, label);
  if (chunks.length === 0) fail(`${label} is empty`);
  return chunks;
}

function union(...groups) {
  return [...new Set(groups.flat())].sort((left, right) =>
    left.localeCompare(right),
  );
}

function intersection(groups) {
  const [first, ...rest] = groups;
  if (!first) fail("dashboard route entries are absent");
  return first.filter((file) => rest.every((group) => group.includes(file)));
}

function equalSets(left, right) {
  return left.length === right.length && left.every((file) => right.includes(file));
}

function routeClass(appPath) {
  if (appPath.startsWith(AUTH_APP_PATH)) return ROUTE_CLASS.auth;
  if (appPath.startsWith(CLI_AUTH_APP_PATH)) return ROUTE_CLASS.cliAuth;
  if (appPath.startsWith(DASHBOARD_APP_PATH)) return ROUTE_CLASS.dashboard;
  return null;
}

function isApplicationPage(appPath) {
  return (
    appPath.endsWith(PAGE_APP_PATH_SUFFIX) &&
    !appPath.startsWith(FRAMEWORK_APP_PATH_PREFIX)
  );
}

function parseClientManifest(source, route) {
  const marker = source.indexOf(CLIENT_MANIFEST_ASSIGNMENT);
  if (marker < 0) fail(`${route} client manifest assignment is absent`);
  try {
    const payload = source
      .slice(marker + CLIENT_MANIFEST_ASSIGNMENT.length)
      .trim()
      .replace(/;$/, "");
    const manifest = JSON.parse(payload);
    if (!manifest?.entryJSFiles || Array.isArray(manifest.entryJSFiles)) {
      fail(`${route} client manifest has no entryJSFiles object`);
    }
    return manifest;
  } catch (error) {
    fail(`${route} client manifest is malformed: ${error.message}`);
  }
}

function manifestFiles(manifest, route) {
  const entries = Object.entries(manifest.entryJSFiles);
  if (entries.length === 0) fail(`${route} client manifest has no entries`);
  return union(
    ...entries.map(([entry, files]) =>
      uniqueChunks(files, `${route} ${entry}`),
    ),
  );
}

function dashboardLayoutFiles(manifest, route) {
  const entries = Object.entries(manifest.entryJSFiles).filter(([entry]) =>
    entry.endsWith(DASHBOARD_LAYOUT_SUFFIX),
  );
  if (entries.length !== 1) {
    fail(`${route} expected one dashboard layout entry`);
  }
  return uniqueChunks(entries[0][1], `${route} dashboard layout`);
}

async function discoverRoutes(appPathRoutes) {
  const routes = [];
  let dashboardLayout;
  for (const [appPath, route] of Object.entries(appPathRoutes)) {
    if (!isApplicationPage(appPath)) continue;
    const classification = routeClass(appPath);
    if (!classification) fail(`unclassified application page ${appPath}`);
    if (typeof route !== "string" || route.trim() === "") {
      fail(`${appPath} has no public route`);
    }
    const manifestPath = resolve(
      BUILD_ROOT,
      "server/app",
      `${appPath.slice(1)}${CLIENT_MANIFEST_SUFFIX}`,
    );
    const source = await readFile(manifestPath, "utf8").catch(() =>
      fail(`${route} client manifest is absent`),
    );
    const manifest = parseClientManifest(source, route);
    if (classification === ROUTE_CLASS.dashboard) {
      const layoutFiles = dashboardLayoutFiles(manifest, route);
      if (dashboardLayout && !equalSets(dashboardLayout, layoutFiles)) {
        fail(`${route} disagrees with the dashboard shared entry`);
      }
      dashboardLayout = layoutFiles;
    }
    routes.push({
      route,
      classification,
      files: manifestFiles(manifest, route),
    });
  }
  const routeNames = routes.map(({ route }) => route);
  if (new Set(routeNames).size !== routeNames.length) {
    fail("authenticated public routes are duplicated");
  }
  for (const classification of Object.values(ROUTE_CLASS)) {
    if (!routes.some((route) => route.classification === classification)) {
      fail(`${classification} routes are absent`);
    }
  }
  return routes.sort((left, right) => left.route.localeCompare(right.route));
}

function classifiedPageAppPaths(manifest, label) {
  if (
    manifest === null ||
    typeof manifest !== "object" ||
    Array.isArray(manifest)
  ) {
    fail(`${label} must be an object`);
  }
  const appPaths = Object.keys(manifest).filter(isApplicationPage);
  for (const appPath of appPaths) {
    if (!routeClass(appPath)) {
      fail(`${label} contains unclassified application page ${appPath}`);
    }
  }
  return appPaths.sort((left, right) => left.localeCompare(right));
}

function assertSameAppPaths(appPathRoutes, serverAppPaths) {
  const routed = classifiedPageAppPaths(appPathRoutes, "app route manifest");
  const emitted = classifiedPageAppPaths(serverAppPaths, "server app path manifest");
  if (!equalSets(routed, emitted)) {
    fail("application page manifests disagree");
  }
}

function sizeCheck(name, files, limit) {
  if (files.length === 0) return null;
  return {
    name,
    path: files.map((file) => `${BUILD_DIRECTORY}/${file}`),
    limit,
    gzip: GZIP,
  };
}

async function assertFilesExist(checks) {
  const paths = union(...checks.flatMap(({ path }) => path));
  await Promise.all(
    paths.map(async (path) => {
      try {
        await access(resolve(APP_ROOT, path));
      } catch {
        fail(`${path} is absent`);
      }
    }),
  );
}

async function createConfig() {
  const [buildManifest, appPathRoutes, serverAppPaths] = await Promise.all([
    readJson(resolve(BUILD_ROOT, BUILD_MANIFEST_FILE)),
    readJson(resolve(BUILD_ROOT, APP_ROUTES_MANIFEST_FILE)),
    readJson(resolve(BUILD_ROOT, SERVER_APP_PATHS_MANIFEST_FILE)),
  ]);
  assertSameAppPaths(appPathRoutes, serverAppPaths);
  const frameworkFiles = requiredChunks(
    buildManifest.rootMainFiles,
    "framework runtime",
  );
  const routes = await discoverRoutes(appPathRoutes);
  const dashboardRoutes = routes.filter(
    ({ classification }) => classification === ROUTE_CLASS.dashboard,
  );
  const sharedFiles = union(
    frameworkFiles,
    intersection(dashboardRoutes.map(({ files }) => files)),
  );
  const sharedSet = new Set(sharedFiles);
  const checks = [
    sizeCheck("framework runtime", frameworkFiles),
    sizeCheck("authenticated dashboard shared", sharedFiles, BUDGETS.dashboardShared),
  ];
  for (const route of routes) {
    if (route.classification === ROUTE_CLASS.dashboard) {
      const files = union(frameworkFiles, route.files).filter(
        (file) => !sharedSet.has(file),
      );
      checks.push(
        sizeCheck(`${route.route} incremental`, files, BUDGETS.routeIncremental),
      );
    } else {
      const limit =
        route.classification === ROUTE_CLASS.auth
          ? BUDGETS.auth
          : BUDGETS.cliAuth;
      checks.push(
        sizeCheck(`${route.route} total`, union(frameworkFiles, route.files), limit),
      );
    }
  }
  const activeChecks = checks.filter(Boolean);
  await assertFilesExist(activeChecks);
  return activeChecks;
}

export default await createConfig();
