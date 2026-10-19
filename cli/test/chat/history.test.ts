import { expect, test } from "bun:test";
import { runCli } from "../../src/cli.ts";
import { bufferStream, cliEnv, withAuthedStateDir } from "../helpers-cli-state.ts";
import { DRAFT, DRAFT_ID, FLEET_ID, WORKSPACE, problem, withApi } from "./fixture.ts";

const PATH = `/v1/workspaces/${WORKSPACE}/drafts`;
const TITLE = "Watch release failures";
const CURSOR = "opaque/query+cursor=";
const ROW = {id: DRAFT_ID, title: TITLE, etag: DRAFT.etag, state: "editing", linked_fleet_id: FLEET_ID,
  updated_at: "2026-10-19T00:00:00Z", expires_at: null, candidate_revision: 7, setup_revision: 3, publication_revision: 2};
const PAGE = {items: [ROW], next_cursor: CURSOR, total: null, extra: "preserved"};
type Call = {method: string; path: string; query: string; body: unknown; tag: string | null; authenticated: boolean};
type Reply = (request: Request, calls: readonly Call[]) => Response | Promise<Response>;

async function run(args: readonly string[], reply: Reply) {
  const calls: Call[] = [];
  return withApi(async request => {
    const url = new URL(request.url);
    calls.push({method: request.method, path: url.pathname, query: url.search,
      body: request.method === "PATCH" ? await request.clone().json() : null,
      tag: request.headers.get("If-Match"), authenticated: request.headers.has("Authorization")});
    return reply(request, calls);
  }, async (_transport, origin) => withAuthedStateDir({workspaceId: WORKSPACE, apiUrl: origin}, async () => {
    const out = bufferStream(); const err = bufferStream();
    const code = await runCli(["chat", ...args], {stdout: out.stream, stderr: err.stream,
      env: cliEnv({AGENTSFLEET_API_URL: origin, NO_COLOR: "1"})});
    return {code, stdout: out.read(), stderr: err.read(), calls};
  }));
}

test("lists a bounded private page and repeats the same literal search with its opaque cursor", async () => {
  const q = "release %_+? 🔎";
  const args = ["list", "--search", ` ${q} `, "--state", "editing", "--fleet", FLEET_ID, "--limit", "1"];
  const first = await run(args, () => Response.json(PAGE));
  expect(first.code).toBe(0);
  expect(first.calls).toHaveLength(1);
  expect(first.calls[0]?.path).toBe(PATH);
  expect(first.calls[0]?.authenticated).toBe(true);
  expect(Object.fromEntries(new URLSearchParams(first.calls[0]?.query))).toEqual({
    q, limit: "1", state: "editing", linked_fleet_id: FLEET_ID,
  });
  expect(first.stdout).toContain(TITLE);
  expect(first.stdout).toContain(DRAFT_ID);
  expect(first.stdout).toContain("Proposed v7 · Saved v3 · Published v2");
  expect(first.stdout).toContain("STATUS");
  expect(first.stdout).toContain("UPDATED");
  expect(first.stdout).toContain(CURSOR);
  const next = await run([...args, "--cursor", CURSOR, "--json"], () => Response.json({...PAGE, next_cursor: null}));
  expect(next.code).toBe(0);
  expect(new URLSearchParams(next.calls[0]?.query).get("starting_after")).toBe(CURSOR);
  expect(new URLSearchParams(next.calls[0]?.query).get("q")).toBe(q);
  expect(JSON.parse(next.stdout)).toEqual({...PAGE, next_cursor: null});
  expect(next.calls).toHaveLength(1);
});

test.each([
  ["editing", null, null, null, "In progress", "—"],
  ["saving", null, 4, null, "Saving fleet", "Saved v4"],
  ["closed", FLEET_ID, null, null, "Fleet saved", "—"],
  ["closed", null, null, null, "Chat closed", "—"],
] as const)("renders recorded %s chat state without inventing versions", async (state, linked_fleet_id, setup_revision, publication_revision, label, versions) => {
  const row = {...ROW, state, linked_fleet_id, candidate_revision: null, setup_revision, publication_revision};
  const result = await run(["list"], () => Response.json({items: [row], next_cursor: null}));
  expect(result.code).toBe(0);
  expect(new URLSearchParams(result.calls[0]?.query).get("limit")).toBe("50");
  expect(result.stdout).toContain(label);
  expect(result.stdout).toContain(versions);
  expect(result.stdout).not.toContain("vnull");
  expect(result.stdout).not.toContain("v0");
});

test.each([
  ["missing items", {}], ["null row", {items: [null], next_cursor: null}],
  ["missing cursor", {items: []}], ["unsafe revision", {items: [{...ROW, candidate_revision: Number.MAX_SAFE_INTEGER + 1}], next_cursor: null}],
  ["missing revision", {items: [{...ROW, setup_revision: undefined}], next_cursor: null}],
  ["numeric title", {items: [{...ROW, title: 4}], next_cursor: null}],
  ["invalid instant", {items: [{...ROW, updated_at: "2026-13-01T00:00:00Z"}], next_cursor: null}],
  ["oversized page", {items: Array.from({length: 101}, () => ROW), next_cursor: null}],
])("refuses %s before text or JSON success", async (_label, body) => {
  for (const json of [false, true]) {
    const result = await run(["list", ...(json ? ["--json"] : [])], () => Response.json(body));
    expect(result.code).not.toBe(0);
    expect(result.stdout + result.stderr).toContain("CHAT_RESPONSE_INVALID");
    expect(result.stdout).not.toContain(TITLE);
    expect(result.calls).toHaveLength(1);
  }
});

test("an empty page is explicit in text and intact in JSON", async () => {
  for (const json of [false, true]) {
    const body = {items: [], next_cursor: null};
    const result = await run(["list", ...(json ? ["--json"] : [])], () => Response.json(body));
    expect(result.code).toBe(0);
    if (json) expect(JSON.parse(result.stdout)).toEqual(body);
    else expect(result.stdout).toContain("No setup chats match this list.");
  }
});

test.each([
  ["search blank", ["list", "--search", "   "]],
  ["search long", ["list", "--search", "x".repeat(201)]],
  ["search NUL", ["list", "--search", "x\0y"]],
  ["zero limit", ["list", "--limit", "0"]], ["over limit", ["list", "--limit", "101"]],
  ["decimal limit", ["list", "--limit", "1.5"]], ["text limit", ["list", "--limit", "all"]],
  ["empty cursor", ["list", "--cursor", ""]], ["unknown state", ["list", "--state", "draft"]],
  ["invalid fleet", ["list", "--fleet", "missing"]],
  ["invalid draft", ["rename", "missing", TITLE]],
  ["title blank", ["rename", DRAFT_ID, "  "]],
  ["title long", ["rename", DRAFT_ID, "x".repeat(101)]],
  ["title NUL", ["rename", DRAFT_ID, "x\0y"]],
  ["delete invalid draft", ["delete", "missing"]],
] as const)("rejects %s without a request", async (_label, args) => {
  const result = await run(args, () => problem(500));
  expect(result.code).toBe(4);
  expect(result.calls).toHaveLength(0);
});

test("Unicode bounds count characters and rename uses the freshly read tag", async () => {
  const title = "🚀".repeat(100);
  const changed = {...DRAFT, title, etag: '"draft-2"', extra: "preserved"};
  for (const json of [false, true]) {
    const result = await run(["rename", DRAFT_ID, ` ${title} `, ...(json ? ["--json"] : [])], request =>
      Response.json(request.method === "GET" ? {...DRAFT, etag: '"fresh"'} : changed));
    expect(result.code).toBe(0);
    expect(result.calls.map(({method, path, tag, body}) => ({method, path, tag, body}))).toEqual([
      {method: "GET", path: `${PATH}/${DRAFT_ID}`, tag: null, body: null},
      {method: "PATCH", path: `${PATH}/${DRAFT_ID}`, tag: '"fresh"', body: {title}},
    ]);
    if (json) expect(JSON.parse(result.stdout)).toEqual(changed);
    else expect(result.stdout).toContain(title);
  }
  const searched = await run(["list", "--search", "🚀".repeat(200)], () => Response.json({items: [], next_cursor: null}));
  expect(searched.code).toBe(0);
});

test.each(["read", "write"] as const)("rename stops on a refused %s without reporting success", async step => {
  const result = await run(["rename", DRAFT_ID, TITLE], request =>
    step === "read" || request.method === "PATCH" ? problem(412) : Response.json(DRAFT));
  expect(result.code).not.toBe(0);
  expect(result.stdout).not.toContain("Renamed");
  expect(result.calls).toHaveLength(step === "read" ? 1 : 2);
});

test.each(["read", "write"] as const)("rename rejects a malformed %s response", async step => {
  const result = await run(["rename", DRAFT_ID, TITLE, "--json"], request =>
    Response.json(step === "read" || request.method === "PATCH" ? {} : DRAFT));
  expect(result.code).not.toBe(0);
  expect(result.stdout + result.stderr).toContain("CHAT_RESPONSE_INVALID");
  expect(result.calls).toHaveLength(step === "read" ? 1 : 2);
});

test.each([401, 403, 404, 410] as const)("list and delete preserve access refusal %s", async status => {
  for (const args of [["list"], ["delete", DRAFT_ID, "--yes"]]) {
    const result = await run(args, () => problem(status));
    expect(result.code).not.toBe(0);
    expect(result.stdout).toBe("");
    expect(result.calls).toHaveLength(1);
  }
});

test("delete removes only the selected private chat and keeps JSON machine readable", async () => {
  for (const json of [false, true]) {
    const result = await run(["delete", DRAFT_ID, "--yes", ...(json ? ["--json"] : [])], () => new Response(null, {status: 204}));
    expect(result.code).toBe(0);
    expect(result.calls).toEqual([{method: "DELETE", path: `${PATH}/${DRAFT_ID}`, body: null, query: "", tag: null, authenticated: true}]);
    if (json) expect(JSON.parse(result.stdout)).toEqual({draft_id: DRAFT_ID, deleted: true});
    else expect(result.stdout).toContain(`Deleted setup chat ${DRAFT_ID}`);
  }
});

test("chat history commands describe their own filters and actions in help", async () => {
  for (const [verb, label] of [["list", "--search"], ["rename", "title"], ["delete", "private setup chat"]] as const) {
    const result = await run([verb, "--help"], () => problem(500));
    expect(result.code).toBe(0);
    expect(result.stdout).toContain(label);
    expect(result.calls).toHaveLength(0);
  }
});

test("rename explains the daemon's busy refusal without retrying a write", async () => {
  const result = await run(["rename", DRAFT_ID, TITLE], request => request.method === "GET" ? Response.json(DRAFT)
    : Response.json({title: "Chat is busy", detail: "active message", error_code: "UZ-AUTHOR-002"}, {status: 409}));
  expect(result.code).not.toBe(0);
  expect(result.stdout + result.stderr).toContain("busy or closed");
  expect(result.stdout + result.stderr).toContain("wait for its reply");
  expect(result.stdout).not.toContain("Renamed");
  expect(result.calls).toHaveLength(2);
});

test("unattended deletion requires --yes without sending a request", async () => {
  const result = await run(["delete", DRAFT_ID], () => problem(500));
  expect(result.code).toBe(4);
  expect(result.stderr).toContain("--yes");
  expect(result.calls).toHaveLength(0);
});

test("an explicit workspace is used for the whole private history read", async () => {
  const result = await run(["list", "--workspace", FLEET_ID], () => Response.json({items: [], next_cursor: null}));
  expect(result.code).toBe(0);
  expect(result.calls[0]?.path).toBe(`/v1/workspaces/${FLEET_ID}/drafts`);
});
