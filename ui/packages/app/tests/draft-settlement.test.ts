// @vitest-environment node
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { chatRecovery, UNKNOWN_USAGE_REASON } from "@/lib/api/draft-recovery";
import { MESSAGE_STATE } from "@/lib/api/draft-types";
import { DraftClientFixture } from "./helpers/draft-client-fixture";
import { FakeEventSource } from "./helpers/fake-event-source";
import { ACCEPTED, DRAFT_ID, MESSAGE, response } from "./helpers/draft-fixtures";

const fixture = new DraftClientFixture();
const SETTLED_REASON = "provider_failed";
const FOLLOW_LIMIT_MS = 330_000;
const POLL_WINDOW_MS = 6_000;
beforeEach(() => { fixture.mount(); vi.useFakeTimers(); vi.spyOn(Math, "random").mockReturnValue(0); });
afterEach(() => { fixture.dispose(); vi.useRealTimers(); });

function messages(unknown: boolean): Response {
  return response({items: [{...MESSAGE, state: MESSAGE_STATE.failed,
    failure: {reason: unknown ? UNKNOWN_USAGE_REASON : SETTLED_REASON,
      error_code: "UZ-AUTHOR-015", recovery_action: "await_accounting"}}], next_cursor: null});
}

it("should unblock Send when accounting settles without a second finish event", async () => {
  let unknown = true;
  fixture.override = ({path, method}) => path.endsWith("/messages") && method === "GET" ? messages(unknown) : undefined;
  const client = await fixture.ready(true);
  const stream = FakeEventSource.instances[0];
  expect(stream).toBeDefined();
  expect(chatRecovery(client.getSnapshot()).blocked).toBe(true);
  stream?.open(); await vi.advanceTimersByTimeAsync(0);
  unknown = false;
  await vi.advanceTimersByTimeAsync(POLL_WINDOW_MS);
  expect(chatRecovery(client.getSnapshot()).blocked).toBe(false);
  expect(client.getSnapshot().messages[0]?.failure?.reason).toBe(SETTLED_REASON);
  expect(stream?.closed).toBe(true);
  const reads = fixture.calls.length;
  await vi.advanceTimersByTimeAsync(POLL_WINDOW_MS);
  expect(fixture.calls).toHaveLength(reads);
  await client.send("Continue setup");
  expect(fixture.mutations().at(-1)?.path).toContain(`/drafts/${DRAFT_ID}/messages`);
});

it("should stop accounting polling without saying failed work is still pending", async () => {
  const deadline = new AbortController();
  const timeout = AbortSignal.timeout.bind(AbortSignal);
  vi.spyOn(AbortSignal, "timeout").mockImplementation((ms) => ms === FOLLOW_LIMIT_MS ? deadline.signal : timeout(ms));
  fixture.override = ({path}) => path.endsWith("/messages") ? messages(true) : undefined;
  const client = await fixture.ready(true);
  FakeEventSource.instances[0]?.open();
  await vi.advanceTimersByTimeAsync(0);
  deadline.abort(); await vi.advanceTimersByTimeAsync(0);
  expect(client.getSnapshot().notice).toBeNull();
  expect(client.getSnapshot().messages[0]?.failure?.reason).toBe(UNKNOWN_USAGE_REASON);
  expect(FakeEventSource.instances[0]?.closed).toBe(true);
  const reads = fixture.calls.length;
  await vi.advanceTimersByTimeAsync(POLL_WINDOW_MS);
  expect(fixture.calls).toHaveLength(reads);
});

it("should release accounting polling when Stop is followed by New Fleet", async () => {
  fixture.override = ({path, method}) => {
    if (path.endsWith("/messages") && method === "GET") return messages(true);
    if (path.endsWith("/cancellations")) {
      fixture.current = {...fixture.current, cancelled_through: MESSAGE.sequence};
      return response({cancelled_through: MESSAGE.sequence, stopped: []});
    }
    return undefined;
  };
  const client = await fixture.ready(true);
  const stream = FakeEventSource.instances[0];
  stream?.open(); await vi.advanceTimersByTimeAsync(0);
  await client.stop();
  expect(chatRecovery(client.getSnapshot()).blocked).toBe(false);
  expect(stream?.closed).toBe(true);
  client.fresh();
  expect(stream?.closed).toBe(true);
  const reads = fixture.calls.length;
  await vi.advanceTimersByTimeAsync(POLL_WINDOW_MS);
  expect(fixture.calls).toHaveLength(reads);
  expect(client.getSnapshot().draft).toBeNull();
});

it("should give a new Send its own watch deadline after Stop", async () => {
  const deadlines: AbortController[] = [];
  const timeout = AbortSignal.timeout.bind(AbortSignal);
  vi.spyOn(AbortSignal, "timeout").mockImplementation((ms) => {
    if (ms !== FOLLOW_LIMIT_MS) return timeout(ms);
    const deadline = new AbortController(); deadlines.push(deadline); return deadline.signal;
  });
  let fresh = false;
  fixture.override = ({path, method}) => {
    if (path.endsWith("/messages")) {
      if (method === "POST") { fresh = true; return response({...ACCEPTED, sequence: "2"}, 202); }
      if (fresh) return response({items: [{...MESSAGE, sequence: "2", state: MESSAGE_STATE.queued}], next_cursor: null});
      return messages(true);
    }
    if (path.endsWith("/cancellations")) {
      fixture.current = {...fixture.current, cancelled_through: MESSAGE.sequence};
      return response({cancelled_through: MESSAGE.sequence, stopped: []});
    }
    return undefined;
  };
  const client = await fixture.ready(true);
  FakeEventSource.instances[0]?.open(); await vi.advanceTimersByTimeAsync(0);
  await client.stop();
  await client.send("Continue setup");
  const replacement = FakeEventSource.instances[1];
  expect(replacement).toBeDefined();
  expect(deadlines).toHaveLength(2);
  deadlines[0]?.abort(); await vi.advanceTimersByTimeAsync(0);
  expect(replacement?.closed).toBe(false);
  expect(client.getSnapshot().notice).toBeNull();
  deadlines[1]?.abort(); await vi.advanceTimersByTimeAsync(0);
  expect(replacement?.closed).toBe(true);
  expect(client.getSnapshot().notice).toBe("Still waiting for a result.");
});

it("should follow concurrent work above Stop's observed cutoff", async () => {
  let concurrent = false;
  fixture.override = ({path, method}) => {
    if (path.endsWith("/messages") && method === "GET") {
      return concurrent ? response({items: [{...MESSAGE, sequence: "2", state: MESSAGE_STATE.queued}], next_cursor: null}) : messages(true);
    }
    if (path.endsWith("/cancellations")) {
      concurrent = true;
      fixture.current = {...fixture.current, cancelled_through: MESSAGE.sequence};
      return response({cancelled_through: MESSAGE.sequence, stopped: []});
    }
    return undefined;
  };
  const client = await fixture.ready(true);
  const stopped = FakeEventSource.instances[0];
  stopped?.open(); await vi.advanceTimersByTimeAsync(0);
  await client.stop();
  expect(fixture.mutations().at(-1)?.body).toEqual({through_sequence: MESSAGE.sequence});
  expect(stopped?.closed).toBe(true);
  expect(FakeEventSource.instances).toHaveLength(2);
  expect(FakeEventSource.instances[1]?.closed).toBe(false);
  expect(chatRecovery(client.getSnapshot())).toMatchObject({pending: true, canStop: true, blocked: false});
});
