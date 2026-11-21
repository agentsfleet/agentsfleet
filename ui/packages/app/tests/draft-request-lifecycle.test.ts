// @vitest-environment node
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { DRAFT_EVENT } from "@/lib/api/draft-events";
import { DraftClientFixture } from "./helpers/draft-client-fixture";
import { FakeEventSource } from "./helpers/fake-event-source";
import { DRAFT_ID, MESSAGE, TURN_ID, response } from "./helpers/draft-fixtures";

const fixture = new DraftClientFixture();
beforeEach(() => fixture.mount());
afterEach(() => fixture.dispose());

it("keeps access loss terminal when a caller tries to reset or resume the conversation", async () => {
  const client = await fixture.ready(true);
  fixture.override = ({method}) => method === "POST" ? response({}, 403) : undefined;
  await expect(client.send("A follow-up")).rejects.toMatchObject({status: 403});
  const requests = fixture.calls.length;
  client.fresh();
  expect(client.getSnapshot().inaccessible).toBe(true);
  await expect(client.resume(DRAFT_ID)).rejects.toMatchObject({status: 403});
  expect(client.getSnapshot()).toMatchObject({inaccessible: true, draft: null, messages: [], busy: false, pending: false});
  expect(fixture.calls).toHaveLength(requests);
});

it("cancels a stalled watcher read before Stop reads and confirms cancellation", async () => {
  let stopped = false;
  let stalled = false;
  const cancelled = vi.fn();
  fixture.override = ({path, method}) => {
    if (method === "PATCH" && path.endsWith(`/drafts/${DRAFT_ID}/turns/${TURN_ID}`)) {
      stopped = true;
      return response({sequence: "1", state: "cancelled"});
    }
    if (path.endsWith("/messages") && method === "GET") {
      if (stalled) return new Response(new ReadableStream({cancel: cancelled}));
      return response({items: [{...MESSAGE, state: stopped ? "cancelled" : "running"}], next_cursor: null});
    }
  };
  const client = await fixture.ready(true);
  stalled = true;
  const source = FakeEventSource.instances[0];
  source?.open();
  await vi.waitFor(() => expect(fixture.calls.filter(call => call.path.endsWith("/messages"))).toHaveLength(2));
  stalled = false;
  const stopping = client.stop();
  void stopping.catch(() => undefined);
  await vi.waitFor(() => expect(cancelled).toHaveBeenCalledOnce());
  await stopping;
  expect(stopped).toBe(true);
  expect(source?.closed).toBe(true);
  expect(client.getSnapshot()).toMatchObject({busy: false, pending: false, messages: [{state: "cancelled"}]});
});

it("restarts stream reconciliation when a Stop request fails", async () => {
  fixture.override = ({path, method}) => path.endsWith("/messages")
    ? response({items: [{...MESSAGE, state: "running"}], next_cursor: null})
    : method === "PATCH" ? response({user_message: "Stop unavailable"}, 503) : undefined;
  const client = await fixture.ready(true);
  const first = FakeEventSource.instances[0];
  await expect(client.stop()).rejects.toMatchObject({status: 503, message: "Stop couldn’t be confirmed. Check the conversation, then try again."});
  expect(first?.closed).toBe(true);
  expect(FakeEventSource.instances).toHaveLength(2);
  fixture.override = ({path}) => path.endsWith("/messages") ? response({items: [MESSAGE], next_cursor: null}) : undefined;
  FakeEventSource.instances[1]?.emitNamed(DRAFT_EVENT.completed, JSON.stringify({draft_id: DRAFT_ID}));
  await vi.waitFor(() => expect(client.getSnapshot().messages[0]?.state).toBe("completed"));
  expect(FakeEventSource.instances[1]?.closed).toBe(true);
});
