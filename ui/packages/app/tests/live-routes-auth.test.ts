// The same-origin event routes answer a signed-out request with the one
// registered 401 code, as the event-detail and steer routes do. An
// unregistered code reads as UZ-UNKNOWN to every error surface.

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ERROR_CODE } from "@/lib/errors";

const { getTokenFn } = vi.hoisted(() => ({ getTokenFn: vi.fn() }));

vi.mock("@clerk/nextjs/server", () => ({
  auth: () => Promise.resolve({ getToken: getTokenFn }),
}));

vi.mock("@/lib/api/client", () => ({
  API_ORIGIN: "https://api.example.test",
  request: vi.fn(),
}));

import { GET as workspaceEvents } from "../app/live/v1/workspaces/[workspaceId]/events/route";
import { GET as workspaceStream } from "../app/live/v1/workspaces/[workspaceId]/events/stream/route";
import { GET as fleetEvents } from "../app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/events/route";
import { GET as fleetStream } from "../app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/events/stream/route";

const HTTP_UNAUTHORIZED = 401;
const WORKSPACE_ID = "ws_1";
const FLEET_ID = "fleet_1";
const UNREGISTERED_AUTH_CODE = "UZ-401";
const APP_ROOT = join(__dirname, "..");
// Tests too: a test pinning the old code keeps it alive as an expectation.
const SCANNED_DIRS = ["app", "components", "lib", "tests"];
const SOURCE_FILE = /\.(ts|tsx)$/;

type Route = (req: Request, ctx: { params: Promise<Record<string, string>> }) => Promise<Response>;

const ROUTES: ReadonlyArray<readonly [string, Route, Record<string, string>]> = [
  ["workspace events", workspaceEvents as Route, { workspaceId: WORKSPACE_ID }],
  ["workspace stream", workspaceStream as Route, { workspaceId: WORKSPACE_ID }],
  ["fleet events", fleetEvents as Route, { workspaceId: WORKSPACE_ID, fleetId: FLEET_ID }],
  ["fleet stream", fleetStream as Route, { workspaceId: WORKSPACE_ID, fleetId: FLEET_ID }],
];

beforeEach(() => {
  vi.clearAllMocks();
});

describe("same-origin event routes — signed out", () => {
  it.each(ROUTES)("test_routes_answer_auth_401 — %s", async (_name, route, params) => {
    getTokenFn.mockResolvedValueOnce(null);
    const res = await route(new Request("http://localhost/live"), { params: Promise.resolve(params) });
    expect(res.status).toBe(HTTP_UNAUTHORIZED);
    const body = (await res.json()) as { error: string; code: string };
    expect(body.code).toBe(ERROR_CODE.AUTH_401);
  });

  it("test_no_unregistered_auth_code", () => {
    // This file names the code it looks for, and only this file may.
    const hits = SCANNED_DIRS.flatMap((dir) => sourceFiles(join(APP_ROOT, dir)))
      .filter((file) => file !== __filename)
      .filter((file) => readFileSync(file, "utf8").includes(`"${UNREGISTERED_AUTH_CODE}"`));
    expect(hits).toEqual([]);
  });
});

function sourceFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((entry) => {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) return sourceFiles(path);
    return SOURCE_FILE.test(entry) ? [path] : [];
  });
}
