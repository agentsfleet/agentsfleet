import {afterEach, beforeEach, expect, it, vi} from "vitest";
import {cleanup, fireEvent, render, screen, waitFor, within} from "@testing-library/react";
import {ChatFixture, CHAT_PROPS} from "./fixture";
import {WorkspaceChat} from "@/components/domain/chat/WorkspaceChat";
import {stagePlatformLibraryAction} from "@/app/(dashboard)/admin/fleet-libraries/actions";
import {DRAFT, FLEET, FLEET_ID, PUBLICATION, REVIEW, response} from "@/tests/helpers/draft-fixtures";

let fixture: ChatFixture;
beforeEach(() => {fixture = new ChatFixture(); fixture.mount(); vi.mocked(stagePlatformLibraryAction).mockReset();});
afterEach(() => {cleanup(); vi.unstubAllGlobals();});

it("reads no private state until identity is loaded and authenticated", () => {
  fixture.auth.isLoaded = false;
  const view = render(<WorkspaceChat {...CHAT_PROPS} />);
  expect(screen.getByLabelText("Loading your private chat")).toBeTruthy();
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
  fireEvent.click(screen.getByRole("button", {name: "Review setup"}));
  const dialog = await screen.findByRole("dialog", {name: "Review fleet setup"});
  fireEvent.click(await within(dialog).findByRole("button", {name: "Stage platform entry"}));
  if (status === 200) {
    await screen.findAllByText(/is staged/);
    fireEvent.click(within(dialog).getByRole("button", {name: "Close"}));
    expect(screen.getByRole("status").textContent).toContain("Publish it explicitly");
  } else if (status === 403) {
    await screen.findAllByText(/doesn’t have access here/);
    expect(screen.getByRole("button", {name: "Reload"})).toBeTruthy();
  } else {
    await screen.findAllByText("Staging could not be confirmed");
    expect(screen.getByRole("button", {name: "Retry"})).toBeTruthy();
  }
});

it("continues a saved fleet from the conversation without reopening its review", async () => {
  fixture.draft = {...DRAFT, state: "saving", linked_fleet_id: FLEET_ID};
  const base = fixture.fetch.getMockImplementation()!;
  let continued = false;
  fixture.fetch.mockImplementation(async (input, init) => {
    if ((typeof input === "string" ? input : input instanceof URL ? input.href : input.url).endsWith(`/fleets/${FLEET_ID}`)) {
      if (init?.method === "PATCH") continued = true;
      return response(continued ? {...FLEET, status: "active", setup: {state: "ready", checks: []}} : FLEET);
    }
    return base(input, init);
  });
  render(<WorkspaceChat {...CHAT_PROPS} draft={DRAFT.id} />);
  fireEvent.click(await screen.findByRole("button", {name: "Continue setup"}));
  await waitFor(() => expect(screen.queryByRole("button", {name: "Continue setup"})).toBeNull());
  expect(screen.getByText("repairer is ready")).toBeTruthy();
  expect(continued).toBe(true);
});

it("retries a saved fleet's unregistered schedule from the conversation", async () => {
  fixture.draft = {...DRAFT, state: "saving", linked_fleet_id: FLEET_ID};
  const owed = {schedule_id: "sched-1", fleet_id: FLEET_ID, sync: "failed", last_error: null};
  const base = fixture.fetch.getMockImplementation()!;
  let synced = false;
  fixture.fetch.mockImplementation(async (input, init) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (path.endsWith(`/fleets/${FLEET_ID}`)) return response({...FLEET, status: "active", setup: {state: "ready", checks: []}, schedules: [owed]});
    if (path.endsWith(`/fleets/${FLEET_ID}/schedules/sched-1/sync`) && init?.method === "POST") { synced = true; return response({...owed, sync: "synced"}); }
    return base(input, init);
  });
  render(<WorkspaceChat {...CHAT_PROPS} draft={DRAFT.id} />);
  fireEvent.click(await screen.findByRole("button", {name: "Retry registration"}));
  await screen.findByText("repairer is ready");
  expect(synced).toBe(true);
});
