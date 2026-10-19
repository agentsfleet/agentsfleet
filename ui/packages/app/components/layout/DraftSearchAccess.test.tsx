import { useState, useSyncExternalStore } from "react";
import { Button } from "@agentsfleet/design-system";
import { afterEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { DraftClient } from "@/lib/api/drafts";
import { DraftClientFixture } from "@/tests/helpers/draft-client-fixture";
import { DRAFT, WORKSPACE, response } from "@/tests/helpers/draft-fixtures";
import { DraftSidebarHistory } from "./DraftSidebarHistory";
import { useDraftSidebarBridge } from "./use-draft-sidebar-bridge";
import { DraftSearchResults } from "./DraftSearchResults";
import { DraftSidebarProvider, UpdateContext, type DraftSidebarState } from "./draft-sidebar-state";

vi.mock("@/lib/auth/client", () => ({useCurrentUser: () => ({userId: "creator"})}));
vi.mock("next/navigation", () => ({useSearchParams: () => new URLSearchParams(), useRouter: () => ({replace: vi.fn()})}));
const fixture = new DraftClientFixture();
function Bridge({client}: {client: DraftClient}) {
  const view = useSyncExternalStore(client.subscribe, client.getSnapshot, client.getSnapshot);
  useDraftSidebarBridge(client, view, WORKSPACE, "creator", false);
  return <Button disabled={view.inaccessible}>Send again</Button>;
}
afterEach(() => {cleanup(); fixture.dispose();});

it.each([401, 403, 404, 410])("keeps an observed search refusal (%i) in the owning client and disables further work", async status => {
  fixture.mount();
  const card = {...DRAFT, linked_fleet_id: null, updated_at: "2026-10-19T00:00:00Z", expires_at: null,
    candidate_revision: 1, setup_revision: null, publication_revision: null};
  fixture.override = ({path}) => path.endsWith("/drafts") ? response({items: [card], next_cursor: null}) : undefined;
  const client = await fixture.ready(true);
  render(<DraftSidebarProvider><Bridge client={client} /><DraftSidebarHistory workspace={WORKSPACE} onNavigate={() => {}} /></DraftSidebarProvider>);
  expect(screen.getByRole("link", {name: DRAFT.title})).toBeTruthy();
  fixture.override = ({path}) => path.endsWith("/drafts") ? response({}, status) : undefined;
  fireEvent.change(screen.getByRole("textbox", {name: "Search all chats"}), {target: {value: "failures"}});
  await waitFor(() => expect(client.getSnapshot().inaccessible).toBe(true));
  expect(screen.getByRole("button", {name: "Send again"}).hasAttribute("disabled")).toBe(true);
  const count = fixture.calls.length;
  await act(async () => {await expect(client.send("Private message")).rejects.toMatchObject({status: 403});});
  expect(fixture.calls).toHaveLength(count);
  expect(client.getSnapshot()).toMatchObject({cards: [], models: [], draft: null, messages: [], listStatus: "unavailable"});
  expect(screen.queryByRole("link", {name: DRAFT.title})).toBeNull();
});

it("clears retained history after its setup page has unmounted", async () => {
  fixture.mount();
  const client = await fixture.ready(true);
  const shell = (owned: boolean) => <DraftSidebarProvider>{owned ? <Bridge client={client} /> : null}
    <DraftSidebarHistory workspace={WORKSPACE} onNavigate={() => {}} /></DraftSidebarProvider>;
  const view = render(shell(true));
  view.rerender(shell(false));
  fixture.override = ({path}) => path.endsWith("/drafts") ? response({}, 403) : undefined;
  fireEvent.change(screen.getByRole("textbox", {name: "Search all chats"}), {target: {value: "failures"}});
  expect(await screen.findByText("Chat history is unavailable.")).toBeTruthy();
  expect(screen.queryByRole("link", {name: DRAFT.title})).toBeNull();
});

it("does not clear a newer private history when an older search observes lost access", async () => {
  const older: DraftSidebarState = {workspace: WORKSPACE, subject: "creator", cards: [], fresh: null, remove: null,
    owner: null, locked: false, current: null, more: null, status: "ready", searchVersion: null};
  const newer = {...older, workspace: "next-workspace", cards: [{id: DRAFT.id, title: "New workspace chat", detail: "In progress"}]};
  function ChangedScope() {
    const [history, update] = useState<DraftSidebarState | null>(newer);
    return <UpdateContext value={update}><DraftSearchResults history={older} query="failures" onNavigate={() => {}} />
      <output aria-label="Current history">{history?.workspace}: {history?.cards.map(card => card.title).join(", ")}</output></UpdateContext>;
  }
  vi.spyOn(globalThis, "fetch").mockResolvedValue(response({}, 403));
  render(<ChangedScope />);
  expect(await screen.findByRole("alert")).toBeTruthy();
  expect(screen.getByRole("status", {name: "Current history"}).textContent).toBe("next-workspace: New workspace chat");
});
