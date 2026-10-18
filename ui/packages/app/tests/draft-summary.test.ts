import { expect, it } from "vitest";
import { CHAT_TITLE_LIMIT, chatTitle, firstChatRequest, sidebarDrafts } from "@/lib/api/draft-summary";
import { DRAFT, FLEET_ID, MESSAGE, PUBLICATION } from "./helpers/draft-fixtures";

const card = {...DRAFT, linked_fleet_id: null, updated_at: "1970-01-01T00:00:00Z", expires_at: null};

it("keeps an explicit name and gives unnamed chats a distinct readable label", () => {
  expect(chatTitle("Watch builds", DRAFT.id, "Another task")).toBe("Watch builds");
  expect(chatTitle(" New Fleet ", DRAFT.id)).toBe("Setup chat 000002");
  expect(chatTitle("", "other-abc123")).toBe("Setup chat ABC123");
  expect(chatTitle("New fleet", DRAFT.id, "\n Fix  the build \n")).toBe("Fix the build");
  expect(chatTitle("New fleet", DRAFT.id, "🌱".repeat(CHAT_TITLE_LIMIT + 1))).toBe("🌱".repeat(CHAT_TITLE_LIMIT));
});

it("retains list order when the second chat is selected and updates its status in place", () => {
  const rows = sidebarDrafts([{...card, id: "first"}, card], {...DRAFT, title: "New fleet"}, "Fix the build");
  expect(rows.map((row) => row.id)).toEqual(["first", DRAFT.id]);
  expect(rows[1]).toMatchObject({title: "Fix the build", detail: "Setup ready · Setup v1"});
  expect(rows[0]?.detail).toBe("In progress");
});

it("includes a newly created chat once and uses its proposed purpose until a request is available", () => {
  const draft = {...DRAFT, title: "New fleet", publication: {...PUBLICATION, canonical_digest: "one"}};
  const rows = sidebarDrafts([card], draft);
  expect(rows).toHaveLength(1);
  expect(rows[0]).toMatchObject({title: DRAFT.candidate?.definition.description, detail: "Setup ready · Setup v1 · Published"});
  expect(sidebarDrafts([], draft)).toHaveLength(1);
  expect(sidebarDrafts([], null)).toEqual([]);
});

it("distinguishes saved fleets from a closed chat and retains saving state", () => {
  expect(sidebarDrafts([{...card, state: "closed"}], null)[0]?.detail).toBe("Chat closed");
  expect(sidebarDrafts([{...card, state: "closed", linked_fleet_id: FLEET_ID}], null)[0]?.detail).toBe("Fleet saved");
  expect(sidebarDrafts([{...card, state: "saving"}], null)[0]?.detail).toBe("Saving fleet");
  expect(sidebarDrafts([], {...DRAFT, candidate: undefined})[0]?.detail).toBe("In progress");
});

it("uses the first user request and ignores assistant replies or absent text", () => {
  expect(firstChatRequest([{...MESSAGE, role: "assistant"}, {...MESSAGE, text: "First job"}, {...MESSAGE, text: "Second job"}])).toBe("First job");
  expect(firstChatRequest([{...MESSAGE, text: undefined}])).toBe("");
  expect(firstChatRequest([])).toBe("");
});
