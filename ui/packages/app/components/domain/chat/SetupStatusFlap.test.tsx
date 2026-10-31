import type { DraftView } from "@/lib/api/draft-state";
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { EMPTY_DRAFT_VIEW } from "@/lib/api/draft-state";
import { DRAFT, FLEET, MESSAGE, PUBLICATION, WORKSPACE } from "@/tests/helpers/draft-fixtures";
import { SetupStatusFlap } from "./SetupStatusFlap";

afterEach(cleanup);

const SAVED = {...EMPTY_DRAFT_VIEW, fleet: FLEET};
const PUBLISHED = {...DRAFT, publication: {...PUBLICATION, canonical_digest: "candidate-one"}};

it.each([
  EMPTY_DRAFT_VIEW,
  {...SAVED, inaccessible: true},
  {...EMPTY_DRAFT_VIEW, preview: {definition: DRAFT.review!.effective.definition, instanceName: "repairer", revision: "1", setupRevision: "1"}},
  {...EMPTY_DRAFT_VIEW, draft: {...DRAFT, candidate: undefined}},
])("does not invent a saved result for an unavailable or unconfirmed setup %#", view => {
  const {container} = render(<SetupStatusFlap view={view} workspace={WORKSPACE} />);
  expect(container.textContent).toBe("");
});

it.each(["reading", "saving", "publishing"] as const)("shows %s only while that operation is recorded", setupAction => {
  const view = {...SAVED, setupAction, busy: true};
  render(<SetupStatusFlap view={view} workspace={WORKSPACE} />);
  const status = screen.getByRole("status", {name: "Setup status"});
  expect(status.textContent).toBe({reading: "Reading setup", saving: "Saving", publishing: "Publishing"}[setupAction]);
  expect(status.getAttribute("data-setup-status")).toBe("working");
  expect(status.querySelectorAll('svg[aria-hidden="true"]').length).toBeGreaterThan(0);
  expect(screen.queryByRole("link")).toBeNull();
});

it.each([
  {patch: {saveOperation: "accepted-save"}, label: "Saving"},
  {patch: {optimistic: {operation: "pending", text: "Refine", createdAt: MESSAGE.created_at}}, label: "Sending request"},
  {patch: {messages: [{...MESSAGE, state: "queued" as const}]}, label: "Waiting to refine setup"},
  {patch: {messages: [{...MESSAGE, state: "running" as const}]}, label: "Refining setup"},
])("preserves the actual pending state: $label", ({patch, label}) => {
  render(<SetupStatusFlap view={{...EMPTY_DRAFT_VIEW, draft: DRAFT, ...patch}} workspace={WORKSPACE} />);
  expect(screen.getByRole("status").textContent).toBe(label);
  expect(screen.queryByRole("link")).toBeNull();
});

it.each([false, true])("keeps a failed or unconfirmed operation separate from saved success (%s)", pending => {
  render(<SetupStatusFlap view={{...SAVED, error: "A request failed", pending, setupAction: "saving"}} workspace={WORKSPACE} />);
  expect(screen.getByRole("status").textContent).toBe(pending ? "Update not confirmed" : "Setup needs attention");
  expect(screen.getByRole("status").getAttribute("data-setup-status")).toBe("attention");
  expect(screen.queryByRole("link")).toBeNull();
});

it.each([false, true])("marks manual and proposed changes as unsaved (%s)", dirty => {
  render(<SetupStatusFlap view={{...SAVED, draft: dirty ? null : DRAFT}} workspace={WORKSPACE} dirty={dirty} />);
  expect(screen.getByRole("status").textContent).toBe("Unsaved setup changes");
  expect(screen.getByRole("status").getAttribute("data-setup-status")).toBe("attention");
  expect(screen.queryByRole("link")).toBeNull();
});

it("makes an unsaved proposal available for saving without a success tick", () => {
  render(<SetupStatusFlap view={{...EMPTY_DRAFT_VIEW, draft: DRAFT}} workspace={WORKSPACE} />);
  expect(screen.getByRole("status").textContent).toBe("Setup ready to save");
  expect(screen.getByRole("status").getAttribute("data-setup-status")).toBe("attention");
});

it("does not keep refining after the durable stop watermark", () => {
  render(<SetupStatusFlap view={{...EMPTY_DRAFT_VIEW, draft: {...DRAFT, cancelled_through: "1"}, messages: [{...MESSAGE, state: "running"}]}} workspace={WORKSPACE} />);
  expect(screen.getByRole("status").textContent).toBe("Setup ready to save");
});

it("offers Open fleet for an accepted version without claiming activation", () => {
  render(<SetupStatusFlap view={SAVED} workspace={WORKSPACE} />);
  expect(screen.getByRole("status").textContent).toContain("Setup saved · v1");
  expect(screen.getByRole("status").textContent).not.toContain("ready");
  expect(screen.getByRole("link", {name: "Open fleet"}).getAttribute("href")).toBe(`/w/${WORKSPACE}/fleets/${FLEET.id}`);
});

it("opens the refreshed fleet view when setup is already on that page", () => {
  const open = vi.fn();
  render(<SetupStatusFlap view={SAVED} workspace={WORKSPACE} onOpenFleet={open} />);
  fireEvent.click(screen.getByRole("button", {name: "Open fleet"}));
  expect(open).toHaveBeenCalledOnce();
  expect(screen.queryByRole("link", {name: "Open fleet"})).toBeNull();
});

it.each([
  {kind: "setup_ready", revision: "1"},
  {kind: "setup_saved", revision: "2", origin: "cli"},
  {kind: "unsupported", revision: "1"},
  {kind: "setup_saved", revision: "1"},
  {kind: "setup_saved", revision: "1", origin: "cli"},
])("uses only a matching saved receipt for provenance %#", payload => {
  const activities = [{event_id: "receipt", event_type: "activity", status: "recorded", request_json: JSON.stringify(payload), created_at: MESSAGE.created_at}];
  render(<SetupStatusFlap view={{...SAVED, activities}} workspace={WORKSPACE} />);
  const cli = payload.kind === "setup_saved" && payload.revision === "1" && payload.origin === "cli";
  expect(screen.getByRole("status").textContent).toContain(cli ? "Setup saved via CLI · v1" : "Setup saved · v1");
});

it("offers a New fleet pinned to the confirmed publication", () => {
  render(<SetupStatusFlap view={{...EMPTY_DRAFT_VIEW, draft: PUBLISHED}} workspace={WORKSPACE} />);
  expect(screen.getByRole("status").textContent).toContain("Setup published");
  const link = within(screen.getByRole("status")).getByRole("link", {name: "New fleet"});
  expect(link.getAttribute("href")).toBe(`/w/${WORKSPACE}/fleets/new?library=${PUBLICATION.library_id}&revision=${PUBLICATION.revision_id}`);
  expect(screen.queryByRole("link", {name: "Open fleet"})).toBeNull();
});

it("does not call a changed candidate published", () => {
  render(<SetupStatusFlap view={{...EMPTY_DRAFT_VIEW, draft: {...PUBLISHED, candidate: {...DRAFT.candidate!, canonical_digest: "different"}}}} workspace={WORKSPACE} />);
  expect(screen.getByRole("status").textContent).toBe("Setup ready to save");
  expect(screen.queryByRole("link", {name: "New fleet"})).toBeNull();
});

it("retains both actions after saving the same published setup", () => {
  const view: DraftView = {...SAVED, savedSetupRevision: "1", draft: {...PUBLISHED, state: "closed", linked_fleet_id: FLEET.id}};
  render(<SetupStatusFlap view={view} workspace={WORKSPACE} />);
  expect(screen.getByRole("status").textContent).toContain("Setup saved · v1");
  expect(screen.getAllByRole("link")).toHaveLength(2);
});
