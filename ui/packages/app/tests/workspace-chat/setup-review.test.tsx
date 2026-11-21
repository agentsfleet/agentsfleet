import type { DraftView } from "@/lib/api/draft-state";
import { useSyncExternalStore } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { SetupReview } from "@/components/domain/setup/summary/SetupReview";
import { DraftClient } from "@/lib/api/drafts";
import { EMPTY_DRAFT_VIEW } from "@/lib/api/draft-state";
import { DRAFT, FLEET, MODEL, REVIEW, SCHEDULE, WORKSPACE, response } from "@/tests/helpers/draft-fixtures";
import { DRAFT_STATE, SCHEDULE_RUN_STATE, SCHEDULE_SYNC } from "@/lib/api/draft-vocabulary";

vi.mock("@/lib/plugin-ui", () => ({pluginsUiEnabled: () => true}));
let client: DraftClient;
beforeEach(() => {client = new DraftClient({account: "account", workspace: WORKSPACE, subject: "reviewer"});});
afterEach(() => {cleanup(); client.dispose(); vi.restoreAllMocks();});

it("shows failed registration and its native retry without duplicating next wake-up", async () => {
  const schedule = {...SCHEDULE, sync: SCHEDULE_SYNC.failed, last_error: "Scheduler unavailable", run_state: SCHEDULE_RUN_STATE.unavailable, next_run_at: null};
  const request = vi.spyOn(globalThis, "fetch").mockResolvedValue(response({...schedule, sync: SCHEDULE_SYNC.synced}));
  render(<SetupReview workspace={WORKSPACE} client={client} view={{...EMPTY_DRAFT_VIEW, draft: DRAFT, fleet: {...FLEET, schedules: [schedule]}}} />);
  expect(screen.getByText(/Scheduler unavailable/)).toBeTruthy();
  expect(screen.queryByRole("button", {name: "Review setup"})).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: "Retry registration"}));
  await waitFor(() => expect(client.getSnapshot().busy).toBe(false));
  expect(request).toHaveBeenCalledWith(`/live/v1/workspaces/${WORKSPACE}/fleets/${schedule.fleet_id}/schedules/${schedule.schedule_id}/sync`, expect.objectContaining({method: "POST"}));
  expect(client.getSnapshot().error).toBeNull();
});

it("does not invent a source or schedule for an unscheduled saved fleet", () => {
  render(<SetupReview workspace={WORKSPACE} client={client} view={{...EMPTY_DRAFT_VIEW, fleet: FLEET}} />);
  expect(screen.queryByText("Next wake-up")).toBeNull();
  expect(screen.queryByText(/^From /)).toBeNull();
});

it("shows pinned source provenance before the first Plugin message", () => {
  render(<SetupReview workspace={WORKSPACE} client={client} view={{...EMPTY_DRAFT_VIEW,
    preview: {definition: REVIEW.effective.definition, instanceName: "Pinned setup", revision: "pinned-source-revision", sourceLabel: "Merge watch · v3"}}} />);
  expect(screen.getByText("From Merge watch · v3")).toBeTruthy();
  expect(screen.queryByRole("dialog")).toBeNull();
});

it("reads current fleet configuration instead of the older closed draft or stale preview", () => {
  const definition = {...REVIEW.effective.definition, config: {"x-agentsfleet": {
    model: "current-runtime", tools: ["shell"], credentials: ["current-secret"], budget: {daily_nanos: "2000000000"}}}};
  render(<SetupReview workspace={WORKSPACE} client={client} view={{...EMPTY_DRAFT_VIEW, draft: {...DRAFT, state: DRAFT_STATE.closed},
    fleet: {...FLEET, name: "Current fleet", setup_revision: "2", definition},
    preview: {definition: REVIEW.effective.definition, instanceName: "Obsolete preview", revision: "1", setupRevision: "1", sourceLabel: "Old plugin · v1"}}} />);
  expect(screen.getByText("current-runtime")).toBeTruthy();
  expect(screen.getByText("current-secret")).toBeTruthy();
  expect(screen.getByText("$2.00 / day")).toBeTruthy();
  expect(screen.queryByText("From Old plugin · v1")).toBeNull();
});

it("retains the retry after a scheduler refusal without claiming registration succeeded", async () => {
  const state = {...EMPTY_DRAFT_VIEW, fleet: {...FLEET, schedules: [{...SCHEDULE, sync: SCHEDULE_SYNC.failed}]}};
  const refused = "Scheduler unavailable. Retry registration.";
  const request = vi.spyOn(globalThis, "fetch").mockResolvedValue(response({user_message: refused}, 503));
  render(<LiveReview view={state} />);
  fireEvent.click(screen.getByRole("button", {name: "Retry registration"}));
  await waitFor(() => expect(client.getSnapshot().error).toBe(refused));
  expect(screen.getByRole("button", {name: "Retry registration"})).toBeTruthy();
  expect(request).toHaveBeenCalledOnce();
});

function LiveReview({view}: {view: DraftView}) {
  const current = useSyncExternalStore(client.subscribe, client.getSnapshot, client.getSnapshot);
  return <SetupReview workspace={WORKSPACE} client={client} view={{...view, busy: current.busy, error: current.error}} />;
}

it("keeps an incomplete first proposal empty without granting permissions", () => {
  render(<SetupReview workspace={WORKSPACE} client={client} view={{...EMPTY_DRAFT_VIEW, draft: {...DRAFT, candidate: undefined, review: undefined}}} />);
  expect(screen.getByText("Your setup will appear here as you chat.")).toBeTruthy();
  expect(screen.queryByRole("switch")).toBeNull();
  expect(screen.queryByRole("button", {name: /Manage/})).toBeNull();
});

it("names the platform default in a saved setup that omitted an explicit runtime model", () => {
  render(<SetupReview workspace={WORKSPACE} client={client} view={{...EMPTY_DRAFT_VIEW,
    fleet: {...FLEET, definition: REVIEW.effective.definition},
    models: [{...MODEL, ref: "platform-default", label: "MiniMax M3.1 Flash"}],
  }} />);
  expect(screen.getByText("MiniMax M3.1 Flash")).toBeTruthy();
});
