import { expect, test } from "bun:test";
import { Effect } from "effect";
import { DraftSession, DRAFT_SOURCE } from "../../src/commands/chat/session.ts";
import { HTTP_METHOD } from "../../src/constants/http-method.ts";
import { MESSAGE_ROLE, MESSAGE_STATE } from "../../src/commands/chat/protocol.ts";
import { ACCEPTED, DRAFT, DRAFT_ID, MESSAGE, MODEL, PRIVATE_TEXT, EMPTY_PAGE, TURN_ID, withApi, problem, WORKSPACE } from "./fixture.ts";

/** A finished request, which Stop leaves alone. */
const FINISHED_TURN = "0199a801-1000-7000-8000-000000000009";
/** A queued request whose answer lands before its Stop does. */
const QUEUED_TURN = "0199a801-1000-7000-8000-00000000000a";

test("a lost first answer retries one atomic draft and message with the same body key", async () => {
  const bodies: unknown[] = [];
  await withApi(async (request) => {
    expect(new URL(request.url).pathname).toBe(`/v1/workspaces/${WORKSPACE}/drafts`);
    expect(request.headers.get("Authorization")).toBe("Bearer local-chat-test");
    expect(request.headers.has("Idempotency-Key")).toBe(false);
    bodies.push(await request.json());
    return bodies.length === 1 ? problem(503) : Response.json({...DRAFT, accepted: ACCEPTED});
  }, async (transport) => {
    const session = new DraftSession(transport, MODEL, {kind: DRAFT_SOURCE.blank});
    expect((await Effect.runPromise(Effect.flip(session.send(PRIVATE_TEXT))))._tag).toBe("ServerError");
    expect(session.id).toBeUndefined();
    expect(session.pending).toBe(true);
    expect((await Effect.runPromise(session.retry())).draft_id).toBe(DRAFT_ID);
    expect(bodies[0]).toEqual(bodies[1]);
    expect(session.pending).toBe(false);
  });
});

test("malformed creation cannot claim acceptance or forget its replay key", async () => {
  await withApi(() => Response.json(DRAFT), async (transport) => {
    const session = new DraftSession(transport, MODEL, {kind: DRAFT_SOURCE.blank});
    const failure = await Effect.runPromise(Effect.flip(session.send(PRIVATE_TEXT)));
    expect(failure._tag).toBe("ServerError");
    expect(session.id).toBeUndefined();
    expect(session.pending).toBe(true);
  });
});

test("a candidate without a validated review cannot be presented as ready to save", async () => {
  const {review: _review, ...unreviewed} = DRAFT;
  await withApi(() => Response.json(unreviewed), async transport => {
    const session = new DraftSession(transport, MODEL, {kind: DRAFT_SOURCE.blank}, DRAFT_ID);
    const failure = await Effect.runPromise(Effect.flip(session.review()));
    expect(failure._tag).toBe("ValidationError");
    expect(failure.detail).toContain("no validated candidate");
    expect(session.pending).toBe(false);
  });
});

test("resume sends a body key, edits under the fresh tag, and stops each unfinished turn by its PATCH", async () => {
  const seen: {method: string; path: string; body: unknown; tag: string | null}[] = [];
  const page = {items: [
    {...MESSAGE, sequence: "11", turn_id: FINISHED_TURN},
    {...MESSAGE, sequence: "12", state: MESSAGE_STATE.running},
    {...MESSAGE, sequence: "12", role: MESSAGE_ROLE.assistant, state: MESSAGE_STATE.running},
    {...MESSAGE, sequence: "13", turn_id: QUEUED_TURN, state: MESSAGE_STATE.queued},
  ], next_cursor: null};
  await withApi(async (request) => {
    const path = new URL(request.url).pathname;
    const body = request.method === HTTP_METHOD.get ? null : await request.json();
    seen.push({method: request.method, path, body, tag: request.headers.get("If-Match")});
    if (path.endsWith(`/turns/${QUEUED_TURN}`)) return problem(409);
    if (path.includes("/turns/")) return Response.json({sequence: "12", state: MESSAGE_STATE.cancelled});
    if (path.endsWith("/messages")) return request.method === HTTP_METHOD.get
      ? Response.json(page) : Response.json({...ACCEPTED, sequence: "12"});
    return Response.json({...DRAFT, etag: '"draft-fresh"'});
  }, async (transport) => {
    const session = new DraftSession(transport, MODEL, {kind: DRAFT_SOURCE.blank}, DRAFT_ID);
    await Effect.runPromise(session.send(PRIVATE_TEXT));
    await Effect.runPromise(session.patch({authoring_model_ref: "owner/replacement"}));
    await Effect.runPromise(session.stop());
    expect(seen[0]?.path).toEndWith(`/${DRAFT_ID}/messages`);
    expect(seen[2]?.tag).toBe('"draft-fresh"');
    expect(seen[2]?.body).toEqual({authoring_model_ref: "owner/replacement"});
    const messages = seen[0]?.path ?? "";
    const turn = (id: string) => messages.replace(/\/messages$/, `/turns/${id}`);
    expect(seen.slice(3).map(({method, path, body}) => ({method, path, body}))).toEqual([
      {method: HTTP_METHOD.get, path: messages, body: null},
      {method: HTTP_METHOD.patch, path: turn(TURN_ID), body: {status: "cancelled"}},
      // The answer won this one's race: a 409 is a finished turn, and Stop goes on.
      {method: HTTP_METHOD.patch, path: turn(QUEUED_TURN), body: {status: "cancelled"}},
    ]);
  });
});

for (const status of [401, 403, 404, 410]) {
  test(`access refusal ${status} clears cached candidate and uncertain private text`, async () => {
    let deny = false;
    await withApi(() => deny ? problem(status) : Response.json(DRAFT), async (transport) => {
      const session = new DraftSession(transport, MODEL, {kind: DRAFT_SOURCE.blank}, DRAFT_ID);
      await Effect.runPromise(session.refresh());
      expect(session.draft?.candidate).toBeDefined();
      deny = true;
      await Effect.runPromise(Effect.flip(session.refresh()));
      expect(session.id).toBeUndefined();
      expect(session.draft).toBeUndefined();
      expect(session.pending).toBe(false);
    });
  });
}

test("delete clears private state and absent draft actions do not make requests", async () => {
  let requests = 0;
  await withApi((request) => {
    requests++;
    return request.method === HTTP_METHOD.delete ? new Response(null, {status: 204}) : Response.json(EMPTY_PAGE);
  }, async (transport) => {
    const session = new DraftSession(transport, MODEL, {kind: DRAFT_SOURCE.blank}, DRAFT_ID);
    await Effect.runPromise(session.remove());
    await Effect.runPromise(Effect.flip(session.refresh()));
    await Effect.runPromise(Effect.flip(session.stop()));
    await Effect.runPromise(Effect.flip(session.page()));
    expect(requests).toBe(1);
    expect(session.id).toBeUndefined();
  });
});


test("a separate invocation recovers an unknown creation with the reported operation identity", async () => {
  const bodies: unknown[] = [];
  await withApi(async (request) => {
    bodies.push(await request.json());
    return bodies.length === 1 ? problem(503) : Response.json({...DRAFT, accepted: ACCEPTED});
  }, async (transport) => {
    const first = new DraftSession(transport, MODEL, {kind: DRAFT_SOURCE.blank});
    await Effect.runPromise(Effect.flip(first.send(PRIVATE_TEXT)));
    const operation = first.operationId;
    expect(operation).toBeString(); first.clear();
    const next = new DraftSession(transport, MODEL, {kind: DRAFT_SOURCE.blank}, undefined, operation);
    expect(await Effect.runPromise(next.send(PRIVATE_TEXT))).toEqual({draft_id: DRAFT_ID, sequence: ACCEPTED.sequence});
    expect(bodies[0]).toEqual(bodies[1]); expect(next.pending).toBe(false);
  });
});
