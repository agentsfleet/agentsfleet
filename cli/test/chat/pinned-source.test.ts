// A pinned Library revision or fleet setup revision opens its draft without a
// first message, so `--no-input` can review and act on it with no prompt.

import { expect, test } from "bun:test";
import { runCli } from "../../src/cli.ts";
import { DRAFT_SOURCE } from "../../src/commands/chat/session.ts";
import { START } from "../../src/commands/chat/setup-session.ts";
import { HTTP_METHOD } from "../../src/constants/http-method.ts";
import { isRecord } from "../../src/lib/guards.ts";
import { bufferStream, cliEnv, withAuthedStateDir } from "../helpers-cli-state.ts";
import { CANDIDATE_ID, DRAFT, DRAFT_ID, FLEET, FLEET_ID, LIBRARY_ID, MODEL, PRIVATE_TEXT, REVIEW, REVISION_ID,
  WORKSPACE, withApi, problem } from "./fixture.ts";

const DRAFTS = `/v1/workspaces/${WORKSPACE}/drafts`;
const FLEETS = `/v1/workspaces/${WORKSPACE}/fleets`;
const LIBRARY_ARGS = ["--library", LIBRARY_ID, "--revision", REVISION_ID, "--model", MODEL, "--no-input"];
const FLEET_ARGS = ["--fleet", FLEET_ID, "--setup-revision", FLEET.setup_revision, "--model", MODEL, "--no-input"];
const LIBRARY_SOURCE = {kind: DRAFT_SOURCE.library, library_id: LIBRARY_ID, revision_id: REVISION_ID};
const FLEET_SOURCE = {kind: DRAFT_SOURCE.fleet, fleet_id: FLEET_ID, setup_revision: FLEET.setup_revision};
// A fleet-sourced draft names its fleet and the setup revision it improves, so
// review reads that fleet to capture the setup tag Apply must send.
const LINKED_DRAFT = {...DRAFT, linked_fleet_id: FLEET_ID,
  review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: FLEET.setup_revision}}};
const APPLIED = {fleet_id: FLEET_ID, setup_revision: "2", setup_etag: '"setup-2"', etag: '"fleet-2"', schedules: []};
type Call = { method: string; path: string; body: unknown; ifMatch: string | null };

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
  calls.push({method: request.method, path: new URL(request.url).pathname, ifMatch: request.headers.get("if-match"),
    body: request.method === HTTP_METHOD.get ? null : await request.json()});
}

function operationOf(body: unknown): string {
  const key = isRecord(body) ? body.operation_id : undefined;
  if (typeof key !== "string") throw new Error("the creation carried no operation identity");
  return key;
}

test("a pinned Library revision is reviewed and saved with no first message and no prompt", async () => {
  const calls: Call[] = [];
  await withApi(async (request) => {
    await record(request, calls);
    return Response.json(new URL(request.url).pathname === FLEETS ? FLEET : DRAFT);
  }, async (_, origin) => {
    const result = await invoke(origin, [...LIBRARY_ARGS, "--review", "--save", "--review-digest", REVIEW.review_digest]);
    expect(result.code).toBe(0);
    expect(calls.map(({method, path}) => `${method} ${path}`))
      .toEqual([`POST ${DRAFTS}`, `GET ${DRAFTS}/${DRAFT_ID}`, `POST ${FLEETS}`]);
    expect(calls[0]?.body).toEqual({source: LIBRARY_SOURCE, authoring_model_ref: MODEL, operation_id: operationOf(calls[0]?.body)});
    expect(calls[2]?.body).toEqual({source: {kind: "draft", draft_candidate_id: CANDIDATE_ID},
      name: REVIEW.effective.instance_name, review_digest: REVIEW.review_digest, start: {kind: START.none}});
    expect(result.out).toContain(`"draft_id":"${DRAFT_ID}"`);
  });
});

test("a fleet setup revision is reviewed and applied under its observed setup tag with no first message", async () => {
  const calls: Call[] = [];
  await withApi(async (request) => {
    await record(request, calls);
    const path = new URL(request.url).pathname;
    if (path === `${FLEETS}/${FLEET_ID}/revisions`) return Response.json(APPLIED);
    return Response.json(path === `${FLEETS}/${FLEET_ID}` ? FLEET : LINKED_DRAFT);
  }, async (_, origin) => {
    const result = await invoke(origin, [...FLEET_ARGS, "--review", "--apply", "--review-digest", REVIEW.review_digest]);
    expect(result.code).toBe(0);
    expect(calls[0]?.body).toEqual({source: FLEET_SOURCE, authoring_model_ref: MODEL, operation_id: operationOf(calls[0]?.body)});
    const applied = calls.at(-1);
    expect(`${applied?.method} ${applied?.path}`).toBe(`POST ${FLEETS}/${FLEET_ID}/revisions`);
    expect(applied?.ifMatch).toBe(FLEET.setup_etag);
    expect(applied?.body).toEqual({draft_candidate_id: CANDIDATE_ID, review_digest: REVIEW.review_digest});
  });
});

test("a Library first message still travels with its source in one atomic creation", async () => {
  const calls: Call[] = [];
  await withApi(async (request) => {
    await record(request, calls);
    return problem(503);
  }, async (_, origin) => {
    await invoke(origin, [...LIBRARY_ARGS, "--message", PRIVATE_TEXT]);
    expect(calls[0]?.body).toEqual({source: LIBRARY_SOURCE, authoring_model_ref: MODEL, message: PRIVATE_TEXT,
      operation_id: operationOf(calls[0]?.body)});
    expect(calls).toHaveLength(1);
  });
});

const CONFLICTS: ReadonlyArray<readonly [string, ReadonlyArray<string>, string]> = [
  ["--library with --fleet", [...LIBRARY_ARGS, "--fleet", FLEET_ID, "--setup-revision", "1"], "Choose one source"],
  ["--library on a resumed draft", [DRAFT_ID, "--library", LIBRARY_ID, "--revision", REVISION_ID, "--no-input"],
    "A resumed draft keeps its source"],
  ["--fleet on a resumed draft", [DRAFT_ID, "--fleet", FLEET_ID, "--setup-revision", "1", "--no-input"],
    "A resumed draft keeps its source"],
  ["--library without --revision", ["--library", LIBRARY_ID, "--model", MODEL, "--no-input", "--review"],
    "--library requires --revision"],
  ["--operation-id with --review", [...LIBRARY_ARGS, "--review", "--operation-id", DRAFT_ID], "--operation-id recovers"],
];

for (const [label, args, detail] of CONFLICTS) {
  test(`${label} is refused before any request`, async () => {
    const seen: string[] = [];
    await withApi((request) => { seen.push(request.url); return problem(500); }, async (_, origin) => {
      const result = await invoke(origin, [...args]);
      expect(result.code).not.toBe(0);
      expect(result.err).toContain(detail);
      expect(seen).toEqual([]);
    });
  });
}

test("a blank draft still needs a message outside an interactive terminal", async () => {
  const seen: string[] = [];
  await withApi((request) => { seen.push(request.url); return problem(500); }, async (_, origin) => {
    const result = await invoke(origin, ["--model", MODEL, "--no-input"]);
    expect(result.code).not.toBe(0);
    expect(result.err).toContain("A message is required");
    expect(seen).toEqual([]);
  });
});

for (const [label, args] of [["Library", LIBRARY_ARGS], ["fleet", FLEET_ARGS]] as const) {
  test(`an unknown ${label} creation is recovered by a later invocation with its reported identity`, async () => {
    const calls: Call[] = [];
    await withApi(async (request) => {
      await record(request, calls);
      return calls.length === 1 ? problem(503) : Response.json(DRAFT);
    }, async (_, origin) => {
      const failed = await invoke(origin, [...args]);
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
}
