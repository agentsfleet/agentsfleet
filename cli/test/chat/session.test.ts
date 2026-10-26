import { expect, test } from "bun:test";
import { Effect } from "effect";
import { DraftSession, DRAFT_SOURCE } from "../../src/commands/chat/session.ts";
import { HTTP_METHOD } from "../../src/constants/http-method.ts";
import { ACCEPTED, DRAFT, DRAFT_ID, MODEL, PRIVATE_TEXT, EMPTY_PAGE, withApi, problem, WORKSPACE } from "./fixture.ts";

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

test("resume sends a body key, edits under the fresh tag, and cancels through the accepted sequence", async () => {
  const seen: {method: string; path: string; body: unknown; tag: string | null}[] = [];
  await withApi(async (request) => {
    const path = new URL(request.url).pathname;
    const body = request.method === HTTP_METHOD.get ? null : await request.json();
    seen.push({method: request.method, path, body, tag: request.headers.get("If-Match")});
    if (path.endsWith("/cancellations")) return Response.json({});
    if (path.endsWith("/messages")) return Response.json({...ACCEPTED, sequence: "12"});
    return Response.json({...DRAFT, etag: '"draft-fresh"'});
  }, async (transport) => {
    const session = new DraftSession(transport, MODEL, {kind: DRAFT_SOURCE.blank}, DRAFT_ID);
    await Effect.runPromise(session.send(PRIVATE_TEXT));
    await Effect.runPromise(session.patch({authoring_model_ref: "owner/replacement"}));
    await Effect.runPromise(session.stop());
    expect(seen[0]?.path).toEndWith(`/${DRAFT_ID}/messages`);
    expect(seen[2]?.tag).toBe('"draft-fresh"');
    expect(seen[2]?.body).toEqual({authoring_model_ref: "owner/replacement"});
    expect(seen[3]?.body).toEqual({through_sequence: "12"});
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
