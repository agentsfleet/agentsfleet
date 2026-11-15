import {cleanup, fireEvent, render, screen, waitFor} from "@testing-library/react";
import {afterEach, beforeEach, expect, it, vi} from "vitest";
import {TooltipProvider} from "@agentsfleet/design-system";
import {DraftClient} from "@/lib/api/drafts";
import {EMPTY_DRAFT_VIEW} from "@/lib/api/draft-state";
import {DRAFT, FLEET, REVIEW, SCHEDULE, WORKSPACE, response} from "@/tests/helpers/draft-fixtures";
import {ChatFixture} from "@/tests/workspace-chat/fixture";
import {SetupWakeups} from "./SetupWakeups";

let client: DraftClient;
let fixture: ChatFixture;
beforeEach(() => {
  fixture = new ChatFixture();fixture.mount();
  client = new DraftClient({account: "account", workspace: WORKSPACE, subject: "creator"});
});
afterEach(() => {cleanup();client.dispose();vi.unstubAllGlobals();});

it("keeps an unsaved schedule above the composer without inventing a next run", () => {
  const definition = {...REVIEW.effective.definition, config: {"x-agentsfleet": {triggers: [
    {type: "webhook", source: "github"}, {type: "cron", schedule: "*/30 * * * *", timezone: "Asia/Kolkata"},
  ]}}};
  render(<SetupWakeups client={client} workspace={WORKSPACE} view={{...EMPTY_DRAFT_VIEW,
    draft: {...DRAFT, candidate: {...DRAFT.candidate!, definition}}}} />);
  expect(screen.getByRole("status").textContent).toBe("Scheduled wake-up · Pending save and registration");
  expect(document.querySelector("time")).toBeNull();
  expect(fixture.calls).toHaveLength(0);
});

it("omits inaccessible, unscheduled and malformed setup schedules", () => {
  const view = render(<SetupWakeups client={client} workspace={WORKSPACE} view={{...EMPTY_DRAFT_VIEW, inaccessible: true}} />);
  expect(screen.queryByRole("status")).toBeNull();
  view.rerender(<SetupWakeups client={client} workspace={WORKSPACE} view={EMPTY_DRAFT_VIEW} />);
  expect(screen.queryByRole("status")).toBeNull();
  view.rerender(<SetupWakeups client={client} workspace={WORKSPACE} view={{...EMPTY_DRAFT_VIEW, draft: DRAFT}} />);
  expect(screen.queryByRole("status")).toBeNull();
});

it.each([false, true])("refreshes a stale wake-up through its native setup owner (refused=%s)", async refused => {
  await client.resume(DRAFT.id);
  const schedule = {...SCHEDULE, next_run_at: "2000-01-01T00:00:00Z"};
  const state = {...client.getSnapshot(), fleet: {...FLEET, schedules: [schedule]}};
  const before = fixture.calls.length;
  if (refused) vi.stubGlobal("fetch", async () => response({user_message: "Setup refresh unavailable"}, 503));
  render(<SetupWakeups client={client} workspace={WORKSPACE} view={state} />, {wrapper: TooltipProvider});
  fireEvent.click(await screen.findByRole("button", {name: "Refresh schedule"}));
  await waitFor(() => expect(client.getSnapshot().busy).toBe(false));
  if (refused) expect(client.getSnapshot().error).toContain("Setup refresh unavailable");
  else {expect(client.getSnapshot().error).toBeNull();expect(fixture.calls.length).toBeGreaterThan(before);}
  expect(screen.getByRole("button", {name: "Refresh schedule"})).toBeTruthy();
});

it("can read recorded schedules while their fleet context is still loading", async () => {
  render(<SetupWakeups client={client} workspace={WORKSPACE} view={{...EMPTY_DRAFT_VIEW, draft: DRAFT, schedules: [SCHEDULE]}} />, {wrapper: TooltipProvider});
  expect(await screen.findByRole("button", {name: "Edit wake-up"})).toBeTruthy();
});
