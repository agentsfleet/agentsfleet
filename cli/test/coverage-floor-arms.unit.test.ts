// Floor-closure arms — the branches `main` shipped uncovered, pinned the day
// the 100% floor became enforcing (scripts/enforce-coverage.mjs wired into CI).
// Each block names the file it closes; every test drives an exported effect
// through the shared memory layers, never internals.

import { describe, expect, test } from "bun:test";
import { Effect, Exit, Layer } from "effect";

import {
  apiKeyCreateEffectFromArgs,
  apiKeyListEffectFromArgs,
  apiKeyRevokeEffectFromId,
  apiKeyDeleteEffectFromId,
} from "../src/commands/api_key.ts";
import {
  connectorListEffectFromArgs,
  connectorStatusEffectFromArgs,
} from "../src/commands/connector.ts";
import {
  scheduleAddEffectFromArgs,
  scheduleListEffectFromArgs,
  scheduleUpdateEffectFromArgs,
} from "../src/commands/fleet_schedule.ts";
import { installEffectFromFlags } from "../src/commands/fleet_install.ts";
import { HttpClient } from "../src/services/http-client.ts";
import { Workspaces } from "../src/services/workspaces.ts";
import { ValidationError, ConfigError } from "../src/errors/index.ts";
import {
  MEMORY_TEST_WS_ID,
  failureOf,
  httpLayerReturning,
  newCapture,
  runWith,
  workspacesLayer,
} from "./helpers-memory-layers.ts";

const VALID_ID = "01900000-0000-7000-8000-0000000000aa";
const VALID_ID_2 = "01900000-0000-7000-8000-0000000000bb";

/** Answers per-path so multi-request flows (pagination walks) are drivable. */
const httpLayerByPath = (
  answer: (path: string) => unknown,
  paths: string[],
): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: ((input: { path: string }) => {
      paths.push(input.path);
      return Effect.succeed(answer(input.path));
    }) as HttpClient["request"],
  });

/** A workspace state with nothing selected — the fail-closed arm's fixture. */
const emptyWorkspacesLayer = (): Layer.Layer<Workspaces> =>
  Layer.succeed(Workspaces, {
    load: Effect.succeed({ current_workspace_id: null, items: [] }),
    save: () => Effect.void,
  });

// ── src/commands/api_key.ts ─────────────────────────────────────────────────

describe("api_key floor arms", () => {
  test("revoke rejects a malformed id before any request", async () => {
    const cap = newCapture();
    const paths: string[] = [];
    const exit = await runWith(apiKeyRevokeEffectFromId("not-a-uuid"), {
      cap,
      http: httpLayerReturning({}, paths),
    });
    const err = failureOf(exit);
    expect(err).toBeInstanceOf(ValidationError);
    expect(err?.suggestion).toContain("uuidv7");
    expect(paths).toEqual([]);
  });

  test("list rejects an out-of-range page and an unknown sort, sending nothing", async () => {
    const cap = newCapture();
    const paths: string[] = [];
    const range = await runWith(
      apiKeyListEffectFromArgs({ page: "0", pageSize: undefined, sort: undefined }),
      { cap, http: httpLayerReturning({ items: [] }, paths) },
    );
    expect(failureOf(range)?.detail).toContain("page must be an integer");

    const sort = await runWith(
      apiKeyListEffectFromArgs({ page: undefined, pageSize: undefined, sort: "bogus" }),
      { cap, http: httpLayerReturning({ items: [] }, paths) },
    );
    expect(failureOf(sort)?.detail).toContain("sort must be one of");
    expect(paths).toEqual([]);
  });

  test("JSON mode prints the raw envelope for create, list, revoke, and delete", async () => {
    const cap = newCapture();
    const paths: string[] = [];
    const http = httpLayerReturning({ id: VALID_ID, items: [] }, paths);

    for (const effect of [
      apiKeyCreateEffectFromArgs({ name: "ci-key", description: undefined }),
      apiKeyListEffectFromArgs({ page: undefined, pageSize: undefined, sort: undefined }),
      apiKeyRevokeEffectFromId(VALID_ID),
      apiKeyDeleteEffectFromId(VALID_ID),
    ]) {
      const exit = await runWith(effect, { cap, http, jsonMode: true });
      expect(Exit.isSuccess(exit)).toBe(true);
    }
    expect(cap.jsons.length).toBe(4);
    expect(cap.infos).toEqual([]);
  });

  test("an empty key list reports itself in human mode instead of a bare table", async () => {
    const cap = newCapture();
    const exit = await runWith(
      apiKeyListEffectFromArgs({ page: undefined, pageSize: undefined, sort: undefined }),
      { cap, http: httpLayerReturning({ items: [] }, []) },
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(cap.infos.join("\n")).toContain("no API keys found");
  });
});

// ── src/commands/connector.ts ───────────────────────────────────────────────

describe("connector floor arms", () => {
  const ENTRY = {
    id: "github",
    name: "GitHub",
    configured: true,
    connected: false,
  };

  test("JSON mode prints connector summaries verbatim", async () => {
    const cap = newCapture();
    const exit = await runWith(connectorListEffectFromArgs(undefined), {
      cap,
      http: httpLayerReturning([ENTRY], []),
      jsonMode: true,
    });
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(cap.jsons.length).toBe(1);
  });

  test("an empty catalog reports itself in human mode", async () => {
    const cap = newCapture();
    const exit = await runWith(connectorListEffectFromArgs(undefined), {
      cap,
      http: httpLayerReturning([], []),
    });
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(cap.infos.join("\n")).toContain("no connectors found");
  });

  test("human mode renders boolean cells through the primitive normalizer", async () => {
    const cap = newCapture();
    const exit = await runWith(connectorListEffectFromArgs(undefined), {
      cap,
      http: httpLayerReturning([ENTRY], []),
    });
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(cap.tables.length).toBe(1);
  });

  test("status renders numeric and boolean detail cells through the primitive normalizer", async () => {
    const cap = newCapture();
    const paths: string[] = [];
    const http = httpLayerByPath(
      (path) =>
        path.endsWith("/github")
          ? { status: "connected", installation_id: 42, active: true }
          : [{ id: "github", name: "GitHub", configured: true, connected: true }],
      paths,
    );
    const exit = await runWith(connectorStatusEffectFromArgs(undefined, "github"), { cap, http });
    expect(Exit.isSuccess(exit)).toBe(true);
    // The detail table carries the number and the boolean as rendered cells.
    const cells = JSON.stringify(cap.tables);
    expect(cells).toContain("42");
    expect(cells).toContain("true");
  });

  test("status of a provider absent from the catalog is a typed refusal", async () => {
    const cap = newCapture();
    const exit = await runWith(connectorStatusEffectFromArgs(undefined, "gitlab"), {
      cap,
      http: httpLayerReturning([ENTRY], []),
    });
    const err = failureOf(exit);
    expect(err).toBeInstanceOf(ValidationError);
    expect(err?.detail).toContain("unknown connector provider");
  });
});

// ── src/commands/fleet_schedule.ts ──────────────────────────────────────────

describe("fleet_schedule floor arms", () => {
  test("a missing fleet id is refused before any request", async () => {
    const cap = newCapture();
    const paths: string[] = [];
    const exit = await runWith(
      scheduleListEffectFromArgs(undefined, {}),
      { cap, http: httpLayerReturning({ items: [] }, paths) },
    );
    expect(failureOf(exit)?.detail).toContain("is required");
    expect(paths).toEqual([]);
  });

  test("a malformed fleet id is refused before any request", async () => {
    const cap = newCapture();
    const paths: string[] = [];
    const exit = await runWith(
      scheduleListEffectFromArgs("not-a-uuid", {}),
      { cap, http: httpLayerReturning({ items: [] }, paths) },
    );
    expect(failureOf(exit)?.suggestion).toContain("uuidv7");
    expect(paths).toEqual([]);
  });

  test("a --workspace override is validated and used for the request path", async () => {
    const cap = newCapture();
    const paths: string[] = [];
    const exit = await runWith(
      scheduleListEffectFromArgs(VALID_ID, { workspaceId: VALID_ID_2 }),
      { cap, http: httpLayerReturning({ items: [] }, paths) },
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(paths[0]).toContain(VALID_ID_2);
  });

  test("no selected workspace and no override is a ConfigError naming the fix", async () => {
    const cap = newCapture();
    const exit = await runWith(
      scheduleAddEffectFromArgs(VALID_ID, { cron: "0 * * * *", message: "hi" }),
      { cap, http: httpLayerReturning({}, []), workspaces: emptyWorkspacesLayer() },
    );
    const err = failureOf(exit);
    expect(err).toBeInstanceOf(ConfigError);
    expect(err?.suggestion).toContain("workspace use");
  });

  test("update refuses a status outside active|paused", async () => {
    const cap = newCapture();
    const exit = await runWith(
      scheduleUpdateEffectFromArgs(VALID_ID, VALID_ID_2, { status: "sleeping" }),
      { cap, http: httpLayerReturning({}, []) },
    );
    expect(failureOf(exit)?.detail).toContain("status must be active or paused");
  });

  test("an empty schedule list reports itself in human mode", async () => {
    const cap = newCapture();
    const exit = await runWith(
      scheduleListEffectFromArgs(VALID_ID, {}),
      { cap, http: httpLayerReturning({ items: [] }, []) },
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(cap.infos.join("\n")).toContain("No schedules for this Fleet.");
  });
});

// ── src/commands/fleet_install.ts ───────────────────────────────────────────

describe("fleet_install floor arms", () => {
  test("the gallery walk follows next_cursor and ends typed when no page holds the id", async () => {
    const cap = newCapture();
    const paths: string[] = [];
    // Page 1 carries a cursor and no match; page 2 is the last page and has
    // no match either — the walk must advance the cursor, stop at the end,
    // and refuse with a typed error rather than a silent undefined.
    const http = httpLayerByPath(
      (path) =>
        path.includes("starting_after")
          ? { items: [{ id: "other-2" }], next_cursor: null }
          : { items: [{ id: "other-1" }], next_cursor: "cur-2" },
      paths,
    );
    const exit = await runWith(
      installEffectFromFlags({ libraryId: "lib-missing" }),
      { cap, http, workspaces: workspacesLayer() },
    );
    expect(Exit.isFailure(exit)).toBe(true);
    expect(paths.length).toBe(2);
    expect(paths[1]).toContain("starting_after=cur-2");
    expect(paths[0]).toContain(MEMORY_TEST_WS_ID);
  });

  test("the gallery walk stops at its page cap instead of chasing cursors forever", async () => {
    const cap = newCapture();
    const paths: string[] = [];
    let page = 0;
    // Every page dangles another cursor. The walk must give up at its cap and
    // refuse typed — an unbounded chase against a misbehaving server is the
    // failure mode the cap exists to prevent.
    const http = httpLayerByPath(() => {
      page += 1;
      return { items: [{ id: `other-${page}` }], next_cursor: `cur-${page}` };
    }, paths);
    const exit = await runWith(
      installEffectFromFlags({ libraryId: "lib-never-found" }),
      { cap, http, workspaces: workspacesLayer() },
    );
    expect(Exit.isFailure(exit)).toBe(true);
    expect(paths.length).toBe(50); // pin test: literal is the contract (GALLERY_MAX_PAGES)
  });
});
