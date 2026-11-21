// @vitest-environment node
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { chatRecovery, UNKNOWN_USAGE_REASON } from "@/lib/api/draft-recovery";
import { DRAFT_STATE, MESSAGE_STATE } from "@/lib/api/draft-types";
import { DraftClientFixture } from "./helpers/draft-client-fixture";
import { FakeEventSource } from "./helpers/fake-event-source";
import { ACCEPTED, DRAFT_ID, MESSAGE, TURN_ID, WORKSPACE, response } from "./helpers/draft-fixtures";

const fixture = new DraftClientFixture();
// The follow used to end itself at 330 seconds; it now ends when the turn settles.
const OLD_FOLLOW_CUT_MS = 330_000;
const POLL_WINDOW_MS = 6_000;
beforeEach(() => { fixture.mount(); vi.useFakeTimers(); vi.spyOn(Math, "random").mockReturnValue(0); });
afterEach(() => { fixture.dispose(); vi.useRealTimers(); });

function unknownUsage(): Response {
  return response({items: [{...MESSAGE, state: MESSAGE_STATE.failed,
    failure: {reason: UNKNOWN_USAGE_REASON, error_code: "UZ-SETUP-015", recovery_action: "retry_message"}}],
  next_cursor: null});
}

function queuedWork(state: string = MESSAGE_STATE.queued): Response {
  return response({items: [{...MESSAGE, state}], next_cursor: null});
}

const STOPPED = {sequence: MESSAGE.sequence, state: MESSAGE_STATE.cancelled};

it("should leave Send open and watch nothing after a failure whose usage is unaccounted", async () => {
  fixture.override = ({path, method}) => path.endsWith("/messages") && method === "GET" ? unknownUsage() : undefined;
  const client = await fixture.ready(true);
  expect(chatRecovery(client.getSnapshot())).toEqual({pending: false, canStop: false});
  expect(FakeEventSource.instances).toHaveLength(0);
  const reads = fixture.calls.length;
  await vi.advanceTimersByTimeAsync(POLL_WINDOW_MS);
  expect(fixture.calls, "nothing polls for accounting").toHaveLength(reads);
  await client.send("Continue setup");
  expect(fixture.mutations().at(-1)?.path).toContain(`/drafts/${DRAFT_ID}/messages`);
});

it("should keep a new Send's watch open past the old 330-second cut after Stop", async () => {
  let fresh = false;
  let stopped = false;
  fixture.override = ({path, method}) => {
    if (path.endsWith("/messages")) {
      if (method === "POST") { fresh = true; return response({...ACCEPTED, sequence: "2"}, 202); }
      if (fresh) return response({items: [{...MESSAGE, sequence: "2", state: MESSAGE_STATE.queued}], next_cursor: null});
      return queuedWork(stopped ? MESSAGE_STATE.cancelled : MESSAGE_STATE.queued);
    }
    if (method === "PATCH") { stopped = true; return response(STOPPED); }
    return undefined;
  };
  const client = await fixture.ready(true);
  FakeEventSource.instances[0]?.open(); await vi.advanceTimersByTimeAsync(0);
  await client.stop();
  await client.send("Continue setup");
  expect(FakeEventSource.instances[1]).toBeDefined();
  await vi.advanceTimersByTimeAsync(OLD_FOLLOW_CUT_MS + 1);
  // An unopened source is renewed by the connection; a watch that had ended
  // would open no further source, so a later one proves it is still following.
  const sources = FakeEventSource.instances.length;
  await vi.advanceTimersByTimeAsync(OLD_FOLLOW_CUT_MS);
  expect(FakeEventSource.instances.length).toBeGreaterThan(sources);
  expect(client.getSnapshot().notice).toBeNull();
});

it("should follow concurrent work Stop did not see", async () => {
  let concurrent = false;
  fixture.override = ({path, method}) => {
    if (path.endsWith("/messages") && method === "GET") {
      return concurrent ? response({items: [{...MESSAGE, sequence: "2", state: MESSAGE_STATE.queued}], next_cursor: null}) : queuedWork();
    }
    if (method === "PATCH") { concurrent = true; return response(STOPPED); }
    return undefined;
  };
  const client = await fixture.ready(true);
  const stopped = FakeEventSource.instances[0];
  stopped?.open(); await vi.advanceTimersByTimeAsync(0);
  await client.stop();
  expect(fixture.mutations().at(-1)).toMatchObject({path: `/live/v1/workspaces/${WORKSPACE}/drafts/${DRAFT_ID}/turns/${TURN_ID}`, body: {status: "cancelled"}});
  expect(stopped?.closed).toBe(true);
  expect(FakeEventSource.instances).toHaveLength(2);
  expect(FakeEventSource.instances[1]?.closed).toBe(false);
  expect(chatRecovery(client.getSnapshot())).toEqual({pending: true, canStop: true});
});

it("should end the watch with a waiting notice when the draft leaves editing while its turn still runs", async () => {
  fixture.override = ({path, method}) => path.endsWith("/messages") && method === "GET" ? queuedWork() : undefined;
  const client = await fixture.ready(true);
  expect(chatRecovery(client.getSnapshot())).toEqual({pending: true, canStop: true});
  fixture.current = {...fixture.current, state: DRAFT_STATE.closed};
  FakeEventSource.instances[0]?.open(); await vi.advanceTimersByTimeAsync(0);
  expect(chatRecovery(client.getSnapshot())).toEqual({pending: true, canStop: false});
  expect(FakeEventSource.instances[0]?.closed).toBe(true);
  expect(client.getSnapshot().notice).toBe("Still waiting for a result.");
});
