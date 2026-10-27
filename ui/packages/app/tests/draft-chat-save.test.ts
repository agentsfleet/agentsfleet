// @vitest-environment node
import { afterEach, beforeEach, expect, it } from "vitest";
import type { Message } from "@/lib/api/draft-types";
import { DRAFT_STATE, MESSAGE_ROLE, MESSAGE_STATE } from "@/lib/api/draft-types";
import { savingOperation } from "@/lib/api/draft-chat-save";
import { EMPTY_DRAFT_VIEW } from "@/lib/api/draft-state";
import { DraftClientFixture } from "./helpers/draft-client-fixture";
import { DRAFT, FLEET_DETAIL, FLEET_ID, MESSAGE, response } from "./helpers/draft-fixtures";

const fixture = new DraftClientFixture();
let messages: Message[];
beforeEach(() => {
  fixture.mount(); messages = [];
  fixture.override = ({path, method, body}) => {
    if (!path.includes("/drafts/") || !path.endsWith("/messages")) return;
    if (method === "GET") return response({items: messages, next_cursor: null});
    const input = body as {message: string; operation_id: string; save: boolean};
    messages = [{...MESSAGE, operation_id: input.operation_id, text: input.message, save_requested: input.save},
      {...MESSAGE, operation_id: input.operation_id, save_requested: input.save, role: MESSAGE_ROLE.assistant,
        state: MESSAGE_STATE.running, text: undefined}];
  };
});
afterEach(() => fixture.dispose());
const saves = () => fixture.mutations().filter(call => call.path.endsWith("/fleets") || call.path.endsWith("/revisions"));
const input = () => fixture.mutations().findLast(call => call.path.endsWith("/messages"))?.body;
function completedSave(revision = "1"): void {
  fixture.current = {...DRAFT, state: DRAFT_STATE.closed, linked_fleet_id: FLEET_ID};
  fixture.fleet = {...FLEET_DETAIL, setup_revision: revision};
  messages = messages.map(message => ({...message, state: MESSAGE_STATE.completed}));
}

// Value: authorization persists with the message, including a compound imperative.
it.each(["Save", "Please save this.", "Save changes", "Save it and run it daily"])("sends durable authorization for %s without a browser save request", async text => {
  const client = await fixture.ready(true);
  await client.send(text);
  expect(input()).toMatchObject({message: text, save: true});
  expect(client.getSnapshot().saveOperation).toBe(messages[0]?.operation_id);
  expect(saves()).toEqual([]);
});

it.each(["What happens when I save this?", "Don't save this", "Say 'save this'", "Change the budget", "Save this after I review it", "Save if it is ready"])("keeps %s conversational", async text => {
  const client = await fixture.ready(true);
  await client.send(text);
  expect(input()).toMatchObject({save: false});
  expect(client.getSnapshot().saveOperation).toBeNull();
  expect(saves()).toEqual([]);
});

it("restores saving progress after the original tab closes, then reads the committed result", async () => {
  const original = await fixture.ready(true);
  await original.send("Save");
  original.dispose();
  const reopened = await fixture.ready(true);
  expect(reopened.getSnapshot().saveOperation).toBe(messages[0]?.operation_id);
  completedSave("2");
  await reopened.retry();
  expect(reopened.getSnapshot()).toMatchObject({savedSetupRevision: "2", saveOperation: null});
  expect(saves()).toEqual([]);
});

it("reads a server save already completed while no tab was open", async () => {
  messages = [{...MESSAGE, save_requested: true}, {...MESSAGE, role: MESSAGE_ROLE.assistant, save_requested: true}];
  completedSave();
  const client = await fixture.ready(true);
  expect(client.getSnapshot()).toMatchObject({savedSetupRevision: "1", saveOperation: null});
  expect(saves()).toEqual([]);
});

it.each([MESSAGE_STATE.failed, MESSAGE_STATE.cancelled, MESSAGE_STATE.timedOut])("ends progress for a %s turn without writing", async state => {
  const client = await fixture.ready(true);
  await client.send("Save");
  messages = messages.map(message => ({...message, state}));
  await client.retry();
  expect(client.getSnapshot().saveOperation).toBeNull();
  expect(saves()).toEqual([]);
});

it("keeps a refused save editable and presents its durable safe error code", async () => {
  const client = await fixture.ready(true);
  await client.send("Save");
  messages = messages.map(message => ({...message, state: MESSAGE_STATE.completed,
    ...(message.role === MESSAGE_ROLE.assistant ? {save_error_code: "AGENTSFLEET_NAME_EXISTS"} : {})}));
  await client.retry();
  expect(client.getSnapshot()).toMatchObject({pending: false, saveOperation: null, savedSetupRevision: null});
  expect(client.getSnapshot().error).toBeTruthy();
  await client.selectModel("model:second");
  expect(client.getSnapshot().model).toBe("model:second");
  expect(saves()).toEqual([]);
});

it("does not leave admission pending when the following history read fails", async () => {
  const client = await fixture.ready(true);
  const fallback = fixture.override;
  fixture.override = call => call.method === "GET" && call.path.endsWith("/messages") ? response({}, 502) : fallback(call);
  await expect(client.send("Save")).rejects.toMatchObject({status: 502});
  expect(client.getSnapshot().pending).toBe(false);
  fixture.override = fallback;
  await client.retry();
  expect(input()).toMatchObject({save: true});
  expect(saves()).toEqual([]);
});

it("clears optimistic saving after a definite admission refusal", async () => {
  const client = await fixture.ready(true);
  const fallback = fixture.override;
  fixture.override = call => call.method === "POST" && call.path.endsWith("/messages") ? response({}, 409) : fallback(call);
  await expect(client.send("Save")).rejects.toMatchObject({status: 409});
  expect(client.getSnapshot()).toMatchObject({pending: false, saveOperation: null, optimistic: null});
  await client.selectModel("model:second");
  expect(client.getSnapshot().model).toBe("model:second");
  expect(saves()).toEqual([]);
});

it("does not authorize a historical save from its text", async () => {
  messages = [{...MESSAGE, text: "Save"}];
  const client = await fixture.ready(true);
  expect(client.getSnapshot().saveOperation).toBeNull();
  expect(saves()).toEqual([]);
});

it("does not restore an old save refusal after a later conversational turn", async () => {
  const old = {...MESSAGE, operation_id: "0199a801-1000-7000-8000-000000000021", save_requested: true};
  messages = [old, {...old, role: MESSAGE_ROLE.assistant, save_error_code: "AGENTSFLEET_NAME_EXISTS"},
    {...MESSAGE, sequence: "2", text: "Use a different name", save_requested: false}];
  const client = await fixture.ready(true);
  expect(client.getSnapshot()).toMatchObject({error: null, pending: false, saveOperation: null});
  expect(saves()).toEqual([]);
});

it("does not restore saving progress below a persisted Stop watermark", async () => {
  fixture.current = {...DRAFT, cancelled_through: "1"};
  messages = [{...MESSAGE, save_requested: true, state: MESSAGE_STATE.running}];
  const client = await fixture.ready(true);
  expect(client.getSnapshot()).toMatchObject({saveOperation: null, pending: false});
  expect(saves()).toEqual([]);
});

it("keeps durable save progress while its draft snapshot is still loading", () => {
  const user = {...MESSAGE, save_requested: true, state: MESSAGE_STATE.running};
  expect(savingOperation({...EMPTY_DRAFT_VIEW, messages: [user]})).toBe(user.operation_id);
});

// Value: protects=manual save recovery after a durable chat-save refusal;
// fails_when=the preparatory read restores an old error after the write succeeds; seam=HTTP.
it("clears the previous chat-save refusal when a manual revision commits", async () => {
  fixture.fleet = FLEET_DETAIL;
  fixture.current = {...DRAFT, linked_fleet_id: FLEET_ID,
    review: {...DRAFT.review!, effective: {...DRAFT.review!.effective, target_setup_revision: "1"}}};
  messages = [{...MESSAGE, save_requested: true}, {...MESSAGE, role: MESSAGE_ROLE.assistant,
    save_requested: true, save_error_code: "AGENTSFLEET_NAME_EXISTS"}];
  const fallback = fixture.override;
  fixture.override = call => {
    if (call.path.endsWith("/revisions")) {
      fixture.fleet = {...FLEET_DETAIL, setup_revision: "2"};
      return response({fleet_id: FLEET_ID, setup_revision: "2", setup_etag: fixture.fleet.setup_etag,
        etag: fixture.fleet.etag, schedules: []}, 201);
    }
    return fallback(call);
  };
  const client = await fixture.ready(true);
  expect(client.getSnapshot().error).toBeTruthy();
  await client.apply();
  expect(client.getSnapshot()).toMatchObject({savedSetupRevision: "2", error: null, pending: false});
  expect(saves()).toHaveLength(1);
});

// Value: protects=historical refusal remains history after the draft was saved;
// fails_when=reopening a committed draft resurrects its old save error; seam=HTTP.
it("does not restore a historical save refusal when reopening a saved draft", async () => {
  fixture.current = {...DRAFT, state: DRAFT_STATE.closed, linked_fleet_id: FLEET_ID};
  messages = [{...MESSAGE, save_requested: true}, {...MESSAGE, role: MESSAGE_ROLE.assistant,
    save_requested: true, save_error_code: "AGENTSFLEET_NAME_EXISTS"}];
  const client = await fixture.ready(true);
  expect(client.getSnapshot()).toMatchObject({savedSetupRevision: "1", error: null, pending: false});
  expect(saves()).toEqual([]);
});
