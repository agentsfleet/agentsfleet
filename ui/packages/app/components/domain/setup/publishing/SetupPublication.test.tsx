import { useSyncExternalStore } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ChatFixture, CHAT_PROPS } from "@/tests/workspace-chat/fixture";
import { DRAFT, REVIEW } from "@/tests/helpers/draft-fixtures";
import { DraftClient } from "@/lib/api/drafts";
import { SetupPublication } from "./SetupPublication";

let client: DraftClient;
let fixture: ChatFixture;
beforeEach(async () => {
  fixture = new ChatFixture(); fixture.mount();
  client = new DraftClient({...CHAT_PROPS, subject: "creator"});
  await client.initialize(); await client.resume(DRAFT.id);
});
afterEach(() => {cleanup();client.dispose();vi.useRealTimers();vi.unstubAllGlobals();});
function Publication({dirty = false}: {dirty?: boolean}) {
  const view = useSyncExternalStore(client.subscribe, client.getSnapshot, client.getSnapshot);
  return <><SetupPublication client={client} view={view} workspace={CHAT_PROPS.workspace} dirty={dirty} />
    {view.pending ? <button onClick={() => {void client.retry();}}>Retry</button> : null}</>;
}

// Value: protects=single direct publication with visible elapsed progress;
// fails_when=double clicks publish twice or open the removed popup; seam=native transport; no_claim=server persistence.
it("keeps the spinner and ticking seconds on the flap until the native receipt arrives", async () => {
  const pending = Promise.withResolvers<Response>();
  const base = fixture.fetch.getMockImplementation()!;
  fixture.fetch.mockImplementation((input, init) => init?.method === "POST" ? pending.promise : base(input, init));
  render(<Publication />);
  vi.useFakeTimers();
  const publish = screen.getByRole("button", {name: "Save and publish"});
  act(() => {fireEvent.click(publish);fireEvent.click(publish);});
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.getByText("Saving and publishing").getAttribute("data-chat-loading")).toBe("true");
  expect(screen.getByRole("status", {name: "Publishing setup"}).querySelector("[data-chat-spinner]")).toBeTruthy();
  await act(async () => {vi.advanceTimersByTime(2_000);});
  expect(screen.getByText("2s")).toBeTruthy();
  expect(screen.queryByText("Setup published")).toBeNull();
  const calls = fixture.fetch.mock.calls.filter(([, init]) => init?.method === "POST");
  expect(calls).toHaveLength(1);
  await act(async () => {pending.resolve(await base(...calls[0]!));});
  expect(screen.getByText("Setup published")).toBeTruthy();
  expect(screen.queryByText("Saving and publishing")).toBeNull();
  expect(fixture.saveAttempts).toBe(0);
});

it("retries an interrupted publication with the original key and body without saving a fleet", async () => {
  fixture.failPublish = true;
  render(<Publication />);
  fireEvent.click(screen.getByRole("button", {name: "Save and publish"}));
  fireEvent.click(await screen.findByRole("button", {name: "Retry"}));
  await screen.findByText("Setup published");
  const writes = fixture.writes();
  expect(writes).toHaveLength(2);
  expect(writes[1]?.body).toEqual(writes[0]?.body);
  expect(fixture.publicationKeys.size).toBe(1);
  expect(fixture.saveAttempts).toBe(0);
});

it("does not publish while manual edits are unsaved", async () => {
  const {rerender} = render(<Publication dirty />);
  expect(screen.queryByRole("button", {name: "Save and publish"})).toBeNull();
  expect(fixture.writes()).toHaveLength(0);
  rerender(<Publication />);
  fireEvent.click(screen.getByRole("button", {name: "Save and publish"}));
  await waitFor(() => expect(fixture.publicationAttempts).toBe(1));
  expect(fixture.writes()[0]?.body).toMatchObject({label: REVIEW.effective.instance_name});
});

it("publishes an already materialized source without creating another draft", async () => {
  client.fresh({kind: "definition", definition: REVIEW.effective.definition});await client.refreshSetup();
  await client.edit({definition: REVIEW.effective.definition, instanceName: REVIEW.effective.instance_name});
  const creations = fixture.writes().length;
  render(<Publication />);
  fireEvent.click(screen.getByRole("button", {name: "Save and publish"}));
  await screen.findByText("Setup published");
  expect(fixture.writes()).toHaveLength(creations + 1);
  expect(fixture.writes().at(-1)?.path).toMatch(/\/plugins$/);
});
