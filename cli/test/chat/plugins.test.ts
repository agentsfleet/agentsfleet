import { expect, test } from "bun:test";
import { runCli } from "../../src/cli.ts";
import { PLUGIN_PAGE_LIMIT } from "../../src/commands/plugins-read.ts";
import { bufferStream, cliEnv, withAuthedStateDir } from "../helpers-cli-state.ts";
import { PLUGIN_ID, REVISION_ID, WORKSPACE, withApi, problem } from "./fixture.ts";

const CARD = {
  id: PLUGIN_ID, name: "argo", description: "Review deployments",
  created_at: "1970-01-01T00:00:00.001Z", requirements: {}, required_credentials_reasons: {}, head_revision_id: REVISION_ID, etag: '"plugin-1"',
};
const BASE = `/v1/workspaces/${WORKSPACE}/plugins`;
const CURSOR = "opaque+/cursor";

async function invoke(origin: string, argv: string[]) {
  return withAuthedStateDir({ workspaceId: WORKSPACE, apiUrl: origin }, async () => {
    const stdout = bufferStream();
    const stderr = bufferStream();
    const code = await runCli(["--api", origin, ...argv], {
      stdout: stdout.stream, stderr: stderr.stream, env: cliEnv({ NO_COLOR: "1" }),
    });
    return { code, out: stdout.read(), err: stderr.read() };
  });
}

for (const [verb, suffix] of [[[], ""], [["list"], ""]] as const) {
  test(`Plugin ${suffix || "gallery"} preserves native rows and an explicit cursor`, async () => {
    const seen: string[] = [];
    await withApi((request) => {
      seen.push(new URL(request.url).pathname + new URL(request.url).search);
      expect(request.headers.get("authorization")).toMatch(/^Bearer /u);
      return Response.json({ items: [CARD], next_cursor: CURSOR });
    }, async (_, origin) => {
      const result = await invoke(origin, ["--json", "plugins", ...verb, "--limit", "2", "--starting-after", CURSOR]);
      expect(result.code).toBe(0);
      expect(JSON.parse(result.out)).toEqual({ items: [CARD], next_cursor: CURSOR });
      expect(seen).toEqual([`${BASE}${suffix}?limit=2&starting_after=${encodeURIComponent(CURSOR)}`]);
    });
  });
}

test("Plugin text gives revision and ordinary chat entry without walking hidden pages", async () => {
  let calls = 0;
  await withApi(() => { calls += 1; return Response.json({ items: [CARD], next_cursor: CURSOR }); }, async (_, origin) => {
    const result = await invoke(origin, ["plugins"]);
    expect(result.code).toBe(0);
    expect(result.out).toContain(REVISION_ID);
    expect(result.out).toContain("agentsfleet chat --plugin");
    expect(result.out).toContain(`--starting-after ${CURSOR}`);
    expect(calls).toBe(1);
  });
});

test("an empty Plugin points to explicit chat publication", async () => {
  await withApi(() => Response.json({ items: [], next_cursor: null }), async (_, origin) => {
    const result = await invoke(origin, ["plugins", "list"]);
    expect(result.code).toBe(0);
    expect(result.out).toContain("/publish");
    expect(result.out).not.toContain("--github");
  });
});

for (const body of [{ items: [] }, { items: [{ id: PLUGIN_ID }], next_cursor: null },
  { items: Array.from({ length: PLUGIN_PAGE_LIMIT + 1 }, () => CARD), next_cursor: null }]) {
  test("malformed native Plugin pages fail visibly instead of becoming an empty result", async () => {
    await withApi(() => Response.json(body), async (_, origin) => {
      const result = await invoke(origin, ["plugins", "--json"]);
      expect(result.code).not.toBe(0);
      expect(result.err).toContain("PLUGIN_RESPONSE_INVALID");
      expect(result.out).toBe("");
    });
  });
}

test("owned deletion names the native resource and leaves installed revision pins alone", async () => {
  const seen: string[] = [];
  await withApi((request) => {
    seen.push(`${request.method} ${new URL(request.url).pathname}`);
    return new Response(null, { status: 204 });
  }, async (_, origin) => {
    const result = await invoke(origin, ["--json", "plugins", "delete", PLUGIN_ID]);
    expect(result.code).toBe(0);
    expect(result.out).toContain('"removed": true');
    expect(seen).toEqual([`DELETE ${BASE}/${PLUGIN_ID}`]);
  });
});

for (const status of [401, 403, 404, 409, 410]) {
  test(`Plugin deletion propagates ${status} without a success claim`, async () => {
    await withApi(() => problem(status), async (_, origin) => {
      const result = await invoke(origin, ["plugins", "delete", PLUGIN_ID]);
      expect(result.code).not.toBe(0);
      expect(result.out).not.toContain("no longer");
      expect(result.err).toContain("injected refusal");
    });
  });
}

test("removed bundle commands refuse before any server mutation", async () => {
  const seen: string[] = [];
  // A root typo reads the caller's scopes for its suggestions; that read is
  // not a mutation, so it is the one request this test lets through.
  await withApi((request) => {
    if (new URL(request.url).pathname !== "/v1/users/me") seen.push(request.method);
    return problem(500);
  }, async (_, origin) => {
    for (const argv of [["install", "--library", "argo"], ["library", "create", "--github", "acme/argo"],
      ["fleet", "update", PLUGIN_ID, "--from", "/tmp/argo"]]) {
      expect((await invoke(origin, argv)).code).not.toBe(0);
    }
    expect(seen).toEqual([]);
  });
});

const READ = {card: CARD, head: {id: REVISION_ID, plugin_id: PLUGIN_ID, revision: "2", canonical_digest: "immutable-digest", definition: {instructions_md: "Keep this full body"}},
  review: {review_digest: "current-review"}, etag: '"current-tag"', extension: {preserved: true}};
const DETAIL = {revision: READ.head, review: READ.review, extension: READ.extension};
const HISTORY = {items: [{id: REVISION_ID, revision: "2", canonical_digest: "immutable-digest", created_at: CARD.created_at}], next_cursor: CURSOR, total: null};

test("plugins_show_and_revisions_read_renamed_routes", async () => {
  const seen: string[] = [];
  await withApi(request => {
    const path = new URL(request.url).pathname;
    seen.push(path);
    return Response.json(path.endsWith(`/${REVISION_ID}`) ? DETAIL : path.endsWith("/revisions") ? HISTORY : READ);
  }, async (_, origin) => {
    const show = await invoke(origin, ["plugins", "show", PLUGIN_ID]);
    expect(show.code).toBe(0);
    for (const value of [CARD.name, PLUGIN_ID, REVISION_ID, READ.etag]) expect(show.out).toContain(value);
    const history = await invoke(origin, ["plugins", "revisions", PLUGIN_ID, "--limit", "2"]);
    expect(history.err).toBe("");
    expect(history.code).toBe(0);
    for (const value of [REVISION_ID, "immutable-digest", CARD.created_at]) expect(history.out).toContain(value);
    expect(history.out).toContain(`agentsfleet plugins revisions ${PLUGIN_ID} --starting-after ${CURSOR} --limit 2`);
    const detail = await invoke(origin, ["plugins", "revisions", PLUGIN_ID, REVISION_ID]);
    expect(detail.code).toBe(0);
    expect(detail.out).toContain("immutable-digest"); expect(detail.out).toContain("current-review");
    expect(seen).toEqual([`${BASE}/${PLUGIN_ID}`, `${BASE}/${PLUGIN_ID}/revisions`, `${BASE}/${PLUGIN_ID}/revisions/${REVISION_ID}`]);
  });
});

test("plugin_readers_validate_ids_and_preserve_paging", async () => {
  const seen: string[] = [];
  await withApi(request => {seen.push(new URL(request.url).pathname + new URL(request.url).search); return Response.json(HISTORY);}, async (_, origin) => {
    const result = await invoke(origin, ["--json", "plugins", "revisions", PLUGIN_ID, "--limit", "2", "--starting-after", CURSOR]);
    expect(result.code).toBe(0); expect(JSON.parse(result.out)).toEqual(HISTORY);
    expect(seen).toEqual([`${BASE}/${PLUGIN_ID}/revisions?limit=2&starting_after=${encodeURIComponent(CURSOR)}`]);
    for (const args of [["show", "slug"], ["revisions", "slug"], ["revisions", PLUGIN_ID, "slug"], ["revisions", PLUGIN_ID, "--limit", "101"]]) {
      expect((await invoke(origin, ["plugins", ...args])).code).toBe(4);
    }
    expect(seen).toHaveLength(1);
  });
});

test.each([["show", []], ["revisions", [REVISION_ID]]] as const)("Plugin reader %s preserves the complete JSON body", async (verb, ids) => {
  const body = verb === "show" ? READ : DETAIL;
  await withApi(() => Response.json(body), async (_, origin) => {
    const result = await invoke(origin, ["--json", "plugins", verb, PLUGIN_ID, ...ids]);
    expect(result.code).toBe(0); expect(JSON.parse(result.out)).toEqual(body);
  });
});

test.each([["show", []], ["revisions", []], ["revisions", [REVISION_ID]]] as const)("Plugin reader %s preserves a structured refusal", async (verb, ids) => {
  await withApi(() => Response.json({error_code: "UZ-AUTHOR-001", user_message: "injected refusal"}, {status: 404}), async (_, origin) => {
    const result = await invoke(origin, ["plugins", verb, PLUGIN_ID, ...ids]);
    expect(result.code).toBe(3); expect(result.err).toContain("UZ-"); expect(result.err).toContain("injected refusal"); expect(result.out).toBe("");
  });
});
