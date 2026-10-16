import { expect, test } from "bun:test";
import { Effect, Exit } from "effect";
import { ChatActions } from "../../src/commands/chat/actions.ts";
import { runChatRepl } from "../../src/commands/chat/repl.ts";
import { DraftSession, DRAFT_SOURCE } from "../../src/commands/chat/session.ts";
import { SetupSession } from "../../src/commands/chat/setup-session.ts";
import { CLI_ERROR_TAG } from "../../src/errors/index.ts";
import { outputDouble } from "../helpers-output-double.ts";
import { DRAFT, DRAFT_ID, MODEL, PRIVATE_TEXT, REVIEW, WORKSPACE, problem, withApi } from "./fixture.ts";

// pin test: literal is the contract — the action list a person reads at the prompt.
const HELP = "/review [instance-name] · /save · /run <message> · /publish <label> · /apply · /model <ref> · /stop · /retry · /delete · /quit. Ctrl+C stops the reply being followed.";
const PROMPT = "you › ";
const DRAFT_PATH = `/v1/workspaces/${WORKSPACE}/drafts/${DRAFT_ID}`;
const MISSING_REVIEW = "Inspect the current setup before this action. use /review [instance-name], inspect the displayed setup and access, then repeat the explicit action";
const CHECK = {kind: "credential", subject: "github", state: "missing", next_action: {kind: "create", href: "/settings/credentials"}};
const REVIEWED_DRAFT = {...DRAFT, review: {...REVIEW, readiness: [CHECK]}};
type Call = { readonly method: string; readonly path: string; readonly tag: string | null; readonly body: unknown };

function terminal(lines: readonly (string | null)[]) {
  const queue = [...lines];
  const prompts: string[] = [];
  const rendered: string[] = [];
  const record = (kind: string) => (text: string) => Effect.sync(() => { rendered.push(`${kind}: ${text}`); });
  return {
    prompts, rendered, unread: () => queue.length,
    input: {readLine: (prompt: string) => Effect.sync(() => { prompts.push(prompt); return queue.shift() ?? null; })},
    output: {...outputDouble(), info: record("info"), warn: record("warn"), success: record("success")},
  };
}

async function call(request: Request): Promise<Call> {
  const text = request.method === "GET" || request.method === "DELETE" ? "" : await request.text();
  return {method: request.method, path: new URL(request.url).pathname, tag: request.headers.get("If-Match"),
    body: text ? JSON.parse(text) : null};
}

test("should keep the conversation open after refused actions, touch nothing and exit 0 on /quit", async () => {
  const calls: Call[] = [];
  const term = terminal(["   ", "/help", "/frobnicate now", "/review", "/save", "/run", "/run start now", "/publish", "/publish Argo",
    "/model", "/quit", "never read"]);
  await withApi(async (request) => { calls.push(await call(request)); return problem(500); }, async (transport) => {
    const actions = new ChatActions(new DraftSession(transport, MODEL, {kind: DRAFT_SOURCE.blank}), new SetupSession(transport),
      term.output, () => Effect.void);
    expect(await Effect.runPromise(runChatRepl(actions, term.input, term.output))).toBe(0);
  });
  expect(calls).toEqual([]);
  expect(term.unread()).toBe(1);
  expect(term.prompts).toEqual(Array(11).fill(PROMPT));
  expect(term.rendered).toEqual([
    `info: ${HELP}`, `info: ${HELP}`, `warn: Unknown chat action. ${HELP}`,
    "warn: This conversation has no accessible draft. send your first message or resume an accessible draft",
    `warn: ${MISSING_REVIEW}`, `warn: Missing run message. ${HELP}`, `warn: ${MISSING_REVIEW}`,
    `warn: Missing Library label. ${HELP}`, `warn: ${MISSING_REVIEW}`, `warn: Missing authoring model reference. ${HELP}`,
  ]);
});

test("should route each terminal action to its own draft request and pin the next turn's model", async () => {
  const calls: Call[] = [];
  const term = terminal(["/review argo-two", "/apply", "/stop", "/model owner/other", "/delete", "/retry", null]);
  await withApi(async (request) => {
    calls.push(await call(request));
    if (request.method === "POST" || request.method === "DELETE") return new Response(null, {status: 204});
    return Response.json(REVIEWED_DRAFT);
  }, async (transport) => {
    const actions = new ChatActions(new DraftSession(transport, MODEL, {kind: DRAFT_SOURCE.blank}, DRAFT_ID),
      new SetupSession(transport), term.output, () => Effect.void);
    expect(await Effect.runPromise(runChatRepl(actions, term.input, term.output))).toBe(0);
    expect(actions.id).toBeUndefined();
  });
  expect(calls).toEqual([
    {method: "GET", path: DRAFT_PATH, tag: null, body: null},
    {method: "PATCH", path: DRAFT_PATH, tag: DRAFT.etag, body: {instance_name: "argo-two"}},
    {method: "GET", path: DRAFT_PATH, tag: null, body: null},
    {method: "POST", path: `${DRAFT_PATH}/cancellations`, tag: null, body: {through_sequence: "0"}},
    {method: "GET", path: DRAFT_PATH, tag: null, body: null},
    {method: "PATCH", path: DRAFT_PATH, tag: DRAFT.etag, body: {authoring_model_ref: "owner/other"}},
    {method: "DELETE", path: DRAFT_PATH, tag: null, body: null},
  ]);
  expect(term.rendered).toContain(`info: Instance: ${REVIEW.effective.instance_name}`);
  expect(term.rendered).toContain("info: credential github: missing → /settings/credentials");
  expect(term.rendered).toContain(`warn: ${MISSING_REVIEW}`);
  expect(term.rendered.at(-1)).toBe("warn: There is no uncertain operation to retry. continue chatting or review the current setup");
});

test("should end the conversation on an access refusal instead of returning to the prompt", async () => {
  const term = terminal(["/stop", "/quit"]);
  await withApi(() => problem(403), async (transport) => {
    const actions = new ChatActions(new DraftSession(transport, MODEL, {kind: DRAFT_SOURCE.blank}, DRAFT_ID),
      new SetupSession(transport), term.output, () => Effect.void);
    const exit = await Effect.runPromiseExit(Effect.flip(runChatRepl(actions, term.input, term.output)));
    if (!Exit.isSuccess(exit)) throw new Error("an access refusal must surface as the conversation's failure");
    expect(exit.value._tag).toBe(CLI_ERROR_TAG.server);
    expect(exit.value._tag === CLI_ERROR_TAG.server && exit.value.status).toBe(403);
    expect(actions.id).toBeUndefined();
  });
  expect(term.unread()).toBe(1);
  expect(term.rendered).toEqual([`info: ${HELP}`]);
});

test("should exit 1 and name the retained operation when a sent message has an unknown outcome", async () => {
  const calls: Call[] = [];
  const term = terminal([PRIVATE_TEXT, "/model owner/other", "/quit"]);
  await withApi(async (request) => { calls.push(await call(request)); return problem(503); }, async (transport) => {
    const actions = new ChatActions(new DraftSession(transport, MODEL, {kind: DRAFT_SOURCE.blank}), new SetupSession(transport),
      term.output, () => Effect.void);
    expect(await Effect.runPromise(runChatRepl(actions, term.input, term.output))).toBe(1);
    expect(actions.pending).toBe(true);
  });
  // One write: the uncertain send is retained, never resent, and /model refuses to change the draft beneath it.
  expect(calls.map((entry) => `${entry.method} ${entry.path}`)).toEqual([`POST /v1/workspaces/${WORKSPACE}/drafts`]);
  const sent = calls[0]?.body;
  const key = typeof sent === "object" && sent !== null && "operation_id" in sent ? sent.operation_id : undefined;
  expect(typeof key).toBe("string");
  const recovery = `warn: Chat outcome is unknown. Repeat the same message, source and model with --operation-id ${String(key)}.`;
  expect(term.rendered.filter((line) => line === recovery)).toHaveLength(3);
  expect(term.rendered).toContain("warn: A chat operation has an unknown outcome. use /retry before changing this draft");
  expect(term.rendered.join("\n")).not.toContain(PRIVATE_TEXT);
});
