// @vitest-environment node
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { Message } from "@/lib/api/draft-types";
import { MESSAGE_ROLE, MESSAGE_STATE } from "@/lib/api/draft-types";
import { UNKNOWN_USAGE_REASON } from "@/lib/api/draft-recovery";
import { DraftClientFixture } from "./helpers/draft-client-fixture";
import { DRAFT, FLEET_DETAIL, FLEET_ID, MESSAGE, REVIEW, response } from "./helpers/draft-fixtures";

const fixture = new DraftClientFixture();
const SAVE = "Please save this.";
let messages: Message[];
beforeEach(() => {
  fixture.mount(); messages = [];
  fixture.override = ({path, method, body}) => {
    if (path.includes("/drafts/") && path.endsWith("/messages")) {
      if (method === "GET") return response({items: messages, next_cursor: null});
      const input = body as {message: string; operation_id: string};
      messages = [{...MESSAGE, operation_id: input.operation_id, text: input.message},
        {...MESSAGE, operation_id: input.operation_id, role: MESSAGE_ROLE.assistant, candidate_id: fixture.current.candidate?.id, text: "I kept your guidance."}];
    }
    return undefined;
  };
});
afterEach(() => fixture.dispose());
const saves = () => fixture.mutations().filter(call => call.path.endsWith("/fleets") || call.path.endsWith("/revisions"));

// Value: direct save uses the existing checked, idempotent write after its conversation turn.
it("saves an explicit chat request once without a separate review confirmation", async () => {
  const client = await fixture.ready(true);
  await client.send(SAVE);
  await vi.waitFor(() => expect(client.getSnapshot().savedSetupRevision).toBe("1"));
  expect(saves()).toHaveLength(1);
  expect(saves()[0]?.body).toMatchObject({source: {draft_candidate_id: DRAFT.candidate?.id}, review_digest: REVIEW.review_digest, start: {kind: "none"}});
  await client.retry();
  expect(saves()).toHaveLength(2);
  expect(saves()[0]?.headers.get("Idempotency-Key")).toBe(saves()[1]?.headers.get("Idempotency-Key"));
});

it("applies an explicit save to its existing fleet rather than creating another", async () => {
  fixture.current = {...DRAFT, linked_fleet_id: FLEET_ID,
    review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}}};
  const fallback = fixture.override;
  fixture.override = call => {
    if (call.path.endsWith("/revisions")) {
      fixture.fleet = {...FLEET_DETAIL, setup_revision: "2"};
      return response({fleet_id: FLEET_ID, setup_revision: "2", setup_etag: '"two"', etag: '"two"', schedules: []});
    }
    return fallback(call);
  };
  const client = await fixture.ready(true);
  await client.send("Save changes");
  await vi.waitFor(() => expect(client.getSnapshot().savedSetupRevision).toBe("2"));
  expect(saves()).toHaveLength(1);
  expect(saves()[0]?.path).toContain(`/fleets/${FLEET_ID}/revisions`);
});

it.each(["What happens when I save this?", "Don't save this", "Say 'save this'", "Change the budget", "Save this after I review it"])("keeps %s conversational without saving", async text => {
  const client = await fixture.ready(true);
  await client.send(text);
  expect(saves()).toEqual([]);
  expect(client.getSnapshot().savedSetupRevision).toBeNull();
});

it.each([MESSAGE_STATE.running, MESSAGE_STATE.failed, MESSAGE_STATE.cancelled, MESSAGE_STATE.timedOut])("does not save a %s authoring result", async state => {
  const fallback = fixture.override;
  fixture.override = async call => {
    const result = await fallback(call);
    if (call.method === "POST" && call.path.endsWith("/messages")) messages = messages.map(message => message.role === MESSAGE_ROLE.assistant ? {...message, state} : message);
    return result;
  };
  const client = await fixture.ready(true);
  await client.send(SAVE);
  expect(saves()).toEqual([]);
  expect(client.getSnapshot().savedSetupRevision).toBeNull();
});

it("never executes an old save request when reopening history", async () => {
  messages = [{...MESSAGE, text: SAVE}, {...MESSAGE, role: MESSAGE_ROLE.assistant, text: "I kept your guidance."}];
  const client = await fixture.ready(true);
  await client.retry();
  expect(saves()).toEqual([]);
});

it("waits for the accepted turn to finish and consumes the save request before later reads", async () => {
  const fallback = fixture.override;
  fixture.override = async call => {
    const result = await fallback(call);
    if (call.method === "POST" && call.path.endsWith("/messages")) messages = messages.map(message => message.role === MESSAGE_ROLE.assistant
      ? {...message, state: MESSAGE_STATE.running} : message);
    return result;
  };
  const client = await fixture.ready(true);
  await client.send(SAVE);
  expect(saves()).toEqual([]);
  messages = messages.map(message => ({...message, state: MESSAGE_STATE.completed}));
  await client.retry();
  await vi.waitFor(() => expect(client.getSnapshot().savedSetupRevision).toBe("1"));
  expect(saves()).toHaveLength(1);
  expect(client.getSnapshot().messages).toHaveLength(2);
});

it.each(["usage unknown", "stopped"])("does not save when %s after the request", async reason => {
  const fallback = fixture.override;
  fixture.override = async call => {
    const result = await fallback(call);
    if (call.method === "POST" && call.path.endsWith("/messages")) {
      if (reason === "usage unknown") messages = messages.map(message => message.role === MESSAGE_ROLE.assistant
        ? {...message, failure: {reason: UNKNOWN_USAGE_REASON, error_code: "UZ-AUTHOR-015", recovery_action: "await_accounting"}} : message);
      if (reason === "stopped") fixture.current = {...DRAFT, cancelled_through: "1"};
    }
    return result;
  };
  const client = await fixture.ready(true);
  await client.send(SAVE);
  expect(saves()).toEqual([]);
});

it("saves the model's finalized candidate rather than demanding another save request", async () => {
  const client = await fixture.ready(true);
  const fallback = fixture.override;
  fixture.override = async call => {
    const result = await fallback(call);
    if (call.method === "POST" && call.path.endsWith("/messages")) {
      fixture.current = {...DRAFT, candidate: {...DRAFT.candidate!, id: FLEET_ID}, review: {...REVIEW, review_digest: "final-review"}};
      messages = messages.map(message => message.role === MESSAGE_ROLE.assistant ? {...message, candidate_id: FLEET_ID} : message);
    }
    return result;
  };
  await client.send(SAVE);
  await vi.waitFor(() => expect(client.getSnapshot().savedSetupRevision).toBe("1"));
  expect(saves()).toHaveLength(1);
  expect(saves()[0]?.body).toMatchObject({source: {draft_candidate_id: FLEET_ID}, review_digest: "final-review"});
});

it("can finalize and save when no candidate existed before the save request", async () => {
  fixture.current = {...DRAFT, candidate: undefined, review: undefined};
  const client = await fixture.ready(true);
  const fallback = fixture.override;
  fixture.override = async call => {
    const result = await fallback(call);
    if (call.method === "POST" && call.path.endsWith("/messages")) {
      fixture.current = DRAFT;
      messages = messages.map(message => message.role === MESSAGE_ROLE.assistant ? {...message, candidate_id: DRAFT.candidate?.id} : message);
    }
    return result;
  };
  await client.send(SAVE);
  await vi.waitFor(() => expect(client.getSnapshot().savedSetupRevision).toBe("1"));
  expect(saves()).toHaveLength(1);
});

it("refuses a head changed after the model's finalization", async () => {
  const client = await fixture.ready(true);
  const fallback = fixture.override;
  fixture.override = async call => {
    const result = await fallback(call);
    if (call.method === "POST" && call.path.endsWith("/messages")) fixture.current = {...DRAFT, candidate: {...DRAFT.candidate!, id: FLEET_ID}};
    return result;
  };
  await client.send(SAVE);
  expect(saves()).toEqual([]);
  expect(client.getSnapshot().error).toBe("The setup changed after the model finalized it. Your draft is retained; try saving again.");
});

it("reports a refused save without announcing success or retrying automatically", async () => {
  const fallback = fixture.override;
  fixture.override = call => call.path.endsWith("/fleets") && call.method === "POST" ? response({}, 409) : fallback(call);
  const client = await fixture.ready(true);
  await client.send(SAVE);
  await vi.waitFor(() => expect(client.getSnapshot().error).toBeTruthy());
  expect(saves()).toHaveLength(1);
  expect(client.getSnapshot().savedSetupRevision).toBeNull();
});

// Value: protects=a refused authoring request cannot leave a save armed after recovery; fails_when=clearing a later error revives an earlier Save command;
// why_new=existing refusals happened after authoring completed; seam=none; no_claim=provider or server acceptance (mocked HTTP).
it.each(["empty", "earlier"])("consumes a refused Save request with %s history before another local action", async history => {
  if (history === "earlier") messages = [MESSAGE];
  const client = await fixture.ready(true);
  const fallback = fixture.override;
  fixture.override = call => call.method === "POST" && call.path.endsWith("/messages")
    ? response({}, 400) : fallback(call);
  await expect(client.send(SAVE)).rejects.toMatchObject({name: "SetupUnavailable", status: 400});
  expect(client.getSnapshot()).toMatchObject({pending: false, savedSetupRevision: null});
  await client.selectModel("model:second");
  expect(client.getSnapshot()).toMatchObject({error: null, saveOperation: null, model: "model:second", messages});
  expect(saves()).toEqual([]);
});

// Value: protects=an assistant result cannot authorize saving without its matching user turn; fails_when=missing or different request history is treated as consent;
// why_new=existing results always included their matching user request; seam=none; no_claim=normal server message ordering (partial HTTP history is injected).
it.each(["missing", "different"])("does not save a finalized answer with a %s user request", async request => {
  const client = await fixture.ready(true);
  const fallback = fixture.override;
  fixture.override = async call => {
    const result = await fallback(call);
    if (call.method === "POST" && call.path.endsWith("/messages")) {
      messages = messages.filter(message => message.role === MESSAGE_ROLE.assistant);
      if (request === "different") messages.unshift(MESSAGE);
    }
    return result;
  };
  await client.send(SAVE);
  expect(saves()).toEqual([]);
  expect(client.getSnapshot().savedSetupRevision).toBeNull();
  const answer = messages.find(message => message.role === MESSAGE_ROLE.assistant)!;
  messages = [{...MESSAGE, operation_id: answer.operation_id, text: SAVE}, answer];
  await client.retry();
  if (request === "missing") {
    await vi.waitFor(() => expect(client.getSnapshot().savedSetupRevision).toBe("1"));
    expect(saves()).toHaveLength(1);
  } else {
    expect(client.getSnapshot()).toMatchObject({saveOperation: null, savedSetupRevision: null});
    expect(saves()).toEqual([]);
  }
});

it("keeps saving progress through model finalization and the database acknowledgement", async () => {
  const commit = Promise.withResolvers<Response>();
  const fallback = fixture.override;
  fixture.override = call => call.path.endsWith("/fleets") && call.method === "POST" ? commit.promise : fallback(call);
  const client = await fixture.ready(true);
  await client.send(SAVE);
  await vi.waitFor(() => expect(saves()).toHaveLength(1));
  expect(client.getSnapshot().saveOperation).toBe(messages[0]?.operation_id);
  expect(client.getSnapshot().savedSetupRevision).toBeNull();
  commit.resolve(response(FLEET_DETAIL));
  await vi.waitFor(() => expect(client.getSnapshot().saveOperation).toBeNull());
  expect(client.getSnapshot().savedSetupRevision).toBe("1");
});

it("retains the conversation when finalization produces no complete setup", async () => {
  fixture.current = {...DRAFT, candidate: undefined, review: undefined};
  const client = await fixture.ready(true);
  await client.send(SAVE);
  expect(saves()).toEqual([]);
  expect(client.getSnapshot().messages).toHaveLength(2);
  expect(client.getSnapshot().saveOperation).toBeNull();
  expect(client.getSnapshot().error).toBe("The model could not prepare a complete setup. Your conversation is retained; keep chatting to finish it.");
});

it("retries a lost save acknowledgement with one remote effect and the same key", async () => {
  const effects = new Map<string, unknown>();
  const fallback = fixture.override;
  let loseAcknowledgement = true;
  fixture.override = call => {
    if (call.path.endsWith("/fleets") && call.method === "POST") {
      const key = call.headers.get("Idempotency-Key")!;
      effects.set(key, call.body);
      if (loseAcknowledgement) { loseAcknowledgement = false; throw new TypeError("reply lost after commit"); }
      return response(FLEET_DETAIL);
    }
    return fallback(call);
  };
  const client = await fixture.ready(true);
  await client.send(SAVE);
  await vi.waitFor(() => expect(client.getSnapshot().error).toBeTruthy());
  expect(effects.size).toBe(1);
  expect(client.getSnapshot().savedSetupRevision).toBeNull();
  await client.retry();
  expect(client.getSnapshot().savedSetupRevision).toBe("1");
  expect(client.getSnapshot().error).toBeNull();
  expect(effects.size).toBe(1);
  expect(saves()).toHaveLength(2);
  expect(saves()[1]?.body).toEqual(saves()[0]?.body);
});
