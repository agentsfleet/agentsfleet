import {useSyncExternalStore} from "react";
import {act, cleanup, fireEvent, render, screen, waitFor} from "@testing-library/react";
import {afterEach, beforeEach, expect, it, vi} from "vitest";
import type {DraftView} from "@/lib/api/draft-state";
import {DraftClient} from "@/lib/api/drafts";
import {ChatFixture, CHAT_PROPS} from "@/tests/workspace-chat/fixture";
import {DRAFT, FLEET, FLEET_ID, REVIEW, response} from "@/tests/helpers/draft-fixtures";
import {SetupSaveFlap} from "./SetupSaveFlap";

const feature = vi.hoisted(() => ({enabled: false}));
vi.mock("@/lib/plugin-ui", () => ({pluginsUiEnabled: () => feature.enabled}));
let client: DraftClient;
let fixture: ChatFixture;
beforeEach(async () => {
  feature.enabled = false;fixture = new ChatFixture();fixture.mount();
  client = new DraftClient({...CHAT_PROPS, subject: "creator"});
  await client.initialize();await client.resume(DRAFT.id);
});
afterEach(() => {cleanup();client.dispose();vi.useRealTimers();vi.unstubAllGlobals();});
function Flap({dirty = false, patch = {}}: {dirty?: boolean; patch?: Partial<DraftView>}) {
  const current = useSyncExternalStore(client.subscribe, client.getSnapshot, client.getSnapshot);
  const view = {...current, ...patch};
  return <><SetupSaveFlap client={client} view={view} workspace={CHAT_PROPS.workspace} dirty={dirty} />
    {current.pending ? <button onClick={() => {void client.retry().catch(() => undefined);}}>Retry save</button> : null}</>;
}

// Value: protects=one native fleet save with no library publication;
// fails_when=the active flap writes a Plugin or duplicates the save; seam=native client; no_claim=server persistence.
it("saves v1 through the fleet API with the same flap label and no Plugin requests", async () => {
  render(<Flap />);
  fireEvent.click(screen.getByRole("button", {name: "Save and publish"}));
  await screen.findByText("Setup saved · v1");
  expect(fixture.writes()).toHaveLength(1);
  expect(fixture.writes()[0]).toMatchObject({path: expect.stringMatching(/\/fleets$/), method: "POST",
    body: {name: REVIEW.effective.instance_name, review_digest: REVIEW.review_digest, start: {kind: "none"}}});
  expect(fixture.calls.some(call => /plugins|fleet-librar/.test(call.path))).toBe(false);
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.queryByRole("button", {name: "Save fleet"})).toBeNull();
  expect(screen.queryByRole("link", {name: "New fleet"})).toBeNull();
  expect(screen.getByRole("link", {name: "Open fleet"})).toBeTruthy();
});

it("keeps seconds and the spinner visible until the fleet receipt arrives, despite duplicate clicks", async () => {
  const pending = Promise.withResolvers<Response>();const base = fixture.fetch.getMockImplementation()!;
  fixture.fetch.mockImplementation((input, init) => init?.method === "POST" ? pending.promise : base(input, init));
  render(<Flap />);vi.useFakeTimers();
  const button = screen.getByRole("button", {name: "Save and publish"});
  act(() => {fireEvent.click(button);fireEvent.click(button);});
  expect(screen.getByRole("status", {name: "Saving setup"}).querySelector("[data-chat-spinner]")).toBeTruthy();
  expect(screen.getByText("Saving and publishing").getAttribute("data-chat-loading")).toBe("true");
  await act(async () => {vi.advanceTimersByTime(2_000);});
  expect(screen.getByText("2s")).toBeTruthy();
  expect(screen.queryByText("Setup saved · v1")).toBeNull();
  const writes = fixture.fetch.mock.calls.filter(([, init]) => init?.method === "POST");
  expect(writes).toHaveLength(1);
  await act(async () => {pending.resolve(await base(...writes[0]!));});
  expect(screen.getByText("Setup saved · v1")).toBeTruthy();
  expect(fixture.publicationAttempts).toBe(0);
});

it("retries an uncertain fleet save with the original key and body", async () => {
  fixture.failSave = true;render(<Flap />);
  fireEvent.click(screen.getByRole("button", {name: "Save and publish"}));
  fireEvent.click(await screen.findByRole("button", {name: "Retry save"}));
  await screen.findByText("Setup saved · v1");
  const writes = fixture.writes();expect(writes).toHaveLength(2);
  expect(writes[1]?.body).toEqual(writes[0]?.body);
  expect(writes[1]?.headers.get("Idempotency-Key")).toBe(writes[0]?.headers.get("Idempotency-Key"));
  expect(fixture.publicationAttempts).toBe(0);
});

it("materializes a local definition and then saves it without a Plugin lookup", async () => {
  client.fresh({kind: "definition", definition: REVIEW.effective.definition});await client.refreshSetup();
  render(<Flap />);fireEvent.click(screen.getByRole("button", {name: "Save and publish"}));
  await screen.findByText("Setup saved · v1");
  expect(fixture.writes().filter(call => call.path.endsWith("/drafts"))).toHaveLength(1);
  expect(fixture.saveAttempts).toBe(1);expect(fixture.publicationAttempts).toBe(0);
});

it("updates an existing fleet to v2 without cloning it or publishing a Plugin", async () => {
  fixture.draft = {...DRAFT, linked_fleet_id: FLEET_ID,
    review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}}};
  const base = fixture.fetch.getMockImplementation()!;let applied = false;let writes = 0;
  fixture.fetch.mockImplementation(async (input, init) => {
    const path = String(input);
    if (path.endsWith(`/fleets/${FLEET_ID}/revisions`) && init?.method === "POST") {
      applied = true;writes++;
      return response({fleet_id: FLEET_ID, setup_revision: "2", setup_etag: '"two"', etag: '"two"', schedules: []});
    }
    if (path.endsWith(`/fleets/${FLEET_ID}`) && applied) return response({...FLEET, setup_revision: "2"});
    return base(input, init);
  });
  await client.resume(DRAFT.id);render(<Flap />);
  fireEvent.click(screen.getByRole("button", {name: "Save and publish"}));
  await screen.findByText("Setup saved · v2");
  expect(writes).toBe(1);expect(fixture.saveAttempts).toBe(0);expect(fixture.publicationAttempts).toBe(0);
  expect(screen.getByRole("link", {name: "Open fleet"}).getAttribute("href")).toContain(FLEET_ID);
});

it.each([
  {busy: true}, {pending: true}, {inaccessible: true}, {error: "Unavailable"},
  {draft: {...DRAFT, candidate: undefined}}, {draft: {...DRAFT, review: undefined}},
  {draft: {...DRAFT, review: {...REVIEW, effective: {...REVIEW.effective, instance_name: ""}}}},
  {draft: null, preview: null},
])("keeps incomplete, unavailable or owned operations from exposing a save %#", async patch => {
  render(<Flap patch={patch} />);
  expect(screen.queryByRole("button", {name: "Save and publish"})).toBeNull();
  expect(fixture.writes()).toEqual([]);
});

it("retains manual changes without starting a write", () => {
  render(<Flap dirty />);
  expect(screen.getByText("Unsaved setup changes")).toBeTruthy();
  expect(screen.queryByRole("button", {name: "Save and publish"})).toBeNull();
  expect(fixture.writes()).toEqual([]);
});

it("retains the old publication tree for explicit re-enabling", async () => {
  feature.enabled = true;render(<Flap />);
  fireEvent.click(await screen.findByRole("button", {name: "Save and publish"}));
  await waitFor(() => expect(fixture.publicationAttempts).toBe(1));
  expect(fixture.saveAttempts).toBe(0);
});
