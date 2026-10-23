// @vitest-environment node
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { z } from "zod";
import { DRAFT_DELTA_EVENT, DRAFT_RESYNC_EVENT } from "@/lib/api/draft-events";
import { DraftClientFixture } from "./helpers/draft-client-fixture";
import { FakeEventSource } from "./helpers/fake-event-source";
import { DRAFT, MESSAGE, response } from "./helpers/draft-fixtures";
import { DraftLists } from "@/lib/api/draft-lists";

const fixture = new DraftClientFixture();
const STREAM_ID = "0199a801-1000-7000-8000-000000000011";
beforeEach(() => fixture.mount());
afterEach(() => fixture.dispose());

it("renders a private delta immediately and retires it only when a durable final arrives", async () => {
  let completed = false;
  fixture.override = ({path}) => path.endsWith("/messages") ? response({items: [
    {...MESSAGE, role: "assistant", state: completed ? "completed" : "running", text: completed ? "Final answer" : undefined},
  ], next_cursor: null}) : undefined;
  const client = await fixture.ready(true);
  const source = FakeEventSource.instances.at(-1);
  if (!source) throw new Error("Expected private stream");
  source.emitNamed(DRAFT_DELTA_EVENT, JSON.stringify({draft_id: DRAFT.id, sequence: MESSAGE.sequence,
    stream_id: STREAM_ID, stream_seq: 1, stream_start: true, text_kind: "answer", text: "Live answer"}));
  expect(client.getSnapshot().liveReply).toMatchObject({text: "Live answer", gapped: false});
  source.emitNamed(DRAFT_RESYNC_EVENT, "{}");
  expect(client.getSnapshot().liveReply?.gapped).toBe(true);
  completed = true;
  source.emitNamed("message.completed", JSON.stringify({draft_id: DRAFT.id}));
  await vi.waitFor(() => expect(client.getSnapshot().messages[0]?.text).toBe("Final answer"));
  expect(client.getSnapshot().liveReply).toBeNull();
  expect(source.closed).toBe(true);
});

it("replaces an optimistic sent message with the durable row bearing its operation identity", async () => {
  let operation = "";
  fixture.override = ({path, method, body}) => {
    if (path.endsWith("/drafts") && method === "POST") operation = z.object({operation_id: z.string()}).parse(body).operation_id;
    if (path.endsWith("/messages")) return response({items: [{...MESSAGE, operation_id: operation}], next_cursor: null});
    return undefined;
  };
  const client = await fixture.ready();
  await client.send("One durable user row");
  expect(client.getSnapshot().optimistic).toBeNull();
  expect(client.getSnapshot().messages).toHaveLength(1);
  expect(client.getSnapshot().messages[0]?.operation_id).toBe(operation);
});

it.each([403, 503])("isolates a background summary failure from completed send bookkeeping (%i)", async (status) => {
  const client = await fixture.ready();
  fixture.override = ({path, method}) => path.endsWith("/drafts") && method === "GET" ? response({}, status) : undefined;
  const sent = client.send("Create a private fleet setup");
  if (status === 403) await expect(sent).rejects.toMatchObject({name: "AbortError"}); else await sent;
  await vi.waitFor(() => expect(client.getSnapshot().listStatus).toBe("unavailable"));
  expect(client.getSnapshot().inaccessible).toBe(status === 403);
  expect(client.getSnapshot().draft).toEqual(status === 403 ? null : DRAFT);
  if (status === 403) expect(client.getSnapshot().error).toContain("Your account doesn’t have access here.");
});

it.each([false, true])("does not restore private lists after a pending summary loses its owner (failed=%s)", async (failed) => {
  const client = await fixture.ready();
  const pending = Promise.withResolvers<Response>();
  let requested = false;
  fixture.override = ({path, method}) => {
    if (path.endsWith("/drafts") && method === "GET") {requested = true; return pending.promise;}
    return undefined;
  };
  await client.send("Private pending summary");
  expect(requested).toBe(true);
  client.dispose();
  if (failed) pending.reject(new TypeError("late network failure")); else pending.resolve(response({items: [], next_cursor: null}));
  await new Promise<void>(resolve => setImmediate(resolve));
  expect(client.getSnapshot().cards).toEqual([]);
  expect(client.getSnapshot().draft).toBeNull();
});

it("ignores a resolved summary if the selected chat changed after that request completed", async () => {
  const client = await fixture.ready();
  const pending = Promise.withResolvers<Awaited<ReturnType<DraftLists["refreshDraft"]>>>();
  const refresh = vi.spyOn(DraftLists.prototype, "refreshDraft").mockReturnValueOnce(pending.promise);
  await client.send("Old selected chat");
  expect(refresh).toHaveBeenCalledOnce();
  client.fresh();
  pending.resolve({models: [], cards: [], moreModels: true, moreChats: true});
  await new Promise<void>(resolve => setImmediate(resolve));
  expect(client.getSnapshot()).toMatchObject({draft: null, moreModels: false, moreChats: false});
});

it("ignores an empty cached snapshot while loading an authorized draft", async () => {
  const client = await fixture.ready();
  await client.resume(DRAFT.id, {draft: null, messages: [], model: "foreign-model", truncated: true});
  expect(client.getSnapshot()).toMatchObject({draft: DRAFT, truncated: false});
  expect(client.getSnapshot().model).toBe(DRAFT.authoring_model_ref);
});

it("shows cached history immediately while its authoritative draft read is pending", async () => {
  const client = await fixture.ready();
  const pending = Promise.withResolvers<Response>();
  let hold = true;
  fixture.override = ({path}) => {
    if (hold && path.endsWith(`/drafts/${DRAFT.id}`)) {hold = false; return pending.promise;}
    return undefined;
  };
  const loading = client.resume(DRAFT.id, {draft: DRAFT, messages: [{...MESSAGE, text: "Cached history"}],
    model: DRAFT.authoring_model_ref, truncated: false});
  expect(client.getSnapshot().messages[0]?.text).toBe("Cached history");
  pending.resolve(response(DRAFT));
  await loading;
  expect(client.getSnapshot().messages).toEqual([MESSAGE]);
});
