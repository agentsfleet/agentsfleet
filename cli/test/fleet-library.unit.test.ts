// Unit coverage for src/commands/fleet_library.ts — the `agentsfleet library`
// gallery list. Exercises the table render, JSON mode, the empty gallery, both
// joinNames branches (credentials present vs none), and the invariant that
// matters most: the path it requests is the workspace gallery, which is what
// `install --library` resolves against.

import { describe, test, expect } from "bun:test";
import { Effect, Exit, Layer, Option, Redacted } from "effect";

import { libraryEffect } from "../src/commands/fleet_library.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient, type HttpRequestInput } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import { Workspaces } from "../src/services/workspaces.ts";

const TOKEN = "test.jwt.templates";
const WS_ID = "019febb0-272b-78aa-aa3a-03f92e543014";

interface TableCapture {
  columns: unknown;
  rows: unknown;
}

const makeLayer = (
  captured: string[],
  tables: TableCapture[],
  jsonMode: boolean,
  requests: HttpRequestInput[],
  response: unknown,
) =>
  Layer.mergeAll(
    Layer.succeed(CliConfig, {
      apiUrl: "https://api.unit-test.local",
      dashboardUrl: "https://dash.unit-test.local",
      accessToken: Option.none(),
      jsonMode,
      noOpen: false,
      telemetryPosthogKey: "phc_unit",
      telemetryPosthogHost: "https://us.i.posthog.com",
    }),
    Layer.succeed(Credentials, {
      getAccessToken: Effect.sync(() => Option.some(Redacted.make(TOKEN))),
      snapshot: Effect.succeed({ accessToken: Option.none(), savedAt: null, sessionId: null, apiUrl: null, credentialId: null }),
      saveAccessToken: () => Effect.void,
      clearAccessToken: Effect.void,
    }),
    Layer.succeed(HttpClient, {
      request: <T>(input: HttpRequestInput) =>
        Effect.sync(() => {
          requests.push(input);
          return response as T;
        }),
    }),
    Layer.succeed(Workspaces, {
      load: Effect.succeed({ current_workspace_id: WS_ID, items: [] }),
      save: () => Effect.void,
    }),
    Layer.succeed(Output, {
      intro: (m) => Effect.sync(() => { captured.push(m); }),
      info: (m) => Effect.sync(() => { captured.push(m); }),
      success: (m) => Effect.sync(() => { captured.push(m); }),
      warn: (m) => Effect.sync(() => { captured.push(m); }),
      error: (m) => Effect.sync(() => { captured.push(m); }),
      outro: (m) => Effect.sync(() => { captured.push(m); }),
      printJson: (p) => Effect.sync(() => { captured.push(JSON.stringify(p)); }),
      printJsonErr: (p) => Effect.sync(() => { captured.push(JSON.stringify(p)); }),
      printKeyValue: () => Effect.void,
      printSection: () => Effect.void,
      printTable: (columns, rows) =>
        Effect.sync(() => { tables.push({ columns, rows }); }),
    }),
  );

describe("libraryEffect — tier column", () => {
  test("a platform row and a tenant row differ by tier", async () => {
    // Two entries sharing a name — one from the platform catalogue, one the
    // workspace's own copy — is the case the gallery actually serves, and the
    // tier is the only cell that tells them apart.
    const captured: string[] = [];
    const tables: TableCapture[] = [];
    const requests: HttpRequestInput[] = [];
    await Effect.runPromiseExit(
      libraryEffect.pipe(
        Effect.provide(
          makeLayer(captured, tables, false, requests, {
            items: [
              { id: "github-pr-reviewer", name: "same", visibility: "platform" },
              { id: "01900000-0000-7000-8000-0000000aa001", name: "same", visibility: "tenant" },
            ],
          }),
        ),
      ),
    );
    const rows = tables[0]?.rows as Array<{ tier: string }>;
    expect(rows[0]?.tier).toBe("platform");
    expect(rows[1]?.tier).toBe("tenant");
  });
});

describe("libraryEffect — table render", () => {
  test("lists the workspace gallery, joining credentials and rendering — for none", async () => {
    const captured: string[] = [];
    const tables: TableCapture[] = [];
    const requests: HttpRequestInput[] = [];
    const exit = await Effect.runPromiseExit(
      libraryEffect.pipe(
        Effect.provide(
          makeLayer(captured, tables, false, requests, {
            items: [
              {
                id: "github-pr-reviewer",
                name: "GitHub Pull Request reviewer",
                visibility: "platform",
                requirements: { credentials: ["github"] },
              },
              {
                id: "no-creds",
                name: "No creds",
                visibility: "tenant",
                requirements: { credentials: [] },
              },
            ],
          }),
        ),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(requests[0]?.path).toBe(`/v1/workspaces/${WS_ID}/fleet-libraries`);
    const rows = tables[0]?.rows as Array<{ id: string; credentials: string; tier: string }>;
    expect(rows[0]?.credentials).toBe("github");

    // empty requirements.credentials renders the em dash, not "undefined"
    expect(rows[1]?.credentials).toBe("—");
  });
});

describe("libraryEffect — JSON mode", () => {
  test("the JSON branch carries the same entries the table renders", async () => {
    const captured: string[] = [];
    const tables: TableCapture[] = [];
    const requests: HttpRequestInput[] = [];
    const payload = { items: [{ id: "x", name: "X", required_credentials: [] }] };
    const exit = await Effect.runPromiseExit(
      libraryEffect.pipe(
        Effect.provide(makeLayer(captured, tables, true, requests, payload)),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(tables).toHaveLength(0);
    expect(captured.join("\n")).toContain("\"x\"");
  });
});

describe("libraryEffect — empty catalog", () => {
  test("an empty gallery names library add as the next move", async () => {
    const captured: string[] = [];
    const tables: TableCapture[] = [];
    const requests: HttpRequestInput[] = [];
    const exit = await Effect.runPromiseExit(
      libraryEffect.pipe(
        Effect.provide(makeLayer(captured, tables, false, requests, { items: [] })),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(tables).toHaveLength(0);
    expect(captured.join("\n")).toContain("No Fleet libraries in this workspace.");
  });

  test("a missing items field is an empty gallery, not a crash", async () => {
    const captured: string[] = [];
    const tables: TableCapture[] = [];
    const requests: HttpRequestInput[] = [];
    const exit = await Effect.runPromiseExit(
      libraryEffect.pipe(
        Effect.provide(makeLayer(captured, tables, false, requests, {})),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(captured.join("\n")).toContain("No Fleet libraries in this workspace.");
  });
});
