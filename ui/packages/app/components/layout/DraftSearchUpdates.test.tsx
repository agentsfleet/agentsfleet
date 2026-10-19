import { useSyncExternalStore } from "react";
import { afterEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { DraftClient } from "@/lib/api/drafts";
import { DraftClientFixture } from "@/tests/helpers/draft-client-fixture";
import { DRAFT, WORKSPACE, response } from "@/tests/helpers/draft-fixtures";
import { DraftSidebarHistory } from "./DraftSidebarHistory";
import { DraftSidebarProvider } from "./draft-sidebar-state";
import { useDraftSidebarBridge } from "./use-draft-sidebar-bridge";

vi.mock("@/lib/auth/client", () => ({useCurrentUser: () => ({userId: "creator"})}));
vi.mock("next/navigation", () => ({useSearchParams: () => new URLSearchParams(), useRouter: () => ({replace: vi.fn()})}));
const fixture = new DraftClientFixture();
const CARD = {...DRAFT, linked_fleet_id: null, updated_at: "2026-10-19T00:00:00Z", expires_at: null,
  candidate_revision: 1, setup_revision: null, publication_revision: null};
function Bridge({client}: {client: DraftClient}) {
  const view = useSyncExternalStore(client.subscribe, client.getSnapshot, client.getSnapshot);
  useDraftSidebarBridge(client, view, WORKSPACE, "creator", false);
  return null;
}
afterEach(() => {cleanup(); fixture.dispose();});

it("keeps matches visible while its owning chat refreshes, including a failed refresh and retry", async () => {
  fixture.mount();
  let revision = 1;
  const refresh = Promise.withResolvers<Response>();
  fixture.override = ({path}) => path.endsWith("/drafts")
    ? revision === 4 ? refresh.promise : response({items: [{...CARD, candidate_revision: revision}], next_cursor: null}) : undefined;
  const client = await fixture.ready(true);
  render(<DraftSidebarProvider><Bridge client={client} /><DraftSidebarHistory workspace={WORKSPACE} onNavigate={() => {}} /></DraftSidebarProvider>);
  revision = 3;
  fireEvent.change(screen.getByRole("textbox", {name: "Search all chats"}), {target: {value: "failures"}});
  expect(await screen.findByText("Setup ready · Proposed v3")).toBeTruthy();
  revision = 4;
  fixture.current = {...DRAFT, etag: '"four"', candidate: DRAFT.candidate ? {...DRAFT.candidate, revision: "4"} : undefined};
  await act(async () => {await client.send("Improve the instructions");});
  expect(screen.getByText("Setup ready · Proposed v3")).toBeTruthy();
  expect(screen.queryByText("Searching chats…")).toBeNull();
  await act(async () => {refresh.resolve(response({}, 503));});
  expect(await screen.findByRole("button", {name: "Retry search"})).toBeTruthy();
  expect(screen.getByText("Setup ready · Proposed v3")).toBeTruthy();
  fixture.override = ({path}) => path.endsWith("/drafts")
    ? response({items: [{...CARD, candidate_revision: revision}], next_cursor: null}) : undefined;
  fireEvent.click(screen.getByRole("button", {name: "Retry search"}));
  expect(await screen.findByText("Setup ready · Proposed v4")).toBeTruthy();
  expect(screen.getByRole("textbox", {name: "Search all chats"})).toHaveProperty("value", "failures");
  expect(screen.queryByText("Setup ready · Proposed v3")).toBeNull();
  expect(fixture.calls.filter(call => call.path.endsWith("/drafts"))).toHaveLength(4);
});

it("finds a newly persisted message even when its chat title and setup revision stay unchanged", async () => {
  fixture.mount();
  let matched = false;
  let reads = 0;
  fixture.override = ({path}) => path.endsWith("/drafts")
    ? response({items: ++reads === 1 || matched ? [CARD] : [], next_cursor: null}) : undefined;
  const client = await fixture.ready(true);
  render(<DraftSidebarProvider><Bridge client={client} /><DraftSidebarHistory workspace={WORKSPACE} onNavigate={() => {}} /></DraftSidebarProvider>);
  fireEvent.change(screen.getByRole("textbox", {name: "Search all chats"}), {target: {value: "new search phrase"}});
  expect(await screen.findByText("No matching chats")).toBeTruthy();
  matched = true;
  fixture.current = {...DRAFT, etag: '"message-added"'};
  await act(async () => {await client.send("new search phrase");});
  expect(await screen.findByRole("link", {name: DRAFT.title})).toBeTruthy();
  expect(screen.queryByText("No matching chats")).toBeNull();
});
