import { describe, expect, it } from "vitest";

import {
  collectReferencedChunks,
  createRouteBundleReport,
  parseClientReferenceManifest,
  type BundleBudgets,
  type BundleReportInput,
  type ClientReferenceManifest,
} from "./route-bundle-report";

const PROJECT_APP = "[project]/ui/packages/app/app";
const FRAMEWORK_CHUNK = "static/chunks/framework.js";
const ROOT_CHUNK = "static/chunks/root.js";
const SHELL_CHUNK = "static/chunks/shell.js";
const HOME_CHUNK = "static/chunks/home.js";
const MODELS_CHUNK = "static/chunks/models.js";
const AUTH_CHUNK = "static/chunks/auth.js";
const CLI_CHUNK = "static/chunks/cli.js";
const DASHBOARD_PATH = "/(dashboard)/page";
const MODELS_PATH = "/(dashboard)/admin/models/page";
const AUTH_PATH = "/(auth)/sign-in/page";
const CLI_PATH = "/cli-auth/[session_id]/page";
const DASHBOARD_ROUTE = "/";
const MODELS_ROUTE = "/admin/models";
const AUTH_ROUTE = "/sign-in";
const CLI_ROUTE = "/cli-auth/[session_id]";

const BUDGETS: BundleBudgets = {
  auth_total_kib: 1,
  cli_auth_total_kib: 1,
  dashboard_shared_total_kib: 1,
  route_incremental_kib: 1,
  required_routes: [
    DASHBOARD_ROUTE,
    MODELS_ROUTE,
    AUTH_ROUTE,
    CLI_ROUTE,
  ],
};

function manifest(
  appPath: string,
  files: string[],
  dashboard = false,
): ClientReferenceManifest {
  const entryJSFiles: Record<string, string[]> = {
    [`${PROJECT_APP}/layout`]: [ROOT_CHUNK],
    [`${PROJECT_APP}${appPath}`]: files,
  };
  if (dashboard) {
    entryJSFiles[`${PROJECT_APP}/(dashboard)/layout`] = [
      ROOT_CHUNK,
      SHELL_CHUNK,
    ];
  }
  return { entryJSFiles };
}

function validInput(): BundleReportInput {
  return {
    buildId: "current",
    buildManifest: {
      rootMainFiles: [`/_next/${FRAMEWORK_CHUNK}`],
    },
    appPathRoutes: {
      [DASHBOARD_PATH]: DASHBOARD_ROUTE,
      [MODELS_PATH]: MODELS_ROUTE,
      [AUTH_PATH]: AUTH_ROUTE,
      [CLI_PATH]: CLI_ROUTE,
    },
    clientManifests: {
      [DASHBOARD_PATH]: manifest(
        DASHBOARD_PATH,
        [ROOT_CHUNK, SHELL_CHUNK, HOME_CHUNK],
        true,
      ),
      [MODELS_PATH]: manifest(
        MODELS_PATH,
        [ROOT_CHUNK, SHELL_CHUNK, MODELS_CHUNK],
        true,
      ),
      [AUTH_PATH]: manifest(AUTH_PATH, [ROOT_CHUNK, AUTH_CHUNK]),
      [CLI_PATH]: manifest(CLI_PATH, [ROOT_CHUNK, CLI_CHUNK]),
    },
    gzipBytes: Object.fromEntries(
      [
        FRAMEWORK_CHUNK,
        ROOT_CHUNK,
        SHELL_CHUNK,
        HOME_CHUNK,
        MODELS_CHUNK,
        AUTH_CHUNK,
        CLI_CHUNK,
      ].map((chunk) => [chunk, 10]),
    ),
    budgets: structuredClone(BUDGETS),
  };
}

function expectInvalid(
  mutate: (input: BundleReportInput) => void,
  message: string | RegExp,
) {
  const input = validInput();
  mutate(input);
  expect(() => createRouteBundleReport(input)).toThrow(message);
}

describe("route bundle report input validation", () => {
  it("collects every referenced chunk once", () => {
    expect(collectReferencedChunks(validInput())).toEqual([
      AUTH_CHUNK,
      CLI_CHUNK,
      FRAMEWORK_CHUNK,
      HOME_CHUNK,
      MODELS_CHUNK,
      ROOT_CHUNK,
      SHELL_CHUNK,
    ]);
  });

  it.each([
    ["", "build ID is empty"],
    ["   ", "build ID is empty"],
  ])("rejects an empty build ID", (buildId, message) => {
    expectInvalid((input) => {
      input.buildId = buildId;
    }, message);
  });

  it.each([
    ["auth_total_kib", "one"],
    ["cli_auth_total_kib", 1.5],
    ["route_incremental_kib", 0],
  ])("rejects an invalid numeric budget", (field, value) => {
    expectInvalid((input) => {
      Object.assign(input.budgets, { [field]: value });
    }, /must be a positive integer/);
  });

  it("rejects malformed required-route and framework lists", () => {
    expectInvalid((input) => {
      input.budgets.required_routes = "all" as never;
    }, "required_routes must be an array of strings");
    expectInvalid((input) => {
      input.budgets.required_routes = [DASHBOARD_ROUTE, 7 as never];
    }, "required_routes must be an array of strings");
    expectInvalid((input) => {
      input.budgets.required_routes = [DASHBOARD_ROUTE, DASHBOARD_ROUTE];
    }, "required_routes contains a duplicate");
    expectInvalid((input) => {
      input.buildManifest.rootMainFiles = "framework" as never;
    }, "rootMainFiles must be an array of strings");
    expectInvalid((input) => {
      input.buildManifest.rootMainFiles = [];
    }, "framework runtime is empty");
  });

  it.each([
    ["vendor.js", "unsafe or unsupported"],
    ["static/chunks/vendor.css", "unsafe or unsupported"],
    ["static/chunks/../vendor.js", "unsafe or unsupported"],
    ["static/chunks\\vendor.js", "unsafe or unsupported"],
  ])("rejects an unsafe client chunk path", (chunk, message) => {
    expectInvalid((input) => {
      input.buildManifest.rootMainFiles = [chunk];
    }, message);
  });

  it("rejects duplicate chunk attribution", () => {
    expectInvalid((input) => {
      input.buildManifest.rootMainFiles = [
        FRAMEWORK_CHUNK,
        FRAMEWORK_CHUNK,
      ];
    }, "duplicate chunk attribution");
  });

  it("rejects invalid routes and missing manifests", () => {
    expectInvalid((input) => {
      input.appPathRoutes[DASHBOARD_PATH] = 7 as never;
    }, "has no valid public route");
    expectInvalid((input) => {
      input.appPathRoutes[DASHBOARD_PATH] = " ";
    }, "has no valid public route");
    expectInvalid((input) => {
      delete input.clientManifests[AUTH_PATH];
    }, "missing its client-reference manifest");
    const input = validInput();
    input.appPathRoutes["/_not-me/page"] = "/ignored";
    expect(createRouteBundleReport(input).routes).toHaveLength(4);
  });

  it("rejects empty, missing, duplicated, or disagreeing dashboard entries", () => {
    expectInvalid((input) => {
      input.clientManifests[DASHBOARD_PATH] = { entryJSFiles: {} };
    }, "has no client entries");
    expectInvalid((input) => {
      delete input.clientManifests[DASHBOARD_PATH]!.entryJSFiles[
        `${PROJECT_APP}/(dashboard)/layout`
      ];
    }, "expected one");
    expectInvalid((input) => {
      input.clientManifests[DASHBOARD_PATH]!.entryJSFiles[
        `duplicate${PROJECT_APP}/(dashboard)/layout`
      ] = [ROOT_CHUNK, SHELL_CHUNK];
    }, "observed 2");
    expectInvalid((input) => {
      input.clientManifests[MODELS_PATH]!.entryJSFiles[
        `${PROJECT_APP}/(dashboard)/layout`
      ] = [ROOT_CHUNK];
    }, "disagrees with the dashboard shared entry");
    expectInvalid((input) => {
      input.clientManifests[MODELS_PATH]!.entryJSFiles[
        `${PROJECT_APP}/(dashboard)/layout`
      ] = [ROOT_CHUNK, "static/chunks/other-shell.js"];
    }, "disagrees with the dashboard shared entry");
  });

  it("rejects a missing required route or dashboard class", () => {
    expectInvalid((input) => {
      input.budgets.required_routes.push("/missing");
    }, "required route /missing is absent");
    expectInvalid((input) => {
      input.budgets.required_routes = [AUTH_ROUTE, CLI_ROUTE];
      delete input.appPathRoutes[DASHBOARD_PATH];
      delete input.appPathRoutes[MODELS_PATH];
    }, "dashboard shared entry is absent");
  });

  it.each([
    ["large", "has no valid gzip measurement"],
    [1.5, "has no valid gzip measurement"],
    [-1, "has no valid gzip measurement"],
  ])("rejects an invalid gzip measurement", (measurement, message) => {
    expectInvalid((input) => {
      input.gzipBytes[HOME_CHUNK] = measurement as never;
    }, message);
  });
});

describe("client-reference manifest parsing", () => {
  it("parses an assignment with or without a semicolon", () => {
    const payload = JSON.stringify({
      entryJSFiles: { entry: [ROOT_CHUNK] },
    });
    expect(
      parseClientReferenceManifest(`globalThis.__RSC_MANIFEST.x] = ${payload};`),
    ).toEqual({ entryJSFiles: { entry: [ROOT_CHUNK] } });
    expect(
      parseClientReferenceManifest(`globalThis.__RSC_MANIFEST.x] = ${payload}`),
    ).toEqual({ entryJSFiles: { entry: [ROOT_CHUNK] } });
  });

  it.each([
    ["no assignment", "assignment is absent"],
    ["x] = {", "payload is malformed"],
    ["x] = null", "has no entryJSFiles object"],
    ["x] = []", "has no entryJSFiles object"],
    ["x] = 7", "has no entryJSFiles object"],
    ['x] = {"entryJSFiles":[]}', "has no entryJSFiles object"],
    ['x] = {"entryJSFiles":{"entry":"chunk"}}', "must be an array of strings"],
  ])("rejects malformed manifest input", (source, message) => {
    expect(() => parseClientReferenceManifest(source)).toThrow(message);
  });
});
