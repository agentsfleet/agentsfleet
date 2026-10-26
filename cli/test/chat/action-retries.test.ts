import { expect, test } from "bun:test";
import { Effect } from "effect";
import { ChatActions } from "../../src/commands/chat/actions.ts";
import { DraftSession, DRAFT_SOURCE } from "../../src/commands/chat/session.ts";
import { SetupSession } from "../../src/commands/chat/setup-session.ts";
import { outputDouble } from "../helpers-output-double.ts";
import { ACCEPTED, DRAFT, DRAFT_ID, REVIEW, FLEET, FLEET_ID, MODEL, MESSAGE, PRIVATE_TEXT, PUBLICATION, withApi, problem } from "./fixture.ts";

for (const operation of ["send", "save", "publish", "apply"] as const) {
  test(`a lost ${operation} response recovers through chat Retry with the retained request`, async () => {
    const calls: {path: string; key: string | null; body: unknown}[] = [];
    const effects = new Set<string>();
    const draft = {...DRAFT, linked_fleet_id: FLEET_ID,
      review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: FLEET.setup_revision}}};
    await withApi(async request => {
      const path = new URL(request.url).pathname;
      if (request.method === "GET") {
        if (path.endsWith("/messages")) return Response.json({items: [MESSAGE], next_cursor: null});
        return Response.json(path.endsWith(`/${FLEET_ID}`) ? FLEET : draft);
      }
      const body: unknown = await request.json();
      const key = request.headers.get("Idempotency-Key");
      calls.push({path, key, body});
      effects.add(JSON.stringify({path, key, body}));
      if (calls.length === 1) return problem(503);
      return Response.json(operation === "send" ? ACCEPTED : operation === "save" ? FLEET
        : operation === "publish" ? PUBLICATION
        : {fleet_id: FLEET_ID, setup_revision: "2", setup_etag: '"setup-2"', etag: FLEET.etag, schedules: []});
    }, async transport => {
      const actions = new ChatActions(new DraftSession(transport, MODEL, {kind: DRAFT_SOURCE.blank}, DRAFT_ID),
        new SetupSession(transport), outputDouble(), () => Effect.void);
      await Effect.runPromise(actions.review());
      const failed = operation === "send" ? actions.message(PRIVATE_TEXT) : operation === "save" ? actions.save()
        : operation === "publish" ? actions.publish("Support triage") : actions.apply();
      expect((await Effect.runPromise(Effect.flip(failed)))._tag).toBe("ServerError");
      await Effect.runPromise(actions.retry());
      expect(calls).toHaveLength(2);
      expect(calls[1]).toEqual(calls[0]);
      expect(effects.size).toBe(1);
      expect(actions.partial).toBe(false);
    });
  });
}
