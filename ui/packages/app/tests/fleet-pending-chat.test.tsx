import {afterEach, beforeEach, expect, it, vi} from "vitest";
import {cleanup, fireEvent, render, screen, waitFor} from "@testing-library/react";
import {FleetPendingChat} from "@/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetPendingChat";
import {ChatFixture, CHAT_PROPS, chatRouter} from "./workspace-chat/fixture";
import {DRAFT, FLEET_DETAIL, FLEET_ID, REVIEW, response} from "./helpers/draft-fixtures";
import {FleetDetailSchema} from "@/lib/api/fleet-detail";

vi.mock("next/navigation", () => ({useRouter: () => chatRouter, useSearchParams: () => new URLSearchParams()}));
vi.mock("@/lib/auth/client", () => ({useCurrentUser: () => ({isLoaded: true, isSignedIn: true, userId: "creator"})}));
vi.mock("@/app/(dashboard)/admin/fleet-libraries/actions", () => ({stagePlatformLibraryAction: vi.fn()}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", () => ({
  listFleetMemoriesAction: vi.fn(), setMemoryAccessAction: vi.fn(), forgetFleetMemoryAction: vi.fn(),
}));

const PENDING_FLEET = FleetDetailSchema.parse(FLEET_DETAIL);
let fixture: ChatFixture;
beforeEach(() => {
  fixture = new ChatFixture(); fixture.mount();
  fixture.draft = {...DRAFT, linked_fleet_id: FLEET_ID,
    review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}}};
  const respond = fixture.fetch.getMockImplementation();
  if (!respond) throw new Error("Chat fixture is not mounted");
  fixture.fetch.mockImplementation((input, init) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (path.endsWith(`/fleets/${FLEET_ID}/authoring-context`)) return Promise.resolve(response({fleet: PENDING_FLEET,
      conversations: [], activities: {items: [], next_cursor: null}, next_cursor: null}));
    return path.endsWith(`/fleets/${FLEET_ID}`) && (!init?.method || init.method === "GET")
      ? Promise.resolve(response(PENDING_FLEET)) : respond(input, init);
  });
});
afterEach(() => {cleanup(); vi.unstubAllGlobals();});

it("keeps an installing fleet's questions in setup chat without admitting a run or changing its saved setup", async () => {
  render(<FleetPendingChat {...CHAT_PROPS} fleet={PENDING_FLEET} canGrant />);
  await screen.findByRole("heading", {name: "Setup saved · v1"});
  expect(screen.getAllByRole("heading", {name: "Setup saved · v1"})).toHaveLength(1);
  expect(screen.queryByText("INSTALLING")).toBeNull();
  expect(screen.queryByText("Continue setup in chat")).toBeNull();
  expect(screen.queryByRole("link", {name: "Open fleet"})).toBeNull();
  const composer = await screen.findByPlaceholderText("Ask a question or change the setup…");
  await waitFor(() => expect(composer).toHaveProperty("disabled", false));
  fireEvent.change(composer, {target: {value: "Why does Fly need access?"}});
  fireEvent.click(screen.getByRole("button", {name: "Send message"}));
  await waitFor(() => expect(fixture.writes()).toHaveLength(1));
  expect(fixture.writes()[0]).toMatchObject({path: `/live/v1/workspaces/${CHAT_PROPS.workspace}/drafts`, method: "POST",
    body: {source: {kind: "fleet", fleet_id: FLEET_ID, setup_revision: "1"}, message: "Why does Fly need access?"}});
  expect(fixture.saveAttempts).toBe(0);
  expect(fixture.calls.some(call => call.method === "POST" && call.path.endsWith(`/fleets/${FLEET_ID}/messages`))).toBe(false);
});

it("offers a recoverable account error without opening an execution composer", () => {
  render(<FleetPendingChat {...CHAT_PROPS} account={null} fleet={PENDING_FLEET} canGrant={false} />);
  expect(screen.getByRole("alert").textContent).toContain("The setup conversation could not be loaded.");
  fireEvent.click(screen.getByRole("button", {name: "Try again"}));
  expect(chatRouter.refresh).toHaveBeenCalledOnce();
  expect(screen.queryByRole("textbox")).toBeNull();
  expect(fixture.fetch).not.toHaveBeenCalled();
});
