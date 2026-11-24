import { expect, test } from "bun:test";
import { runCli } from "../../src/cli.ts";
import { MESSAGE_ROLE, MESSAGE_STATE } from "../../src/commands/chat/protocol.ts";
import { bufferStream, cliEnv, withAuthedStateDir } from "../helpers-cli-state.ts";
import { ACCEPTED, DRAFT, DRAFT_ID, MESSAGE, MODEL, PRIVATE_TEXT, WORKSPACE, withApi } from "./fixture.ts";

const DRAFT_PATH = `/v1/workspaces/${WORKSPACE}/drafts/${DRAFT_ID}`;
const COMPLETED_NOTICE = 'event: message.completed\ndata: {"sequence":"1","state":"completed"}\n\n';

test("should wait on the draft's own event stream while a sent message is queued", async () => {
  const streams: { path: string; accept: string | null; bearer: boolean }[] = [];
  let reads = 0;
  await withApi((request) => {
    const path = new URL(request.url).pathname;
    if (request.method === "POST") return Response.json({...DRAFT, accepted: ACCEPTED}, {status: 201});
    if (path.endsWith("/turns/stream")) {
      streams.push({path, accept: request.headers.get("Accept"),
        bearer: (request.headers.get("Authorization") ?? "").startsWith("Bearer ")});
      return new Response(COMPLETED_NOTICE, {headers: {"Content-Type": "text/event-stream"}});
    }
    if (path.endsWith("/messages")) {
      reads++;
      return Response.json({items: reads === 1 ? [{...MESSAGE, state: MESSAGE_STATE.queued}]
        : [MESSAGE, {...MESSAGE, role: MESSAGE_ROLE.assistant, text: "the answer"}], next_cursor: null});
    }
    return Response.json(DRAFT);
  }, async (_transport, origin) => {
    const result = await withAuthedStateDir({workspaceId: WORKSPACE, apiUrl: origin}, async () => {
      const stdout = bufferStream();
      const code = await runCli(["--api", origin, "chat", "--model", MODEL, "--no-input", "--message", PRIVATE_TEXT], {
        stdout: stdout.stream, stderr: bufferStream().stream, env: cliEnv({NO_COLOR: "1"}),
      });
      return {code, out: stdout.read()};
    });
    expect(result.code).toBe(0);
    expect(result.out).toContain("Assistant: the answer");
  });
  expect(streams).toEqual([{path: `${DRAFT_PATH}/turns/stream`, accept: "text/event-stream", bearer: true}]);
  expect(reads).toBe(2);
});

test("should print a resumed draft's transcript, a reply still in progress included", async () => {
  const reads: string[] = [];
  await withApi((request) => {
    const url = new URL(request.url);
    if (request.method !== "GET") return Response.json({}, {status: 500});
    if (!url.pathname.endsWith("/messages")) return Response.json(DRAFT);
    reads.push(url.pathname);
    return Response.json({items: [MESSAGE, {...MESSAGE, role: MESSAGE_ROLE.assistant, state: MESSAGE_STATE.running, text: undefined}],
      next_cursor: null});
  }, async (_transport, origin) => {
    const result = await withAuthedStateDir({workspaceId: WORKSPACE, apiUrl: origin}, async () => {
      const stdout = bufferStream();
      const code = await runCli(["--api", origin, "chat", DRAFT_ID, "--no-input"], {
        stdout: stdout.stream, stderr: bufferStream().stream, env: cliEnv({NO_COLOR: "1"}),
      });
      return {code, out: stdout.read()};
    });
    expect(result.code).toBe(0);
    expect(result.out).toContain(`You: ${PRIVATE_TEXT}`);
    expect(result.out).toContain(`Message ${MESSAGE.sequence}: ${MESSAGE_STATE.running}`);
  });
  expect(reads).toEqual([`${DRAFT_PATH}/messages`]);
});

test.each([
  ["stop without a draft", ["--model", MODEL, "--stop"], "Use --stop by itself with a draft ID"],
  ["stop with a message", [DRAFT_ID, "--stop", "--message", PRIVATE_TEXT], "Use --stop by itself with a draft ID"],
  ["stop with a save", [DRAFT_ID, "--stop", "--save"], "Use --stop by itself with a draft ID"],
  ["name without a review", [DRAFT_ID, "--name", "Release checklist"], "--name requires --review, --save or --apply"],
  ["digest without a write", [DRAFT_ID, "--review-digest", "reviewed"], "--review-digest requires --save or --apply"],
  ["empty retry identity", [DRAFT_ID, "--message", PRIVATE_TEXT, "--operation-id", ""], "operation_id is required"],
  ["empty candidate identity", [DRAFT_ID, "--candidate-id", ""], "draft_candidate_id is required"],
  ["empty draft identity", [""], "draft_id is required"],
  ["empty fleet identity", ["--model", MODEL, "--fleet", ""], "fleet_id is required"],
  ["empty revision without a fleet", [DRAFT_ID, "--setup-revision", ""], "Revision selection requires its matching source"],
  ["unpaired empty setup tag", [DRAFT_ID, "--setup-etag", ""], "Recovery identity fields require their matching --operation-id setup action"],
] as const)("rejects %s before reading or writing a draft", async (_scenario, args, message) => {
  const requests: string[] = [];
  await withApi(request => {
    requests.push(request.url);
    return new Response(null, {status: 500});
  }, async (_transport, origin) => withAuthedStateDir({workspaceId: WORKSPACE, apiUrl: origin}, async () => {
    const stdout = bufferStream();
    const stderr = bufferStream();
    const code = await runCli(["chat", ...args, "--no-input", "--json"], {
      stdout: stdout.stream, stderr: stderr.stream, env: cliEnv({AGENTSFLEET_API_URL: origin}),
    });
    expect(code).toBe(4);
    expect(stdout.read()).toBe("");
    const error = JSON.parse(stderr.read()).error;
    expect(error).toMatchObject({code: "INVALID_ARGUMENT", message});
    if (args.some(arg => arg === "--stop")) expect(error.suggestion).toBe("agentsfleet chat <draft-id> --stop");
    expect(requests).toEqual([]);
  }));
});
