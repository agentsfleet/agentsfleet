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
    if (path.endsWith("/events/stream")) {
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
  expect(streams).toEqual([{path: `${DRAFT_PATH}/events/stream`, accept: "text/event-stream", bearer: true}]);
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
