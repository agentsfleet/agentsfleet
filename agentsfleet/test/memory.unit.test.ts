// Direct Effect-layer tests for the memory read verbs: the ValidationError
// guards commander normally pre-empts, table render order, JSON-mode
// selection, workspace resolution, and the ServerError suggestion remap.
// Pure render-helper tests live in memory-render.unit.test.ts (file cap).

import { describe, test, expect } from "bun:test";
import { Effect, Exit, Layer } from "effect";

import {
  memoryListEffectFromFlags,
  memorySearchEffectFromArgs,
} from "../src/commands/memory.ts";
import { Workspaces } from "../src/services/workspaces.ts";
import {
  ConfigError,
  NetworkError,
  ServerError,
  ValidationError,
} from "../src/errors/index.ts";
import {
  MEMORY_TEST_WS_ID as WS_ID,
  failureOf,
  httpLayerFailing,
  httpLayerReturning,
  newCapture,
  runWith,
} from "./helpers-memory-layers.ts";

const ZOMBIE_ID = "01900000-0000-7000-8000-0000005e4e72";

// The wire shape: numeric epoch milliseconds (schema/013 BIGINT).
const FIXTURE_ITEMS = [
  { key: "acme_contact", content: "ops@acme.test is the escalation contact", category: "core", updated_at: 1765500300000 },
  { key: "deploy_window", content: "deploys freeze on fridays", category: "daily", updated_at: 1765500200000 },
  { key: "greeting_style", content: "terse, no emoji", category: "conversation", updated_at: 1765500100000 },
];

const FIXTURE_ENVELOPE = {
  items: FIXTURE_ITEMS,
  total: FIXTURE_ITEMS.length,
  request_id: "req_mem_unit",
};

describe("memory — ValidationError guards", () => {
  test("list fails with ValidationError when --zombie is missing", async () => {
    const cap = newCapture();
    const exit = await runWith(
      memoryListEffectFromFlags({}),
      { http: httpLayerFailing(new NetworkError({ detail: "unused", suggestion: "unused", url: "unused" })), cap },
    );
    const err = failureOf(exit);
    expect(err).toBeInstanceOf(ValidationError);
    if (err instanceof ValidationError) {
      expect(err.detail).toMatch(/--zombie <id> is required/);
      expect(err.suggestion).toMatch(/agentsfleet memory list/);
    }
  });

  test("search fails with ValidationError when --zombie is missing", async () => {
    const cap = newCapture();
    const exit = await runWith(
      memorySearchEffectFromArgs("acme", {}),
      { http: httpLayerFailing(new NetworkError({ detail: "unused", suggestion: "unused", url: "unused" })), cap },
    );
    const err = failureOf(exit);
    expect(err).toBeInstanceOf(ValidationError);
    if (err instanceof ValidationError) {
      expect(err.suggestion).toMatch(/agentsfleet memory search/);
    }
  });

  test("search fails with ValidationError when the query is empty", async () => {
    const cap = newCapture();
    const exit = await runWith(
      memorySearchEffectFromArgs("  ", { zombieId: ZOMBIE_ID }),
      { http: httpLayerFailing(new NetworkError({ detail: "unused", suggestion: "unused", url: "unused" })), cap },
    );
    const err = failureOf(exit);
    expect(err).toBeInstanceOf(ValidationError);
    if (err instanceof ValidationError) {
      expect(err.detail).toMatch(/search query is required/);
    }
  });
});

describe("test_memory_list_table_newest_first", () => {
  test("three fixture rows render in response (newest-first) order with rendered timestamps", async () => {
    const cap = newCapture();
    const paths: string[] = [];
    const exit = await runWith(
      memoryListEffectFromFlags({ zombieId: ZOMBIE_ID }),
      { http: httpLayerReturning(FIXTURE_ENVELOPE, paths), cap },
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(paths[0]).toBe(`/v1/workspaces/${WS_ID}/zombies/${ZOMBIE_ID}/memories`);
    expect(cap.tables).toHaveLength(1);
    const rows = cap.tables[0]?.rows ?? [];
    expect(rows.map((r) => r["key"])).toEqual(["acme_contact", "deploy_window", "greeting_style"]);
    expect(rows[0]?.["category"]).toBe("core");
    // epoch-seconds string fixture renders as ISO 8601
    expect(String(rows[0]?.["updated"])).toMatch(/^\d{4}-\d{2}-\d{2}T/);
    expect(String(rows[0]?.["preview"])).toContain("escalation contact");
  });

  test("list does not re-sort — row order is the server's", async () => {
    const cap = newCapture();
    const reversed = { ...FIXTURE_ENVELOPE, items: [...FIXTURE_ITEMS].reverse() };
    await runWith(
      memoryListEffectFromFlags({ zombieId: ZOMBIE_ID }),
      { http: httpLayerReturning(reversed, []), cap },
    );
    const rows = cap.tables[0]?.rows ?? [];
    expect(rows.map((r) => r["key"])).toEqual(["greeting_style", "deploy_window", "acme_contact"]);
  });
});

describe("test_memory_search_matches", () => {
  test("query is forwarded on the wire and only response rows render", async () => {
    const cap = newCapture();
    const paths: string[] = [];
    const oneMatch = { items: [FIXTURE_ITEMS[0]], total: 1, request_id: "req_mem_search" };
    const exit = await runWith(
      memorySearchEffectFromArgs("acme", { zombieId: ZOMBIE_ID }),
      { http: httpLayerReturning(oneMatch, paths), cap },
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(paths[0]).toContain("?query=acme");
    const rows = cap.tables[0]?.rows ?? [];
    expect(rows).toHaveLength(1);
    expect(rows[0]?.["key"]).toBe("acme_contact");
  });

  test("padded query is trimmed at the boundary before hitting the wire", async () => {
    const paths: string[] = [];
    await runWith(
      memorySearchEffectFromArgs("  acme  ", { zombieId: ZOMBIE_ID }),
      { http: httpLayerReturning(FIXTURE_ENVELOPE, paths), cap: newCapture() },
    );
    expect(paths[0]).toContain("?query=acme");
    expect(paths[0]).not.toContain("%20");
  });

  test("category and limit are forwarded for list; search carries no category", async () => {
    const cap = newCapture();
    const paths: string[] = [];
    await runWith(
      memoryListEffectFromFlags({ zombieId: ZOMBIE_ID, category: "daily", limit: "5" }),
      { http: httpLayerReturning(FIXTURE_ENVELOPE, paths), cap },
    );
    expect(paths[0]).toContain("category=daily");
    expect(paths[0]).toContain("limit=5");

    const searchPaths: string[] = [];
    await runWith(
      memorySearchEffectFromArgs("acme", { zombieId: ZOMBIE_ID, limit: "7" }),
      { http: httpLayerReturning(FIXTURE_ENVELOPE, searchPaths), cap: newCapture() },
    );
    expect(searchPaths[0]).toContain("query=acme");
    expect(searchPaths[0]).toContain("limit=7");
    expect(searchPaths[0]).not.toContain("category=");
  });
});

describe("memory — JSON mode passthrough", () => {
  test("jsonMode prints the envelope verbatim (no table, no info lines)", async () => {
    const cap = newCapture();
    await runWith(
      memoryListEffectFromFlags({ zombieId: ZOMBIE_ID }),
      { jsonMode: true, http: httpLayerReturning(FIXTURE_ENVELOPE, []), cap },
    );
    expect(cap.jsons).toHaveLength(1);
    expect(cap.jsons[0]).toEqual(FIXTURE_ENVELOPE);
    expect(cap.tables).toHaveLength(0);
    expect(cap.infos).toHaveLength(0);
  });

  test("stdoutIsTty=false (piped) prints the envelope even without --json", async () => {
    const cap = newCapture();
    await runWith(
      memoryListEffectFromFlags({ zombieId: ZOMBIE_ID, stdoutIsTty: false }),
      { http: httpLayerReturning(FIXTURE_ENVELOPE, []), cap },
    );
    expect(cap.jsons).toHaveLength(1);
    expect(cap.tables).toHaveLength(0);
  });

  test("a malformed envelope (items not an array) renders as empty instead of crashing", async () => {
    const cap = newCapture();
    const exit = await runWith(
      memoryListEffectFromFlags({ zombieId: ZOMBIE_ID, stdoutIsTty: true }),
      { http: httpLayerReturning({ items: 5, total: 5, request_id: "req_mem_bad" }, []), cap },
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(cap.tables).toHaveLength(0);
    expect(cap.infos[0]).toMatch(/no memories stored/i);
  });

  test("empty result on a terminal prints the friendly line + docs pointer and succeeds", async () => {
    const cap = newCapture();
    const exit = await runWith(
      memoryListEffectFromFlags({ zombieId: ZOMBIE_ID, stdoutIsTty: true }),
      { http: httpLayerReturning({ items: [], total: 0, request_id: "req_mem_empty" }, []), cap },
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(cap.infos[0]).toMatch(/no memories stored/i);
    expect(cap.infos[1]).toContain("docs.agentsfleet.net/memory");
    expect(cap.tables).toHaveLength(0);
  });
});

describe("memory — ServerError suggestion remap", () => {
  const serverError = (code: string): ServerError =>
    new ServerError({
      detail: "zombie not found",
      suggestion: "verify the request payload and retry",
      code,
      status: 404,
      requestId: "req_mem_err",
    });

  test("UZ-MEM-002 remaps to the zombie-listing suggestion", async () => {
    const exit = await runWith(
      memoryListEffectFromFlags({ zombieId: ZOMBIE_ID }),
      { http: httpLayerFailing(serverError("UZ-MEM-002")), cap: newCapture() },
    );
    const err = failureOf(exit);
    expect(err).toBeInstanceOf(ServerError);
    if (err instanceof ServerError) {
      expect(err.suggestion).toContain("agentsfleet list");
      expect(err.code).toBe("UZ-MEM-002");
      expect(err.requestId).toBe("req_mem_err");
    }
  });

  test("UZ-MEM-003 remaps to the retry suggestion", async () => {
    const exit = await runWith(
      memoryListEffectFromFlags({ zombieId: ZOMBIE_ID }),
      { http: httpLayerFailing(serverError("UZ-MEM-003")), cap: newCapture() },
    );
    const err = failureOf(exit);
    if (err instanceof ServerError) {
      expect(err.suggestion).toMatch(/retry shortly/);
    } else {
      throw new Error("expected ServerError");
    }
  });

  test("other server codes keep the transport suggestion untouched", async () => {
    const exit = await runWith(
      memoryListEffectFromFlags({ zombieId: ZOMBIE_ID }),
      { http: httpLayerFailing(serverError("UZ-AUTH-003")), cap: newCapture() },
    );
    const err = failureOf(exit);
    if (err instanceof ServerError) {
      expect(err.suggestion).toBe("verify the request payload and retry");
    } else {
      throw new Error("expected ServerError");
    }
  });
});

describe("memory — workspace resolution", () => {
  test("--workspace override short-circuits the config store entirely", async () => {
    const cap = newCapture();
    const paths: string[] = [];
    const overrideWs = "01900000-0000-7000-8000-0000005e4e99";
    // a die-ing Workspaces layer proves the override never touches disk
    const dieStore = Layer.succeed(Workspaces, {
      load: Effect.die("workspace store must not be read when --workspace is passed"),
      save: () => Effect.die("should not be called"),
    });
    const exit = await runWith(
      memoryListEffectFromFlags({ zombieId: ZOMBIE_ID, workspaceId: overrideWs }),
      { http: httpLayerReturning(FIXTURE_ENVELOPE, paths), cap, workspaces: dieStore },
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(paths[0]).toContain(`/v1/workspaces/${overrideWs}/`);
  });

  test("no workspace selected fails with ConfigError and the workspace-use suggestion", async () => {
    const emptyStore = Layer.succeed(Workspaces, {
      load: Effect.succeed({ current_workspace_id: null, items: [] }),
      save: () => Effect.die("should not be called"),
    });
    const exit = await runWith(memoryListEffectFromFlags({ zombieId: ZOMBIE_ID }), {
      http: httpLayerFailing(new NetworkError({ detail: "unused", suggestion: "unused", url: "unused" })),
      cap: newCapture(),
      workspaces: emptyStore,
    });
    const err = failureOf(exit);
    expect(err).toBeInstanceOf(ConfigError);
    if (err instanceof ConfigError) {
      expect(err.detail).toMatch(/no workspace selected/);
      expect(err.suggestion).toMatch(/workspace use <id>|--workspace <id>/);
    }
  });
});
