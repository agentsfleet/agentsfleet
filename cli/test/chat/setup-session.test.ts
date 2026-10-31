import { expect, test } from "bun:test";
import { Effect } from "effect";
import { SetupSession, START } from "../../src/commands/chat/setup-session.ts";
import { DRAFT, REVIEW, FLEET, FLEET_ID, PUBLICATION, CANDIDATE_ID, LIBRARY_ID, withApi, problem } from "./fixture.ts";

test("partial Save continues one exact reviewed operation and does not publish a Library entry", async () => {
  const calls: {path: string; key: string | null; body: unknown}[] = [];
  await withApi(async (request) => {
    calls.push({path: new URL(request.url).pathname, key: request.headers.get("Idempotency-Key"), body: await request.json()});
    return Response.json(calls.length === 1 ? {...FLEET, status: "installing"} : FLEET);
  }, async (transport) => {
    const setup = new SetupSession(transport);
    const saved = await Effect.runPromise(setup.save(DRAFT, REVIEW, {kind: START.none}));
    expect(saved.status).toBe("installing");
    const ready = await Effect.runPromise(setup.progress());
    expect(ready.id).toBe(saved.id);
    expect(ready.status).toBe(FLEET.status);
    expect(calls[0]).toEqual(calls[1]);
    expect(calls[0]?.body).toEqual({source: {kind: "draft", draft_candidate_id: CANDIDATE_ID},
      name: REVIEW.effective.instance_name, review_digest: REVIEW.review_digest, start: {kind: START.none}, origin: "cli"});
    expect(calls.every((call) => call.path.endsWith("/fleets"))).toBe(true);
  });
});

test("an unknown Save retains its body and key and a changed initial message cannot replace it", async () => {
  const calls: {key: string | null; body: unknown}[] = [];
  await withApi(async (request) => {
    calls.push({key: request.headers.get("Idempotency-Key"), body: await request.json()});
    return calls.length === 1 ? problem(503) : Response.json(FLEET);
  }, async (transport) => {
    const setup = new SetupSession(transport);
    const start = {kind: START.runNow, message: "check deployment"};
    await Effect.runPromise(Effect.flip(setup.save(DRAFT, REVIEW, start)));
    expect(setup.pending).toBe(true);
    await Effect.runPromise(Effect.flip(setup.save(DRAFT, REVIEW, {...start, message: "different"})));
    expect(calls).toHaveLength(1);
    expect((await Effect.runPromise(setup.progress())).id).toBe(FLEET_ID);
    expect(calls[0]).toEqual(calls[1]);
  });
});

test("publication and owned revision append are explicit and retain their separate key and Library tag", async () => {
  const calls: {path: string; key: string | null; tag: string | null; body: unknown}[] = [];
  await withApi(async (request) => {
    calls.push({path: new URL(request.url).pathname, key: request.headers.get("Idempotency-Key"),
      tag: request.headers.get("If-Match"), body: await request.json()});
    return calls.length === 1 ? problem(503) : Response.json(PUBLICATION);
  }, async (transport) => {
    const setup = new SetupSession(transport);
    await Effect.runPromise(Effect.flip(setup.publish(DRAFT, "Argo")));
    expect(await Effect.runPromise(setup.retryPublication())).toEqual(PUBLICATION);
    expect(calls[0]).toEqual(calls[1]);
    await Effect.runPromise(setup.publish(DRAFT, "", {id: LIBRARY_ID, etag: PUBLICATION.etag}));
    expect(calls[2]?.path).toEndWith(`/${LIBRARY_ID}/revisions`);
    expect(calls[2]?.tag).toBe(PUBLICATION.etag);
    expect(calls[2]?.body).toEqual({draft_candidate_id: CANDIDATE_ID});
    expect(calls[2]?.key).not.toBe(calls[1]?.key);
  });
});

test("Apply carries the inspected setup tag, candidate and digest through an uncertain answer", async () => {
  const calls: {path: string; key: string | null; tag: string | null; body: unknown}[] = [];
  const result = {fleet_id: FLEET_ID, setup_revision: "2", setup_etag: '"setup-2"', etag: FLEET.etag, schedules: []};
  await withApi(async (request) => {
    calls.push({path: new URL(request.url).pathname, key: request.headers.get("Idempotency-Key"),
      tag: request.headers.get("If-Match"), body: await request.json()});
    return calls.length === 1 ? problem(503) : Response.json(result);
  }, async (transport) => {
    const setup = new SetupSession(transport);
    await Effect.runPromise(Effect.flip(setup.apply({...DRAFT, linked_fleet_id: FLEET_ID}, REVIEW, FLEET.setup_etag)));
    expect(await Effect.runPromise(setup.retryApply())).toEqual(result);
    expect(calls[0]).toEqual(calls[1]);
    expect(calls[0]?.tag).toBe(FLEET.setup_etag);
    expect(calls[0]?.path).toEndWith(`/${FLEET_ID}/revisions`);
    expect(calls[0]?.body).toEqual({draft_candidate_id: CANDIDATE_ID, review_digest: REVIEW.review_digest, origin: "cli"});
  });
});

test("candidate-less mutations refuse locally and clearing disposes of partial setup state", async () => {
  let requests = 0;
  await withApi(() => { requests++; return Response.json(FLEET); }, async (transport) => {
    const setup = new SetupSession(transport);
    const {candidate: _candidate, ...blank} = DRAFT;
    await Effect.runPromise(Effect.flip(setup.save(blank, REVIEW, {kind: START.none})));
    await Effect.runPromise(Effect.flip(setup.publish(blank, "Argo")));
    await Effect.runPromise(Effect.flip(setup.apply(DRAFT, REVIEW, FLEET.setup_etag)));
    expect(requests).toBe(0);
    await Effect.runPromise(setup.save(DRAFT, REVIEW, {kind: START.none}));
    setup.clear();
    expect(setup.pending).toBe(false);
    expect((await Effect.runPromise(Effect.flip(setup.progress()))).detail).toContain("No Save");
    expect(requests).toBe(1);
  });
});

test("an uncertain setup action fences every other mutation until its own retry resolves", async () => {
  const paths: string[] = [];
  let failing = true;
  await withApi((request) => {
    paths.push(new URL(request.url).pathname);
    if (failing) return problem(503);
    return Response.json(FLEET);
  }, async (transport) => {
    const setup = new SetupSession(transport);
    await Effect.runPromise(Effect.flip(setup.save(DRAFT, REVIEW, {kind: START.none})));
    await Effect.runPromise(Effect.flip(setup.publish(DRAFT, "Argo")));
    await Effect.runPromise(Effect.flip(setup.apply({...DRAFT, linked_fleet_id: FLEET_ID}, REVIEW, FLEET.setup_etag)));
    expect(paths).toHaveLength(1);
    failing = false;
    expect((await Effect.runPromise(setup.progress())).id).toBe(FLEET_ID);
    expect(paths).toHaveLength(2);
    expect(setup.pending).toBe(false);
  });
});
