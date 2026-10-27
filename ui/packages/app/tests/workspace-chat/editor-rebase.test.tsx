import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { DraftClient } from "@/lib/api/drafts";
import { DefinitionEditor } from "@/components/domain/chat/DefinitionEditor";
import { ChatFixture, CHAT_PROPS } from "./fixture";
import { DRAFT, REVIEW } from "@/tests/helpers/draft-fixtures";

let fixture: ChatFixture;
let client: DraftClient;
const NEWER = "Newer instructions proposed in chat";
beforeEach(async () => {fixture = new ChatFixture(); fixture.mount(); client = new DraftClient({...CHAT_PROPS, subject: "creator"}); await client.initialize(); await client.resume(DRAFT.id);});
afterEach(() => {cleanup(); client.dispose(); vi.unstubAllGlobals();});

async function newer() {
  const definition = {...REVIEW.effective.definition, instructions_md: NEWER};
  fixture.draft = {...DRAFT, candidate: DRAFT.candidate ? {...DRAFT.candidate, revision: "2", definition} : undefined,
    review: {...REVIEW, effective: {...REVIEW.effective, definition}}};
  await client.refreshSetup();
}

it("refreshes untouched instructions when a new chat proposal arrives", async () => {
  const page = render(<DefinitionEditor client={client} view={client.getSnapshot()} onUpdated={vi.fn()} />);
  await newer(); page.rerender(<DefinitionEditor client={client} view={client.getSnapshot()} onUpdated={vi.fn()} />);
  expect(screen.getByLabelText("Instructions")).toHaveProperty("value", NEWER);
  expect(screen.queryByRole("alert")).toBeNull();
  expect(fixture.writes()).toEqual([]);
});

it("preserves manual work and blocks a stale replacement until it is discarded", async () => {
  const page = render(<DefinitionEditor client={client} view={client.getSnapshot()} onUpdated={vi.fn()} />);
  fireEvent.change(screen.getByLabelText("Instructions"), {target: {value: "Keep this local instruction"}});
  await newer(); page.rerender(<DefinitionEditor client={client} view={client.getSnapshot()} onUpdated={vi.fn()} />);
  expect(screen.getByLabelText("Instructions")).toHaveProperty("value", "Keep this local instruction");
  expect(screen.getByRole("alert").textContent).toContain("Your edits are still here");
  expect(screen.getByRole("button", {name: "Save changes"})).toHaveProperty("disabled", true);
  fireEvent.click(screen.getByRole("button", {name: "Discard manual changes and load latest"}));
  expect(screen.getByLabelText("Instructions")).toHaveProperty("value", NEWER);
  expect(screen.queryByRole("alert")).toBeNull();
  expect(fixture.writes()).toEqual([]);
});

it("treats an undone manual change as clean when chat advances", async () => {
  const page = render(<DefinitionEditor client={client} view={client.getSnapshot()} onUpdated={vi.fn()} />);
  fireEvent.change(screen.getByLabelText("Instructions"), {target: {value: "Temporary change"}});
  fireEvent.change(screen.getByLabelText("Instructions"), {target: {value: REVIEW.effective.definition.instructions_md}});
  await newer(); page.rerender(<DefinitionEditor client={client} view={client.getSnapshot()} onUpdated={vi.fn()} />);
  expect(screen.getByLabelText("Instructions")).toHaveProperty("value", NEWER);
  expect(screen.queryByRole("alert")).toBeNull();
});
