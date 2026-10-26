import type { DraftView } from "@/lib/api/draft-state";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { DraftClient } from "@/lib/api/drafts";
import { DraftConversation } from "@/components/domain/chat/DraftConversation";
import { FleetSetup } from "@/components/domain/chat/FleetSetup";
import { ChatFixture, CHAT_PROPS } from "./fixture";
import { DRAFT, FLEET, MESSAGE, REVIEW, WORKSPACE } from "@/tests/helpers/draft-fixtures";

const SUMMARY = "Fleet setup";
const DETAILS = "Setup details";
const HEY = "Hey, leave the saved setup as it is";
let client: DraftClient;
beforeEach(async () => {
  const fixture = new ChatFixture(); fixture.mount();
  client = new DraftClient({...CHAT_PROPS, subject: "creator"});
  await client.initialize(); await client.resume(DRAFT.id);
});
afterEach(() => {cleanup(); client.dispose(); vi.unstubAllGlobals();});
function conversation(view: DraftView) {
  return <DraftConversation client={client} view={view}><FleetSetup client={client} view={view} workspace={WORKSPACE} /></DraftConversation>;
}
function before(first: Node, second: Node): boolean {
  return Boolean(first.compareDocumentPosition(second) & Node.DOCUMENT_POSITION_FOLLOWING);
}

it.each(["running", "failed", "completed"] as const)("keeps an existing setup above a later %s turn", (state) => {
  const view: DraftView = {...client.getSnapshot(), messages: [MESSAGE,
    {...MESSAGE, role: "assistant", text: "Original proposal", candidate_id: DRAFT.candidate?.id},
    {...MESSAGE, sequence: "2", state, text: HEY},
    ...(state === "running" ? [] : [{...MESSAGE, sequence: "2", role: "assistant" as const, state, text: "Later result"}])],
  };
  render(conversation(view));
  const summary = screen.getByRole("region", {name: SUMMARY});
  expect(before(screen.getByText("Original proposal"), summary)).toBe(true);
  expect(before(summary, screen.getByText(HEY))).toBe(true);
  if (state === "running") expect(before(summary, screen.getByRole("status", {name: "Working"}))).toBe(true);
});

it("places a source preview before its first pending message", () => {
  render(conversation({...client.getSnapshot(), preview: {definition: REVIEW.effective.definition, instanceName: "repairer", revision: "1"},
    messages: [{...MESSAGE, text: HEY, state: "running"}]}));
  expect(before(screen.getByRole("region", {name: SUMMARY}), screen.getByText(HEY))).toBe(true);
});

it("keeps the latest proposal after the reply that created it", () => {
  render(conversation({...client.getSnapshot(), messages: [MESSAGE,
    {...MESSAGE, role: "assistant", text: "Current proposal", candidate_id: DRAFT.candidate?.id}]}));
  expect(before(screen.getByText("Current proposal"), screen.getByRole("region", {name: SUMMARY}))).toBe(true);
});

it("shows only a heading and short text without an expandable setup report", () => {
  render(<FleetSetup client={client} view={client.getSnapshot()} workspace={WORKSPACE} />);
  expect(screen.getByRole("heading", {name: "Setup ready"})).toBeTruthy();
  expect(screen.getByText(/Keep chatting to refine it/)).toBeTruthy();
  expect(screen.queryByRole("region", {name: "Setup instructions"})).toBeNull();
  expect(screen.queryByRole("button", {name: DETAILS})).toBeNull();
  expect(screen.getByRole("region", {name: SUMMARY}).children).toHaveLength(2);
});

it("does not label an unchanged fleet-source chat as having unsaved changes", () => {
  const draft = {...DRAFT, linked_fleet_id: FLEET.id, review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}}};
  render(<FleetSetup client={client} view={{...client.getSnapshot(), draft}} workspace={WORKSPACE} />);
  expect(screen.getByRole("heading", {name: "Based on v1"})).toBeTruthy();
  expect(screen.queryByText(/Unsaved changes/)).toBeNull();
  expect(screen.queryByText(/New runs use v2/)).toBeNull();
});

it("keeps saved fleet status separate from a resumed draft proposal", () => {
  const draft = {...DRAFT, linked_fleet_id: FLEET.id, candidate: {...DRAFT.candidate!, revision: "2"},
    review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}}};
  render(<FleetSetup client={client} view={{...client.getSnapshot(), draft}} workspace={WORKSPACE} />);
  expect(screen.getByRole("heading", {name: "Based on v1"})).toBeTruthy();
  expect(screen.queryByText(/Unsaved changes/)).toBeNull();
  expect(screen.getByText(/Save changes when you’re ready|Your fleet is ready to use this setup/)).toBeTruthy();
});

it("summarizes a saved fleet without opening its full setup report", () => {
  render(<FleetSetup client={client} view={{...client.getSnapshot(), fleet: {...FLEET, status: "active", setup: {state: "ready", checks: []}}}} workspace={WORKSPACE} />);
  expect(screen.getByRole("heading", {name: "Saved v1"})).toBeTruthy();
  expect(screen.getByText(/Your fleet is ready to use this setup/)).toBeTruthy();
  expect(screen.queryByRole("region", {name: "Saved fleet setup"})).toBeNull();
  expect(screen.getByRole("link", {name: "Open fleet"})).toBeTruthy();
});

it.each(["2", "3"])("shows confirmed saved v%s without labeling the draft as saved or unsaved", (revision) => {
  const snapshot = {...client.getSnapshot(), savedSetupRevision: revision};
  const view = render(<FleetSetup client={client} view={snapshot} workspace={WORKSPACE} />);
  expect(screen.getByRole("heading", {name: `Saved v${revision}`})).toBeTruthy();
  view.rerender(<FleetSetup client={client} view={{...snapshot, draft: {...DRAFT, candidate: {...DRAFT.candidate!, id: FLEET.id}}}} workspace={WORKSPACE} />);
  expect(screen.getByRole("heading", {name: `Saved v${revision}`})).toBeTruthy();
  expect(screen.getByText(/Save changes when you’re ready|Your fleet is ready to use this setup/)).toBeTruthy();
});

it("does not hide an unconfirmed first run behind a saved-success summary", () => {
  render(<FleetSetup client={client} view={{...client.getSnapshot(), fleet: {...FLEET, status: "active", start: {kind: "run_now", state: "pending"}}}} workspace={WORKSPACE} />);
  expect(screen.getByText(/setup needs attention/)).toBeTruthy();
  expect(screen.queryByRole("button", {name: DETAILS})).toBeNull();
  expect(screen.getByRole("button", {name: "Continue setup"})).toBeTruthy();
});
