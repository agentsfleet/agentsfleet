import { useState } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { DraftView } from "@/lib/api/draft-state";
import { DraftClient } from "@/lib/api/drafts";
import { SetupInstructions } from "@/components/domain/chat/SetupInstructions";
import { DraftSaveButton } from "@/components/domain/chat/DraftSaveBar";
import { ChatFixture, CHAT_PROPS } from "./fixture";

const TOGGLE = "Edit instructions manually";
const INSTRUCTIONS = "Instructions";
const NAME = "Fleet name";
const CONFIG = "Runtime config";
const UPDATE = "Update instructions";
const SAVE = "Save changes";
const LOCAL = "Keep this manual instruction.";
const NEWER = "A newer setup from chat, including the safety rule.";
const NEW_NAME = "latest-fleet";
const NEW_CONFIG = {name: NEW_NAME, "x-agentsfleet": {tools: [], budget: {daily_nanos: "2000000"}}};
const LATEST = "Discard manual changes and load latest";
let fixture: ChatFixture;
let client: DraftClient;

function Editor({view}: {view: DraftView}) {
  const [dirty, setDirty] = useState(false);
  return <><SetupInstructions client={client} view={view} onDirtyChange={setDirty} />
    <DraftSaveButton client={client} view={view} dirty={dirty} /></>;
}

beforeEach(async () => {
  fixture = new ChatFixture(); fixture.mount();
  fixture.draft.review!.effective.target_setup_revision = "1";
  client = new DraftClient({...CHAT_PROPS, subject: "creator"});
  await client.initialize(); await client.resume(fixture.draft.id);
});
afterEach(() => {cleanup(); client.dispose(); vi.unstubAllGlobals();});

async function newerCandidate(): Promise<void> {
  const definition = {...fixture.draft.candidate!.definition, instructions_md: NEWER, config: NEW_CONFIG};
  fixture.draft = {...fixture.draft, candidate: {...fixture.draft.candidate!, revision: "2", definition},
    review: {...fixture.draft.review!, effective: {...fixture.draft.review!.effective, definition, instance_name: NEW_NAME}}};
  await client.refreshSetup();
}

function configField() {
  if (!screen.queryByLabelText(CONFIG)) fireEvent.click(screen.getByRole("button", {name: "Advanced settings"}));
  return screen.getByLabelText(CONFIG);
}

it("rebases an untouched folded editor to the latest chat instructions, name and configuration", async () => {
  const page = render(<Editor view={client.getSnapshot()} />);
  fireEvent.click(screen.getByRole("button", {name: TOGGLE}));
  fireEvent.click(screen.getByRole("button", {name: TOGGLE}));
  await newerCandidate(); page.rerender(<Editor view={client.getSnapshot()} />);
  expect(screen.getByRole("button", {name: SAVE})).toHaveProperty("disabled", false);
  fireEvent.click(screen.getByRole("button", {name: TOGGLE}));
  expect(screen.getByLabelText(INSTRUCTIONS)).toHaveProperty("value", NEWER);
  expect(screen.getByLabelText(NAME)).toHaveProperty("value", NEW_NAME);
  expect(configField()).toHaveProperty("value", JSON.stringify(NEW_CONFIG, null, 2));
  expect(screen.queryByRole("alert")).toBeNull();
  expect(fixture.writes()).toHaveLength(0);
});

it("keeps real manual work and blocks stale replacement until the person discards it", async () => {
  const page = render(<Editor view={client.getSnapshot()} />);
  fireEvent.click(screen.getByRole("button", {name: TOGGLE}));
  fireEvent.change(screen.getByLabelText(INSTRUCTIONS), {target: {value: LOCAL}});
  fireEvent.change(screen.getByLabelText(NAME), {target: {value: "local-name"}});
  fireEvent.change(configField(), {target: {value: "{ unfinished"}});
  fireEvent.click(screen.getByRole("button", {name: TOGGLE}));
  await newerCandidate(); page.rerender(<Editor view={client.getSnapshot()} />);
  expect(screen.getByRole("button", {name: SAVE})).toHaveProperty("disabled", true);
  fireEvent.click(screen.getByRole("button", {name: TOGGLE}));
  expect(screen.getByLabelText(INSTRUCTIONS)).toHaveProperty("value", LOCAL);
  expect(screen.getByLabelText(NAME)).toHaveProperty("value", "local-name");
  expect(configField()).toHaveProperty("value", "{ unfinished");
  expect(screen.getByRole("alert").textContent).toContain("Your manual edits are still here");
  const update = screen.getByRole("button", {name: UPDATE});
  expect(update).toHaveProperty("disabled", true); fireEvent.click(update);
  expect(fixture.writes()).toHaveLength(0);
  fireEvent.click(screen.getByRole("button", {name: LATEST}));
  expect(screen.getByLabelText(INSTRUCTIONS)).toHaveProperty("value", NEWER);
  expect(screen.getByLabelText(NAME)).toHaveProperty("value", NEW_NAME);
  expect(configField()).toHaveProperty("value", JSON.stringify(NEW_CONFIG, null, 2));
  expect(screen.getByRole("button", {name: SAVE})).toHaveProperty("disabled", false);
  expect(screen.queryByRole("alert")).toBeNull();
  expect(fixture.writes()).toHaveLength(0);
});

it("rebases after a manual edit is undone instead of misclassifying it as a conflict", async () => {
  const page = render(<Editor view={client.getSnapshot()} />);
  fireEvent.click(screen.getByRole("button", {name: TOGGLE}));
  const input = screen.getByLabelText(INSTRUCTIONS);
  fireEvent.change(input, {target: {value: LOCAL}});
  fireEvent.change(input, {target: {value: fixture.draft.candidate!.definition.instructions_md}});
  await newerCandidate(); page.rerender(<Editor view={client.getSnapshot()} />);
  expect(input).toHaveProperty("value", NEWER);
  expect(screen.getByRole("button", {name: SAVE})).toHaveProperty("disabled", false);
  expect(screen.queryByRole("alert")).toBeNull();
});

it("recognizes manual values already accepted into the latest candidate as clean", async () => {
  const page = render(<Editor view={client.getSnapshot()} />);
  fireEvent.click(screen.getByRole("button", {name: TOGGLE}));
  fireEvent.change(screen.getByLabelText(INSTRUCTIONS), {target: {value: NEWER}});
  fireEvent.change(screen.getByLabelText(NAME), {target: {value: NEW_NAME}});
  fireEvent.change(configField(), {target: {value: JSON.stringify(NEW_CONFIG, null, 2)}});
  await newerCandidate(); page.rerender(<Editor view={client.getSnapshot()} />);
  expect(screen.getByRole("button", {name: SAVE})).toHaveProperty("disabled", false);
  expect(screen.queryByRole("alert")).toBeNull();
  expect(fixture.writes()).toHaveLength(0);
});
