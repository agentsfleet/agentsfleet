// @vitest-environment node
import { afterEach, beforeEach, expect, it } from "vitest";
import { DRAFT_STATE } from "@/lib/api/draft-types";
import { earlierConversations } from "@/lib/api/draft-editor";
import { EMPTY_DRAFT_VIEW, projectDraftView, type DraftView } from "@/lib/api/draft-state";
import { DraftClientFixture } from "./helpers/draft-client-fixture";
import { DRAFT, DRAFT_ID, FLEET_DETAIL, FLEET_ID, MESSAGE, REVIEW, response } from "./helpers/draft-fixtures";

const fixture = new DraftClientFixture();
const OLD = "0199a801-1000-7000-8000-00000000000a";
const draft = {...DRAFT, linked_fleet_id: FLEET_ID,
  review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}}};
const conversation = (id = DRAFT_ID, state = DRAFT_STATE.editing as string) => ({draft: {...draft, id, state},
  messages: [MESSAGE], messages_starting_after: null, truncated: false});
const activity = (id: string) => ({event_id: id, event_type: "activity", status: "recorded",
  request_json: JSON.stringify({kind: "setup_saved", revision: "1"}), created_at: MESSAGE.created_at});
const page = () => ({fleet: FLEET_DETAIL, conversations: [conversation()], activities: {items: [activity("saved")], next_cursor: null as string | null}, next_cursor: null as string | null});
beforeEach(() => fixture.mount());
afterEach(() => fixture.dispose());

// Value: one browser read loads setup without walking all old message pages.
it("opens one bounded editor resource and seeds its recent message cursor", async () => {
  const recent = Array.from({length: 25}, (_, index) => ({...MESSAGE, sequence: String(index + 26)}));
  fixture.override = ({path}) => path.endsWith("/authoring-context") ? response({...page(),
    conversations: [{...conversation(), messages: recent, messages_starting_after: "recent-boundary", truncated: true}]})
    : path.endsWith("/messages") ? response({items: recent, next_cursor: null}) : undefined;
  const client = await fixture.ready();
  const before = fixture.calls.length;
  client.fresh({kind: "fleet", fleet_id: FLEET_ID, setup_revision: "1"});
  await client.refreshFleetSetup();
  expect(fixture.calls.slice(before).map(call => call.path)).toEqual([expect.stringContaining("/authoring-context")]);
  expect(client.getSnapshot()).toMatchObject({draft: {id: DRAFT_ID}, messages: recent, truncated: true});
  await client.retry();
  const read = fixture.calls.findLast(call => call.path.includes("/drafts/") && call.path.endsWith("/messages"));
  expect(new URLSearchParams(read?.search).get("starting_after")).toBe("recent-boundary");
  expect(new URLSearchParams(read?.search).get("limit")).toBe("25");
  expect(fixture.mutations()).toEqual([]);
});

it("loads older creator conversations only on demand without replacing the current proposal", async () => {
  fixture.override = ({path, search}) => path.endsWith("/authoring-context") ? response(new URLSearchParams(search).has("starting_after")
    ? {...page(), conversations: [conversation(OLD, DRAFT_STATE.closed)]} : {...page(), next_cursor: "older-chats"}) : undefined;
  const client = await fixture.ready();
  client.fresh({kind: "fleet", fleet_id: FLEET_ID, setup_revision: "1"});
  await client.refreshFleetSetup();
  const current = client.getSnapshot().draft;
  expect(client.getSnapshot().previousDrafts).toEqual([]);
  await client.loadEarlierConversations();
  expect(client.getSnapshot().draft).toBe(current);
  expect(client.getSnapshot().previousDrafts?.map(item => item.id)).toEqual([OLD]);
  expect(client.getSnapshot().conversationCursor).toBeNull();
  expect(new URLSearchParams(fixture.calls.at(-1)?.search).get("starting_after")).toBe("older-chats");
  expect(fixture.mutations()).toEqual([]);
});

it("pages filtered receipts without dropping already visible receipts", async () => {
  fixture.override = ({path}) => path.endsWith("/authoring-context") ? response({...page(), activities: {items: [activity("new")], next_cursor: "older-receipts"}})
    : path.endsWith("/messages") ? response({items: [activity("old")], next_cursor: null}) : undefined;
  const client = await fixture.ready();
  client.fresh({kind: "fleet", fleet_id: FLEET_ID, setup_revision: "1"});
  await client.refreshFleetSetup();
  const first = client.getSnapshot().activities?.[0];
  await client.loadEarlierConversations();
  expect(client.getSnapshot().activities?.map(item => item.event_id)).toEqual(["new", "old"]);
  expect(client.getSnapshot().activities?.[0]).toBe(first);
  expect(new URLSearchParams(fixture.calls.at(-1)?.search).get("event_type")).toBe("activity");
  expect(new URLSearchParams(fixture.calls.at(-1)?.search).get("starting_after")).toBe("older-receipts");
});

it("keeps the current proposal and cursor after an older history read fails", async () => {
  fixture.override = ({path, search}) => path.endsWith("/authoring-context") ? new URLSearchParams(search).has("starting_after")
    ? response({}, 503) : response({...page(), next_cursor: "retry-cursor"}) : undefined;
  const client = await fixture.ready();
  client.fresh({kind: "fleet", fleet_id: FLEET_ID, setup_revision: "1"});
  await client.refreshFleetSetup();
  const current = client.getSnapshot().draft;
  await expect(client.loadEarlierConversations()).rejects.toMatchObject({status: 503});
  expect(client.getSnapshot()).toMatchObject({draft: current, conversationCursor: "retry-cursor", pending: false, busy: false});
});

it("discards an older conversation response after navigation", async () => {
  const client = await fixture.ready();
  let view = {...client.getSnapshot(), fleet: FLEET_DETAIL, conversationCursor: "older"};
  const pending = Promise.withResolvers<Response>();
  fixture.override = () => pending.promise;
  const reading = earlierConversations("/live/v1/workspaces/owner", () => view, new AbortController().signal);
  view = {...view, fleet: {...FLEET_DETAIL, id: OLD}};
  pending.resolve(response({...page(), conversations: [conversation(OLD, DRAFT_STATE.closed)]}));
  expect(await reading).toEqual({});
});

it.each([null, FLEET_DETAIL])("does not request history without a continuation (fleet=%s)", async fleet => {
  const view = {...EMPTY_DRAFT_VIEW, fleet};
  expect(await earlierConversations("/live/owner", () => view, new AbortController().signal)).toEqual({});
  expect(fixture.calls).toEqual([]);
});

it("merges unfinished older chats while retaining the current proposal and previously loaded rows", async () => {
  const earlier = "0199a801-1000-7000-8000-000000000009";
  fixture.override = ({path, search}) => path.endsWith("/authoring-context") ? response(new URLSearchParams(search).has("starting_after")
    ? {...page(), conversations: [conversation(OLD, DRAFT_STATE.editing), conversation(earlier, DRAFT_STATE.closed), conversation(DRAFT_ID)]}
    : {...page(), conversations: [conversation(), conversation(earlier, DRAFT_STATE.closed)], next_cursor: "older"}) : undefined;
  const client = await fixture.ready();
  client.fresh({kind: "fleet", fleet_id: FLEET_ID, setup_revision: "1"});
  await client.refreshFleetSetup();
  const proposal = client.getSnapshot().draft;
  const retained = client.getSnapshot().previousDrafts?.[0];
  await client.loadEarlierConversations();
  expect(client.getSnapshot().draft).toBe(proposal);
  expect(client.getSnapshot().previousDrafts?.map(item => item.id)).toEqual([earlier, OLD]);
  expect(client.getSnapshot().previousDrafts?.[0]).toBe(retained);
});

it("discards a late receipt page after the selected fleet changes", async () => {
  let view: DraftView = {...EMPTY_DRAFT_VIEW, fleet: FLEET_DETAIL, activityCursor: "older"};
  const pending = Promise.withResolvers<Response>();
  fixture.override = () => pending.promise;
  const reading = earlierConversations("/live/owner", () => view, new AbortController().signal);
  view = {...view, fleet: null};
  pending.resolve(response({items: [activity("old")], next_cursor: null}));
  expect(await reading).toEqual({});
});

it("retains receipt identity when a repeated read follows an initially empty fleet snapshot", () => {
  const row = activity("saved");
  const first = projectDraftView({...EMPTY_DRAFT_VIEW, fleet: FLEET_DETAIL}, {activities: [row]});
  const next = projectDraftView(first, {activities: [{...row}]});
  expect(next.activities).toEqual([row]);
  expect(next.activities?.[0]).toBe(row);
});

it("loads an older creator conversation when no prior page is cached", async () => {
  const view = {...EMPTY_DRAFT_VIEW, fleet: FLEET_DETAIL, conversationCursor: "older"};
  fixture.override = () => response({...page(), conversations: [conversation(OLD, DRAFT_STATE.closed)]});
  const patch = await earlierConversations("/live/owner", () => view, new AbortController().signal);
  expect(patch.previousDrafts?.map(item => item.id)).toEqual([OLD]);
  expect(view.fleet).toBe(FLEET_DETAIL);
  expect(patch.conversationCursor).toBeNull();
});
