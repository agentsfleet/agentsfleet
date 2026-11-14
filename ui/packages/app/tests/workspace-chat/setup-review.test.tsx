import type { DraftView } from "@/lib/api/draft-state";
import { useSyncExternalStore } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { SetupReview } from "@/components/domain/chat/SetupReview";
import { DraftClient } from "@/lib/api/drafts";
import { EMPTY_DRAFT_VIEW } from "@/lib/api/draft-state";
import { DRAFT, FLEET, REVIEW, SCHEDULE, WORKSPACE, response } from "@/tests/helpers/draft-fixtures";

import { DRAFT_STATE, SCHEDULE_RUN_STATE, SCHEDULE_SYNC } from "@/lib/api/draft-vocabulary";

let client: DraftClient;
beforeEach(() => {client = new DraftClient({account: "account", workspace: WORKSPACE, subject: "reviewer"});});
afterEach(() => {cleanup(); client.dispose(); vi.restoreAllMocks();});

it("keeps proposed triggers separate from saved scheduler failures and retries the saved schedule", async () => {
  const schedule = {...SCHEDULE, sync: SCHEDULE_SYNC.failed, last_error: "Scheduler unavailable", run_state: SCHEDULE_RUN_STATE.unavailable, next_run_at: null};
  const request = vi.spyOn(globalThis, "fetch").mockResolvedValue(response({...schedule, sync: SCHEDULE_SYNC.synced}));
  render(<SetupReview client={client} view={{...EMPTY_DRAFT_VIEW, draft: DRAFT, fleet: {...FLEET, schedules: [schedule]}}} />);
  expect(screen.getByText("Proposed setup · not saved")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", {name: "Review setup"}));
  const panel = within(screen.getByRole("dialog", {name: "Fleet setup"}));
  expect(panel.getByText(/Scheduler unavailable/)).toBeTruthy();
  expect(panel.getByText(/Proposed timing changes take effect after saving/)).toBeTruthy();
  fireEvent.click(panel.getByRole("button", {name: "Retry registration"}));
  await waitFor(() => expect(client.getSnapshot().busy).toBe(false));
  expect(request).toHaveBeenCalledWith(`/live/v1/workspaces/${WORKSPACE}/fleets/${schedule.fleet_id}/schedules/${schedule.schedule_id}/sync`, expect.objectContaining({method: "POST"}));
  expect(client.getSnapshot().error).toBeNull();
});

it("shows a saved revision and no saved schedules without inventing a source or access", () => {
  render(<SetupReview client={client} view={{...EMPTY_DRAFT_VIEW, fleet: FLEET}} />);
  expect(screen.getByText("Saved setup · revision 1")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", {name: "Review setup"}));
  expect(screen.getByText("No schedules saved for this fleet.")).toBeTruthy();
  expect(screen.getByText("Source revision unavailable")).toBeTruthy();
  expect(screen.queryByText(/Proposed timing changes/)).toBeNull();
});

it("shows preview identity and unconfirmed scheduling before a fleet is saved", () => {
  render(<SetupReview client={client} view={{...EMPTY_DRAFT_VIEW,
    preview: {definition: REVIEW.effective.definition, instanceName: "Review queue", revision: "1"}}} />);
  expect(screen.getByText("Setup preview")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", {name: "Review setup"}));
  expect(within(screen.getByRole("dialog")).getByText("Review queue", {exact: true})).toBeTruthy();
  expect(screen.getByText("Schedules appear here after you save the fleet.")).toBeTruthy();
});

it("opens an older closed draft against the current saved fleet without mislabelling old configuration", () => {
  const definition = {...REVIEW.effective.definition, description: "Current fleet purpose", instructions_md: "Current instructions",
    config: {"x-agentsfleet": {model: "current-runtime", tools: ["shell"], credentials: ["current-secret"], budget: {daily_nanos: "2000000000"}}}};
  render(<SetupReview client={client} view={{...EMPTY_DRAFT_VIEW, draft: {...DRAFT, state: DRAFT_STATE.closed},
    fleet: {...FLEET, name: "Current fleet", setup_revision: "2", definition}}} />);
  expect(screen.getByText("Saved setup · revision 2")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", {name: "Review setup"}));
  const panel = within(screen.getByRole("dialog"));
  expect(panel.getByText("Current fleet")).toBeTruthy();
  expect(panel.getByText("Current fleet purpose")).toBeTruthy();
  expect(panel.getByText("current-runtime")).toBeTruthy();
  expect(panel.getByText("Revision 2")).toBeTruthy();
  expect(panel.getByText(/current-secret/)).toBeTruthy();
  expect(panel.queryByText(REVIEW.effective.instance_name)).toBeNull();
  expect(panel.queryByRole("list", {name: "Current readiness"})).toBeNull();
});

it("keeps the pinned source revision available before the first Plugin message", () => {
  render(<SetupReview client={client} view={{...EMPTY_DRAFT_VIEW,
    preview: {definition: REVIEW.effective.definition, instanceName: "Pinned setup", revision: "pinned-source-revision"}}} />);
  fireEvent.click(screen.getByRole("button", {name: "Review setup"}));
  expect(screen.getByText("pinned-source-revision")).toBeTruthy();
  expect(screen.queryByText("Source revision unavailable")).toBeNull();
});

it("keeps a failed schedule retry visible inside the open panel", async () => {
  const state = {...EMPTY_DRAFT_VIEW, fleet: {...FLEET, schedules: [{...SCHEDULE, sync: SCHEDULE_SYNC.failed}]}};
  const refused = "Scheduler unavailable. Retry registration.";
  const request = vi.spyOn(globalThis, "fetch").mockResolvedValue(response({user_message: refused}, 503));
  render(<LiveReview view={state} />);
  fireEvent.click(screen.getByRole("button", {name: "Review setup"}));
  fireEvent.click(screen.getByRole("button", {name: "Retry registration"}));
  const panel = within(screen.getByRole("dialog"));
  expect((await panel.findByRole("alert")).textContent).toContain(refused);
  expect(request).toHaveBeenCalledOnce();
  expect(panel.getByRole("button", {name: "Retry registration"})).toBeTruthy();
});

function LiveReview({view}: {view: DraftView}) {
  const current = useSyncExternalStore(client.subscribe, client.getSnapshot, client.getSnapshot);
  return <SetupReview client={client} view={{...view, busy: current.busy, error: current.error}} />;
}

it("does not call a valid saved default model unresolved or reuse a mismatched preview", () => {
  const definition = {...REVIEW.effective.definition, description: "Saved purpose"};
  render(<SetupReview client={client} view={{...EMPTY_DRAFT_VIEW, fleet: {...FLEET, definition, setup_revision: "2"},
    preview: {definition: {...definition, description: "Obsolete preview"}, instanceName: "Obsolete name", revision: "1", setupRevision: "1"}}} />);
  fireEvent.click(screen.getByRole("button", {name: "Review setup"}));
  const panel = within(screen.getByRole("dialog"));
  expect(panel.getByText("Runtime model unavailable in this view")).toBeTruthy();
  expect(panel.queryByText("Account default is not resolved")).toBeNull();
  expect(panel.getByText("Saved purpose")).toBeTruthy();
  expect(screen.queryByText("Obsolete preview")).toBeNull();
  expect(screen.queryByText("Obsolete name")).toBeNull();
});

it("identifies a proposal reviewed against an older saved revision", () => {
  render(<SetupReview client={client} view={{...EMPTY_DRAFT_VIEW,
    draft: {...DRAFT, review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}}},
    fleet: {...FLEET, setup_revision: "2"}}} />);
  fireEvent.click(screen.getByRole("button", {name: "Review setup"}));
  const panel = within(screen.getByRole("dialog"));
  expect(panel.getByText("Revision 2")).toBeTruthy();
  expect(panel.getByText(/reviewed against revision 1/).textContent).toContain("refresh the setup before saving");
});

it("keeps an incomplete first proposal unconfirmed without inventing permissions", () => {
  render(<SetupReview client={client} view={{...EMPTY_DRAFT_VIEW, draft: {...DRAFT, candidate: undefined, review: undefined}}} />);
  fireEvent.click(screen.getByRole("button", {name: "Review setup"}));
  const panel = within(screen.getByRole("dialog"));
  expect(panel.getByText("New fleet")).toBeTruthy();
  expect(panel.getByText(/Access and spending limits will appear after/)).toBeTruthy();
  expect(panel.queryByText(/No tools requested/)).toBeNull();
  expect(panel.getByText("Add instructions before saving or publishing.")).toBeTruthy();
});
