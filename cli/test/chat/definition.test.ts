import { afterAll, beforeAll, expect, test } from "bun:test";
import { Effect } from "effect";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runCli } from "../../src/cli.ts";
import { ChatActions } from "../../src/commands/chat/actions.ts";
import { MAX_DEFINITION_BYTES } from "../../src/commands/chat/commands.ts";
import { DraftSession, DRAFT_SOURCE } from "../../src/commands/chat/session.ts";
import { SetupSession } from "../../src/commands/chat/setup-session.ts";
import { HTTP_METHOD } from "../../src/constants/http-method.ts";
import { isRecord } from "../../src/lib/guards.ts";
import { bufferStream, cliEnv, withAuthedStateDir } from "../helpers-cli-state.ts";
import { outputDouble } from "../helpers-output-double.ts";
import { ACCEPTED, DRAFT, DRAFT_ID, FLEET_ID, LIBRARY_ID, MESSAGE, MODEL, PRIVATE_TEXT, REVIEW, REVISION_ID,
  WORKSPACE, withApi, problem } from "./fixture.ts";

const DEFINITION = REVIEW.effective.definition;
const DRAFTS = `/v1/workspaces/${WORKSPACE}/drafts`;
const SOURCE = {kind: DRAFT_SOURCE.definition, definition: DEFINITION};
type Call = { method: string; path: string; body: unknown };

let dir = "";
let valid = "";

beforeAll(async () => {
  dir = await mkdtemp(join(tmpdir(), "agentsfleet-definition-"));
  valid = join(dir, "definition.json");
  await Bun.write(valid, JSON.stringify(DEFINITION, null, 2));
});

afterAll(async () => { await rm(dir, {recursive: true, force: true}); });

async function invoke(origin: string, argv: string[]) {
  return withAuthedStateDir({ workspaceId: WORKSPACE, apiUrl: origin }, async () => {
    const stdout = bufferStream();
    const stderr = bufferStream();
    const code = await runCli(["--api", origin, "--json", "chat", ...argv], {
      stdout: stdout.stream, stderr: stderr.stream, env: cliEnv({ NO_COLOR: "1" }),
    });
    return { code, out: stdout.read(), err: stderr.read() };
  });
}

async function record(request: Request, calls: Call[]): Promise<void> {
  calls.push({method: request.method, path: new URL(request.url).pathname,
    body: request.method === HTTP_METHOD.get ? null : await request.json()});
}

function operationOf(body: unknown): string {
  const key = isRecord(body) ? body.operation_id : undefined;
  if (typeof key !== "string") throw new Error("the creation carried no operation identity");
  return key;
}

async function write(name: string, text: string): Promise<string> {
  const path = join(dir, name);
  await Bun.write(path, text);
  return path;
}

test("a Definition file opens its draft without a first message and reviews it at once", async () => {
  const calls: Call[] = [];
  await withApi(async (request) => { await record(request, calls); return Response.json(DRAFT); }, async (_, origin) => {
    const result = await invoke(origin, ["--definition", valid, "--model", MODEL, "--no-input", "--review"]);
    expect(result.code).toBe(0);
    expect(calls.map(({method, path}) => `${method} ${path}`)).toEqual([`POST ${DRAFTS}`, `GET ${DRAFTS}/${DRAFT_ID}`]);
    expect(calls[0]?.body).toEqual({source: SOURCE, authoring_model_ref: MODEL, operation_id: operationOf(calls[0]?.body)});
    const printed = result.out.trim().split("\n").map((line): unknown => JSON.parse(line));
    // pin test: literal is the contract
    expect(printed).toEqual([{kind: "chat_created", draft_id: DRAFT_ID}, REVIEW]);
  });
});

test("a first message travels with the Definition in one atomic creation", async () => {
  const calls: Call[] = [];
  await withApi(async (request) => {
    await record(request, calls);
    if (request.method === HTTP_METHOD.post) return Response.json({...DRAFT, accepted: ACCEPTED});
    return Response.json(new URL(request.url).pathname.endsWith("/messages") ? {items: [MESSAGE], next_cursor: null} : DRAFT);
  }, async (_, origin) => {
    const result = await invoke(origin, ["--definition", valid, "--model", MODEL, "--no-input", "--message", PRIVATE_TEXT]);
    expect(result.code).toBe(0);
    expect(calls.filter(({method}) => method === HTTP_METHOD.post)).toHaveLength(1);
    expect(calls[0]?.body).toEqual({source: SOURCE, authoring_model_ref: MODEL, message: PRIVATE_TEXT,
      operation_id: operationOf(calls[0]?.body)});
  });
});

test("the file bound is the daemon's Definition envelope bound", () => {
  // pin test: literal is the contract — afd_fleet_runtime's MAX_DEFINITION_BYTES, 6 × (32,768 + 32,768) + 65,536.
  expect(MAX_DEFINITION_BYTES).toBe(458_752);
});

const UNUSABLE: ReadonlyArray<readonly [string, () => Promise<string>, string]> = [
  ["missing", async () => join(dir, "absent.json"), "cannot be read"],
  ["directory", async () => dir, "cannot be read"],
  ["malformed", () => write("broken.json", '{"schema_version": 1,'), "is not valid JSON"],
  ["array", () => write("array.json", "[]"), "must hold one JSON object"],
  ["null", () => write("null.json", "null"), "must hold one JSON object"],
  ["oversized", () => write("large.json", " ".repeat(MAX_DEFINITION_BYTES + 1)), `at most ${MAX_DEFINITION_BYTES}`],
];

for (const [label, make, detail] of UNUSABLE) {
  test(`a ${label} Definition file is refused with its path before any request`, async () => {
    const path = await make();
    const seen: string[] = [];
    await withApi((request) => { seen.push(request.url); return problem(500); }, async (_, origin) => {
      const result = await invoke(origin, ["--definition", path, "--model", MODEL, "--no-input", "--review"]);
      expect(result.code).not.toBe(0);
      expect(result.err).toContain(path);
      expect(result.err).toContain(detail);
      expect(result.out).toBe("");
      expect(seen).toEqual([]);
    });
  });
}

const CONFLICTS: ReadonlyArray<readonly [string, ReadonlyArray<string>, string]> = [
  ["a resumed draft", [DRAFT_ID, "--no-input"], "A resumed draft keeps its source"],
  ["--fleet", ["--fleet", FLEET_ID, "--setup-revision", "1", "--model", MODEL, "--no-input"], "Choose one source"],
  ["--library", ["--library", LIBRARY_ID, "--revision", REVISION_ID, "--model", MODEL, "--no-input"], "Choose one source"],
];

for (const [label, args, detail] of CONFLICTS) {
  test(`--definition with ${label} is refused before any request`, async () => {
    const seen: string[] = [];
    await withApi((request) => { seen.push(request.url); return problem(500); }, async (_, origin) => {
      const result = await invoke(origin, ["--definition", valid, ...args]);
      expect(result.code).not.toBe(0);
      expect(result.err).toContain(detail);
      expect(seen).toEqual([]);
    });
  });
}

test("an unknown Definition creation is recovered by a later invocation with its reported identity", async () => {
  const calls: Call[] = [];
  await withApi(async (request) => {
    await record(request, calls);
    return calls.length === 1 ? problem(503) : Response.json(DRAFT);
  }, async (_, origin) => {
    const args = ["--definition", valid, "--model", MODEL, "--no-input"];
    const failed = await invoke(origin, args);
    expect(failed.code).not.toBe(0);
    const key = operationOf(calls[0]?.body);
    expect(failed.err).toContain("chat_operation_uncertain");
    expect(failed.err).toContain(key);
    const recovered = await invoke(origin, [...args, "--operation-id", key]);
    expect(recovered.code).toBe(0);
    expect(calls[1]?.body).toEqual(calls[0]?.body);
    expect(recovered.out).toContain(DRAFT_ID);
  });
});

test("/retry resends an uncertain creation unchanged and refuses a message until it resolves", async () => {
  const calls: Call[] = [];
  await withApi(async (request) => {
    await record(request, calls);
    return calls.length === 1 ? problem(503) : Response.json(DRAFT);
  }, async (transport) => {
    const created: string[] = [];
    const output = {...outputDouble(), success: (text: string) => Effect.sync(() => { created.push(text); })};
    const session = new DraftSession(transport, MODEL, SOURCE);
    const actions = new ChatActions(session, new SetupSession(transport), output, () => Effect.void);
    expect((await Effect.runPromise(Effect.flip(actions.create())))._tag).toBe("ServerError");
    expect(actions.pending).toBe(true);
    expect((await Effect.runPromise(Effect.flip(actions.message(PRIVATE_TEXT))))._tag).toBe("ValidationError");
    await Effect.runPromise(actions.retry());
    expect(calls.map(({path}) => path)).toEqual([DRAFTS, DRAFTS]);
    expect(calls[1]?.body).toEqual(calls[0]?.body);
    expect(created).toEqual([`Draft ${DRAFT_ID} created`]);
    expect(actions.pending).toBe(false);
    expect(actions.id).toBe(DRAFT_ID);
  });
});
