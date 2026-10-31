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
  return <DraftConversation workspace={CHAT_PROPS.workspace} client={client} view={view}><FleetSetup client={client} view={view} workspace={WORKSPACE} /></DraftConversation>;
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
  if (state === "running") expect(before(summary, screen.getByRole("status", {name: "Refining setup"}))).toBe(true);
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

// Value: protects=setup placement after restored private history; fails_when=a saved setup moves below a newer turn;
// why_new=existing placement tests use the current draft only; seam=none; no_claim=database persistence of private history.
it("anchors saved setup to the last assistant in an older chat and handles history without an assistant", () => {
  const older = {...MESSAGE, text: "Earlier private request"};
  const reply = {...MESSAGE, role: "assistant" as const, text: "Saved proposal from earlier chat"};
  const view: DraftView = {...client.getSnapshot(), fleet: FLEET, messages: [{...MESSAGE, text: HEY}],
    activities: [{event_id: "unrelated", event_type: "activity", status: "recorded", created_at: "1970-01-01T00:00:01Z", request_json: "{"}],
    previousDrafts: [{id: "earlier-chat", cancelledThrough: "0", messages: [older]}]};
  const page = render(conversation(view));
  expect(before(screen.getByRole("region", {name: SUMMARY}), screen.getByText(HEY))).toBe(true);
  page.rerender(conversation({...view, previousDrafts: [{id: "earlier-chat", cancelledThrough: "0", messages: [older, reply]}]}));
  const summary = screen.getByRole("region", {name: SUMMARY});
  expect(before(screen.getByText(reply.text), summary)).toBe(true);
  expect(before(summary, screen.getByText(HEY))).toBe(true);
});

it("keeps an unsaved proposal conversational without a ready or review heading", () => {
  render(<FleetSetup client={client} view={client.getSnapshot()} workspace={WORKSPACE} />);
  expect(screen.queryByRole("heading")).toBeNull();
  expect(screen.getByText(/Keep chatting to refine it/)).toBeTruthy();
  expect(screen.queryByRole("region", {name: "Setup instructions"})).toBeNull();
  expect(screen.queryByRole("button", {name: DETAILS})).toBeNull();
  expect(screen.getByRole("region", {name: SUMMARY}).children).toHaveLength(1);
});

it("does not label an unchanged fleet-source chat as having unsaved changes", () => {
  const draft = {...DRAFT, linked_fleet_id: FLEET.id, review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}}};
  render(<FleetSetup client={client} view={{...client.getSnapshot(), draft}} workspace={WORKSPACE} />);
  expect(screen.getByRole("heading", {name: "Setup saved · v1"})).toBeTruthy();
  expect(screen.queryByText(/Unsaved changes/)).toBeNull();
  expect(screen.queryByText(/New runs use v2/)).toBeNull();
});

it("keeps saved fleet status separate from a resumed draft proposal", () => {
  const draft = {...DRAFT, linked_fleet_id: FLEET.id, candidate: {...DRAFT.candidate!, revision: "2"},
    review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}}};
  render(<FleetSetup client={client} view={{...client.getSnapshot(), draft}} workspace={WORKSPACE} />);
  expect(screen.getByRole("heading", {name: "Setup saved · v1"})).toBeTruthy();
  expect(screen.queryByText(/Unsaved changes/)).toBeNull();
  expect(screen.getByText(/Save changes when you’re ready|You can keep chatting here/)).toBeTruthy();
});

it("summarizes a saved fleet without opening its full setup report", () => {
  render(<FleetSetup client={client} view={{...client.getSnapshot(), fleet: {...FLEET, status: "active", setup: {state: "ready", checks: []}}}} workspace={WORKSPACE} />);
  expect(screen.getByRole("heading", {name: "Setup saved · v1"})).toBeTruthy();
  expect(screen.getByText(/You can keep chatting here|Save changes when you’re ready/)).toBeTruthy();
  expect(screen.queryByRole("region", {name: "Saved fleet setup"})).toBeNull();
  expect(screen.queryByRole("link", {name: "Open fleet"})).toBeNull();
});

it.each(["2", "3"])("shows confirmed saved v%s without labeling the draft as saved or unsaved", (revision) => {
  const snapshot = {...client.getSnapshot(), savedSetupRevision: revision};
  const view = render(<FleetSetup client={client} view={snapshot} workspace={WORKSPACE} />);
  expect(screen.getByRole("heading", {name: `Setup saved · v${revision}`})).toBeTruthy();
  view.rerender(<FleetSetup client={client} view={{...snapshot, draft: {...DRAFT, candidate: {...DRAFT.candidate!, id: FLEET.id}}}} workspace={WORKSPACE} />);
  expect(screen.getByRole("heading", {name: `Setup saved · v${revision}`})).toBeTruthy();
  expect(screen.getByText(/Save changes when you’re ready|You can keep chatting here/)).toBeTruthy();
});

it("does not hide an unconfirmed first run behind a saved-success summary", () => {
  render(<FleetSetup client={client} view={{...client.getSnapshot(), fleet: {...FLEET, status: "active", start: {kind: "run_now", state: "pending"}}}} workspace={WORKSPACE} />);
  expect(screen.getByRole("heading", {name: "Setup saved · v1"})).toBeTruthy();
  expect(screen.queryByRole("button", {name: DETAILS})).toBeNull();
  expect(screen.getByRole("button", {name: "Retry first run"})).toBeTruthy();
});

// Value: protects=one saved acknowledgement per revision; fails_when=unrelated or malformed activity suppresses it or a saved receipt duplicates it;
// why_new=existing anchoring tests supplied custom setup children rather than FleetSetup; seam=none; no_claim=receipt durability.
it("suppresses the fallback acknowledgement only for a recorded setup receipt of the current revision", () => {
  const activity = {event_id: "setup:current", event_type: "activity", status: "recorded", created_at: "1970-01-01T00:00:01Z"};
  const snapshot = {...client.getSnapshot(), draft: null, fleet: {...FLEET, status: "active", setup: {state: "ready", checks: []}}};
  const page = render(<FleetSetup client={client} view={snapshot} workspace={WORKSPACE} />);
  const unrelated = ["{", JSON.stringify({kind: "access_saved", service: "fly"}), JSON.stringify({kind: "setup_saved", revision: "0"})];
  page.rerender(<FleetSetup client={client} view={{...snapshot, activities: unrelated.map((request_json, index) => ({...activity, event_id: String(index), request_json}))}} workspace={WORKSPACE} />);
  expect(screen.getByRole("heading", {name: "Setup saved · v1"})).toBeTruthy();
  for (const kind of ["setup_saved", "setup_ready"]) {
    page.rerender(<FleetSetup client={client} view={{...snapshot, activities: [{...activity, request_json: JSON.stringify({kind, revision: "1"})}]}} workspace={WORKSPACE} />);
    expect(screen.queryByRole("heading", {name: "Setup saved · v1"})).toBeNull();
    expect(screen.getByText("You can keep chatting here to change the setup.")).toBeTruthy();
  }
});
