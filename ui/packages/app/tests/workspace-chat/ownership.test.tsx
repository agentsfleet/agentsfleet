import {afterEach, beforeEach, expect, it, vi} from "vitest";
import {cleanup, fireEvent, render, screen, waitFor, within} from "@testing-library/react";
import {ChatFixture, CHAT_PROPS, chooseSetupAction, openPublication} from "./fixture";
import {WorkspaceChat} from "@/components/domain/chat/WorkspaceChat";
import {stagePlatformLibraryAction} from "@/app/(dashboard)/admin/fleet-libraries/actions";
import {DRAFT, FLEET, FLEET_ID, PUBLICATION, REVIEW, response} from "@/tests/helpers/draft-fixtures";

let fixture: ChatFixture;
beforeEach(() => {fixture = new ChatFixture(); fixture.mount(); vi.mocked(stagePlatformLibraryAction).mockReset();});
afterEach(() => {cleanup(); vi.unstubAllGlobals();});

it("reads no private state until identity is loaded and authenticated", () => {
  fixture.auth.isLoaded = false;
  const view = render(<WorkspaceChat {...CHAT_PROPS} />);
  expect(screen.getByLabelText("Loading your setup chat")).toBeTruthy();
  fixture.auth.isLoaded = true; fixture.auth.isSignedIn = false;
  view.rerender(<WorkspaceChat {...CHAT_PROPS} />);
  expect(screen.getByRole("alert").textContent).toContain("Sign in again");
  fixture.auth.isSignedIn = true; fixture.auth.userId = "";
  view.rerender(<WorkspaceChat {...CHAT_PROPS} />);
  expect(screen.getByRole("alert").textContent).toContain("Sign in again");
  expect(fixture.fetch).not.toHaveBeenCalled();
});

it.each([200, 403, undefined])("retains the platform staging outcome in the owning chat (%s)", async (status) => {
  vi.mocked(stagePlatformLibraryAction).mockResolvedValue(status === 200
    ? {ok: true, data: PUBLICATION} : {ok: false, error: "Staging could not be confirmed", status});
  render(<WorkspaceChat {...CHAT_PROPS} draft={DRAFT.id} canPublishPlatform
    source={{kind: "definition", definition: REVIEW.effective.definition}} />);
  await screen.findByText("Your setup is ready.");
  await openPublication();
  const dialog = screen.getByRole("dialog", {name: "Publish to Library"});
  fireEvent.click(await within(dialog).findByRole("button", {name: "Stage platform entry"}));
  if (status === 200) {
    await screen.findAllByText(/is staged/);
    expect(within(dialog).getByRole("status").textContent).toContain("Publish it explicitly");
  } else if (status === 403) {
    await screen.findAllByText(/doesn’t have access here/);
    expect(screen.getByRole("button", {name: "Reload"})).toBeTruthy();
  } else {
    await screen.findAllByText("Staging could not be confirmed");
    expect(screen.getByRole("button", {name: "Retry"})).toBeTruthy();
  }
});

it("recovers a failed setup continuation from the conversation without reopening its review", async () => {
  fixture.draft = {...DRAFT, state: "saving", linked_fleet_id: FLEET_ID};
  const base = fixture.fetch.getMockImplementation()!;
  let continued = false;
  let attempts = 0;
  fixture.fetch.mockImplementation(async (input, init) => {
    if ((typeof input === "string" ? input : input instanceof URL ? input.href : input.url).endsWith(`/fleets/${FLEET_ID}`)) {
      if (init?.method === "PATCH") {
        attempts += 1;
        if (attempts === 1) return response({user_message: "Setup retry unavailable"}, 503);
        continued = true;
      }
      return response(continued ? {...FLEET, status: "active", setup: {state: "ready", checks: []}} : {...FLEET, setup: {state: "waiting", checks: []}});
    }
    return base(input, init);
  });
  render(<WorkspaceChat {...CHAT_PROPS} draft={DRAFT.id} />);
  fireEvent.click(await screen.findByRole("button", {name: "Finish setup"}));
  await screen.findByText("Setup retry unavailable");
  const retry = screen.getByRole("button", {name: "Finish setup"});
  expect(retry).toHaveProperty("disabled", false);
  expect(continued).toBe(false);
  fireEvent.click(retry);
  await waitFor(() => expect(screen.queryByRole("button", {name: "Finish setup"})).toBeNull());
  expect(screen.getByRole("heading", {name: "Setup saved · v1"})).toBeTruthy();
  expect(continued).toBe(true);
});

it.each(["saved", "updated"])("recovers failed schedule registration from the owning %s conversation", async (source) => {
  fixture.draft = source === "saved" ? {...DRAFT, state: "saving", linked_fleet_id: FLEET_ID}
    : {...DRAFT, linked_fleet_id: FLEET_ID, review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}}};
  const owed = {schedule_id: "sched-1", fleet_id: FLEET_ID, sync: "failed", last_error: null};
  const base = fixture.fetch.getMockImplementation()!;
  let applied = false;
  let synced = false;
  let attempts = 0;
  fixture.fetch.mockImplementation(async (input, init) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (path.endsWith(`/fleets/${FLEET_ID}/revisions`)) {applied = true; return response({fleet_id: FLEET_ID, setup_revision: "2", setup_etag: '"two"', etag: '"two"', schedules: [owed]});}
    if (path.endsWith(`/fleets/${FLEET_ID}`)) return response({...FLEET, setup_revision: applied ? "2" : "1", status: "active", setup: {state: "ready", checks: []}, schedules: [owed]});
    if (path.endsWith(`/fleets/${FLEET_ID}/schedules/sched-1/sync`) && init?.method === "POST") {
      attempts += 1;
      if (attempts === 1) return response({user_message: "Registration retry unavailable"}, 503);
      synced = true;
      return response({...owed, sync: "synced"});
    }
    return base(input, init);
  });
  render(<WorkspaceChat {...CHAT_PROPS} draft={DRAFT.id} />);
  if (source === "updated") await chooseSetupAction("Save changes");
  fireEvent.click(await screen.findByRole("button", {name: "Retry registration"}));
  await screen.findByText("Registration retry unavailable");
  const retry = screen.getByRole("button", {name: "Retry registration"});
  expect(retry).toHaveProperty("disabled", false);
  expect(synced).toBe(false);
  fireEvent.click(retry);
  await waitFor(() => expect(screen.queryByRole("button", {name: "Retry registration"})).toBeNull());
  expect(screen.queryByText("Registration retry unavailable")).toBeNull();
  expect(synced).toBe(true);
});
