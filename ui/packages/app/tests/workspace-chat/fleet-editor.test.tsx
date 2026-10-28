import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { ChatFixture, CHAT_PROPS } from "./fixture";
import { FleetSetupWorkspace, EditSetupButton } from "@/components/domain/chat/FleetSetupWorkspace";
import { FLEET_ID, WORKSPACE } from "@/tests/helpers/draft-fixtures";

let fixture: ChatFixture;
beforeEach(() => {fixture = new ChatFixture(); fixture.mount();});
afterEach(() => {cleanup(); vi.unstubAllGlobals();});

it("opens Edit setup on the full authoring page at the saved revision", () => {
  render(<FleetSetupWorkspace {...CHAT_PROPS} fleet={FLEET_ID} revision="3" viewer="creator"
    header={<EditSetupButton workspace={WORKSPACE} fleet={FLEET_ID} revision="3" />}><p>Fleet conversation</p></FleetSetupWorkspace>);
  expect(screen.getByRole("link", {name: "Edit setup"}).getAttribute("href")).toBe(`/w/${WORKSPACE}/fleets/new?fleet=${FLEET_ID}&setup_revision=3`);
  expect(screen.getByText("Fleet conversation")).toBeTruthy();
  expect(screen.queryByRole("complementary")).toBeNull();
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(fixture.writes()).toEqual([]);
});

it("removes the mounted fleet conversation when the signed-in identity disappears", () => {
  fixture.auth.isLoaded = true; fixture.auth.userId = "";
  render(<FleetSetupWorkspace {...CHAT_PROPS} fleet={FLEET_ID} revision="3" viewer="creator" header={<p>Fleet heading</p>}><p>Fleet conversation</p></FleetSetupWorkspace>);
  expect(screen.queryByText("Fleet conversation")).toBeNull();
});

// Value: protects=server identity during auth hydration; fails_when=a loading identity drops the conversation or enables writes without an account;
// why_new=existing workspace tests begin with loaded identity and account; seam=none; no_claim=identity-provider verification.
it("keeps inline setup navigation during identity hydration and stays inert without an account", () => {
  fixture.auth.isLoaded = false;
  const view = render(<FleetSetupWorkspace {...CHAT_PROPS} account={null} fleet={FLEET_ID} revision="3" viewer="creator"
    header={<p>Fleet heading</p>}><EditSetupButton inline /><p>Fleet conversation</p></FleetSetupWorkspace>);
  expect(screen.getByRole("link", {name: "Edit setup"}).getAttribute("href")).toBe(`/w/${WORKSPACE}/fleets/new?fleet=${FLEET_ID}&setup_revision=3`);
  expect(screen.getByText("Fleet conversation")).toBeTruthy();
  view.rerender(<EditSetupButton inline />);
  expect(screen.getByText("Edit setup")).toBeTruthy();
  expect(screen.queryByRole("link")).toBeNull();
  expect(fixture.calls).toEqual([]);
});
