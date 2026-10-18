// @vitest-environment node
import { afterEach, beforeEach, expect, it } from "vitest";
import type { DraftClient } from "@/lib/api/drafts";
import { DraftClientFixture } from "./helpers/draft-client-fixture";
import { DRAFT, FLEET_ID, MESSAGE, REVIEW, response } from "./helpers/draft-fixtures";

const fixture = new DraftClientFixture();
let client: DraftClient;
const OTHER_DRAFT_ID = "0199a801-1000-7000-8000-00000000000a";
beforeEach(async () => {
  fixture.mount();
  client = fixture.create();
  await client.initialize(); await client.resume(DRAFT.id);
});
afterEach(() => {fixture.dispose();});

it.each(["save", "publish", "apply"] as const)("refuses %s if a fresh read changes the displayed candidate", async (action) => {
  fixture.current = {...DRAFT, candidate: DRAFT.candidate ? {...DRAFT.candidate, id: OTHER_DRAFT_ID} : undefined};
  const pending = action === "save" ? client.save() : action === "apply" ? client.apply() : client.publish("Recipe", false);
  await expect(pending).rejects.toMatchObject({status: 409, message: expect.stringContaining("setup changed")});
  expect(fixture.mutations()).toHaveLength(0);
  expect(client.getSnapshot().draft?.candidate?.id).toBe(fixture.current.candidate?.id);
});

it.each(["candidate", "review"] as const)("refuses Save if the latest %s disappears", async (field) => {
  fixture.current = {...DRAFT, [field]: undefined};
  await expect(client.save()).rejects.toMatchObject({status: 409});
  expect(fixture.mutations()).toHaveLength(0);
});

it("refuses to save while a newly observed authoring request is running", async () => {
  fixture.override = ({path}) => path.endsWith("/messages") ? response({items: [{...MESSAGE, state: "running"}], next_cursor: null}) : undefined;
  await expect(client.save()).rejects.toMatchObject({status: 409});
  expect(fixture.mutations()).toHaveLength(0);
});

it("refuses an improvement when the saved fleet has advanced", async () => {
  fixture.current = {...DRAFT, linked_fleet_id: FLEET_ID, review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "2"}}};
  await client.resume(DRAFT.id);
  await expect(client.apply()).rejects.toMatchObject({status: 409, message: expect.stringContaining("fleet changed")});
  expect(fixture.mutations()).toHaveLength(0);
});

it("retains a chat when deletion is refused and deletes only the requested chat", async () => {
  const other = OTHER_DRAFT_ID;
  fixture.override = ({method}) => method === "DELETE" ? response({user_message: "Chat is busy"}, 409) : undefined;
  await expect(client.remove(other)).rejects.toMatchObject({status: 409});
  expect(client.getSnapshot().draft?.id).toBe(DRAFT.id);
  fixture.override = ({method}) => method === "DELETE" ? response(null, 204) : undefined;
  await client.remove(other);
  expect(client.getSnapshot().draft?.id).toBe(DRAFT.id);
  expect(fixture.mutations().at(-1)?.path).toContain(`/drafts/${other}`);
});

it("does not require a separate review action before a direct Save", async () => {
  await client.save();
  expect(fixture.mutations()).toHaveLength(1);
  expect(fixture.mutations()[0]?.body).toMatchObject({review_digest: REVIEW.review_digest});
});
