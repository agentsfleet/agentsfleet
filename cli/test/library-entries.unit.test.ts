import { AGE_KEY, ago, entityColumns, EMPTY_CELL } from "../src/output/index.ts";
// Unit coverage for `agentsfleet library list` and `library remove` — the two
// verbs over the workspace's OWN entries.
//
// The invariant these exist for is the path. Bare `agentsfleet library` reads
// the gallery, which is platform ∪ tenant and is what `install --library`
// resolves against; these two read and write the owned collection, which is
// tenant-only. A command that reached the gallery instead would list rows a
// workspace cannot remove and offer a removal that answers 204 for every one
// of them.

import { describe, test, expect } from "bun:test";
import { Effect, Exit, Layer, Option, Redacted } from "effect";

import { libraryEffect } from "../src/commands/fleet_library.ts";
import { libraryListEffect } from "../src/commands/fleet_library_list.ts";
import {
  libraryRemoveEffectFromArgs,
  removedNotice,
} from "../src/commands/fleet_library_remove.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient, type HttpRequestInput } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import { outputDouble } from "./helpers-output-double.ts";
import { Workspaces } from "../src/services/workspaces.ts";
import { ServerError } from "../src/errors/index.ts";

const TOKEN = "test.jwt.library.entries";
const WS_ID = "019febb0-272b-78aa-aa3a-03f92e543014";
const ENTRY_ID = "01900000-0000-7000-8000-0000000aa001";
const OWNED_PATH = `/v1/workspaces/${WS_ID}/library-entries`;

interface TableCapture {
  columns: unknown;
  rows: unknown;
}

interface Harness {
  captured: string[];
  tables: TableCapture[];
  requests: HttpRequestInput[];
}

const harness = (): Harness => ({ captured: [], tables: [], requests: [] });

const makeLayer = (
  h: Harness,
  jsonMode: boolean,
  response: unknown,
  failWith?: ServerError,
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
      snapshot: Effect.succeed({
        accessToken: Option.none(),
        savedAt: null,
        sessionId: null,
        apiUrl: null,
        credentialId: null,
      }),
      saveAccessToken: () => Effect.void,
      clearAccessToken: Effect.void,
    }),
    Layer.succeed(HttpClient, {
      request: <T>(input: HttpRequestInput) => {
        h.requests.push(input);
        return failWith ? Effect.fail(failWith) : Effect.sync(() => response as T);
      },
    }),
    Layer.succeed(Workspaces, {
      load: Effect.succeed({ current_workspace_id: WS_ID, items: [] }),
      save: () => Effect.void,
    }),
    Layer.succeed(Output, {
      ...outputDouble({ jsonMode }),
      intro: (m) => Effect.sync(() => { h.captured.push(m); }),
      info: (m) => Effect.sync(() => { h.captured.push(m); }),
      success: (m, d) => Effect.sync(() => { h.captured.push(d ? JSON.stringify(d) : m); }),
      warn: (m) => Effect.sync(() => { h.captured.push(m); }),
      error: (m) => Effect.sync(() => { h.captured.push(m); }),
      outro: (m) => Effect.sync(() => { h.captured.push(m); }),
      printJson: (p) => Effect.sync(() => { h.captured.push(JSON.stringify(p)); }),
      printJsonErr: (p) => Effect.sync(() => { h.captured.push(JSON.stringify(p)); }),
      printTable: (columns, rows) =>
        Effect.sync(() => { h.tables.push({ columns, rows }); }),
      printEntityTable: (spec, rows) =>
        Effect.sync(() => {
          h.tables.push({
            columns: entityColumns(spec),
            rows: rows.map((r) => ({ ...r, [AGE_KEY]: ago(r[spec.ageKey ?? AGE_KEY]) })),
          });
        }),
    }),
  );

const entry = (id: string, overrides: Record<string, unknown> = {}) => ({
  id,
  name: "github-pr-reviewer",
  description: "Reviews pull requests.",
  source_kind: "github",
  source_ref: "acme/reviewer",
  content_hash: "0123456789abcdef",
  created_at: 1_777_507_200_000,
  ...overrides,
});

describe("library list — the workspace's own entries", () => {
  test("reads the owned collection, not the gallery", async () => {
    const h = harness();
    await Effect.runPromiseExit(
      libraryListEffect.pipe(
        Effect.provide(makeLayer(h, false, { items: [entry(ENTRY_ID)] })),
      ),
    );
    expect(h.requests[0]?.path.startsWith(OWNED_PATH)).toBe(true);
    // The gallery path would list platform rows this workspace cannot remove.
    expect(h.requests[0]?.path).not.toContain("fleet-libraries");
  });

  test("bare `library` still reads the gallery", async () => {
    // The preserved behaviour. `install --library <id>` resolves against this
    // list, so redefining the bare command would change what a shipped verb
    // means for every caller.
    const h = harness();
    await Effect.runPromiseExit(
      libraryEffect.pipe(Effect.provide(makeLayer(h, false, { items: [] }))),
    );
    expect(
      h.requests[0]?.path.startsWith(`/v1/workspaces/${WS_ID}/fleet-libraries`),
    ).toBe(true);
  });

  test("prints a row per entry, with its provenance and age", async () => {
    const h = harness();
    await Effect.runPromiseExit(
      libraryListEffect.pipe(
        Effect.provide(
          makeLayer(h, false, {
            items: [entry(ENTRY_ID), entry("01900000-0000-7000-8000-0000000aa002")],
          }),
        ),
      ),
    );
    const rows = h.tables[0]?.rows as Array<Record<string, string>>;
    expect(rows).toHaveLength(2);
    // Two onboardings of near-identical bundles differ by their source before
    // they differ by anything else the table shows.
    expect(rows[0]?.source).toBe("github:acme/reviewer");
    // The onboarding date became the age: same question, fewer steps for a
    // reader deciding which of these they added this morning.
    expect(rows[0]?.created_at).toMatch(/^\d+[smhdy]$/);
  });

  test("a row missing its provenance renders the empty cell, not `undefined`", async () => {
    const h = harness();
    await Effect.runPromiseExit(
      libraryListEffect.pipe(
        Effect.provide(
          makeLayer(h, false, {
            items: [entry(ENTRY_ID, { source_kind: null, source_ref: null, created_at: null })],
          }),
        ),
      ),
    );
    const rows = h.tables[0]?.rows as Array<Record<string, string>>;
    expect(rows[0]?.source).not.toContain("undefined");
    expect(rows[0]?.created_at).toBe(EMPTY_CELL);
  });

  test("--format json prints the entries as data and no table", async () => {
    const h = harness();
    await Effect.runPromiseExit(
      libraryListEffect.pipe(
        Effect.provide(makeLayer(h, true, { items: [entry(ENTRY_ID)] })),
      ),
    );
    expect(h.tables).toHaveLength(0);
    expect(h.captured.join("\n")).toContain(ENTRY_ID);
  });

  test("an empty workspace gets a state naming `library add`", async () => {
    // An empty list with no next step reads as a broken screen rather than an
    // empty one, and `library add` is the command that fills it.
    const h = harness();
    const exit = await Effect.runPromiseExit(
      libraryListEffect.pipe(Effect.provide(makeLayer(h, false, { items: [] }))),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(h.tables).toHaveLength(0);
    expect(h.captured.join("\n")).toContain("library add");
  });
});

describe("library remove — taking one back out", () => {
  test("issues DELETE against the single-entry path", async () => {
    const h = harness();
    const exit = await Effect.runPromiseExit(
      libraryRemoveEffectFromArgs(ENTRY_ID).pipe(
        Effect.provide(makeLayer(h, false, undefined)),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(h.requests[0]?.method).toBe("DELETE");
    expect(h.requests[0]?.path).toBe(`${OWNED_PATH}/${ENTRY_ID}`);
  });

  test("reports what is true afterwards, and that fleets keep running", async () => {
    // Never "deleted 1 row": the endpoint answers 204 for an entry already
    // gone and for one this workspace never owned, so the count is a thing
    // this command cannot know. What it CAN state is the end state.
    const h = harness();
    await Effect.runPromiseExit(
      libraryRemoveEffectFromArgs(ENTRY_ID).pipe(
        Effect.provide(makeLayer(h, false, undefined)),
      ),
    );
    const printed = h.captured.join("\n");
    expect(printed).toContain(ENTRY_ID);
    // The sentence itself, asserted where it is built: the success call
    // carries a data payload, and the output double prints that instead.
    const notice = removedNotice(ENTRY_ID);
    expect(notice).toContain("keep running");
    expect(notice).toContain("no longer in this workspace");
    expect(notice).not.toContain("deleted 1");
  });

  test("a replay succeeds identically", async () => {
    // The daemon is idempotent, and this command must not invent a failure
    // for the second call that the first did not have.
    const first = harness();
    const second = harness();
    for (const h of [first, second]) {
      const exit = await Effect.runPromiseExit(
        libraryRemoveEffectFromArgs(ENTRY_ID).pipe(
          Effect.provide(makeLayer(h, false, undefined)),
        ),
      );
      expect(Exit.isSuccess(exit)).toBe(true);
    }
    expect(second.captured).toEqual(first.captured);
  });

  test("a server refusal fails the effect carrying the server's own detail", async () => {
    // Non-zero exit with the sentence the daemon sent, rather than a stack
    // trace: `Refusal::malformed` is what an entry_id that is not a UUIDv7
    // earns, and its detail is the repair.
    const h = harness();
    const exit = await Effect.runPromiseExit(
      libraryRemoveEffectFromArgs(ENTRY_ID).pipe(
        Effect.provide(
          makeLayer(
            h,
            false,
            undefined,
            new ServerError({
              detail: "entry_id must be a UUIDv7",
              suggestion: "check the id from: agentsfleet library list",
              code: "HTTP_400",
              status: 400,
              requestId: null,
            }),
          ),
        ),
      ),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    expect(String(exit)).toContain("entry_id must be a UUIDv7");
  });
});
