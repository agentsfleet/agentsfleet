import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { DraftClient } from "@/lib/api/drafts";
import { DefinitionEditor } from "@/components/domain/chat/DefinitionEditor";
import { ChatFixture, CHAT_PROPS } from "./fixture";
import { DRAFT, MODEL_REF, REVIEW, response } from "@/tests/helpers/draft-fixtures";

let fixture: ChatFixture;
let client: DraftClient;
beforeEach(() => { fixture = new ChatFixture(); fixture.mount(); client = new DraftClient({...CHAT_PROPS, subject: "creator"}); });
afterEach(() => {cleanup(); client.dispose(); vi.unstubAllGlobals();});

it("provides editable defaults and rejects malformed configuration without writing", () => {
  render(<DefinitionEditor client={client} view={client.getSnapshot()} onUpdated={vi.fn()} />);
  expect(screen.getByLabelText("Fleet name")).toHaveProperty("value", "my-fleet");
  expect(screen.getByText(/Defaults: chat and steer messages only/)).toBeTruthy();
  fireEvent.change(runtimeConfig(), {target: {value: "{"}});
  fireEvent.click(screen.getByRole("button", {name: "Update instructions"}));
  expect(screen.getByRole("alert").textContent).toBe("Config must be valid JSON.");
  expect(fixture.writes()).toHaveLength(0);
});

it("retains editable values when a valid JSON value has the wrong configuration shape", async () => {
  render(<DefinitionEditor client={client} view={client.getSnapshot()} onUpdated={vi.fn()} />);
  fireEvent.change(runtimeConfig(), {target: {value: "[]"}});
  fireEvent.click(screen.getByRole("button", {name: "Update instructions"}));
  expect(screen.getByRole("alert").textContent).toBe("Runtime config must be a JSON object.");
  expect(runtimeConfig()).toHaveProperty("value", "[]");
  expect(fixture.writes()).toHaveLength(0);
});

it("updates and reviews an existing candidate before announcing completion", async () => {
  await client.initialize(); await client.resume(DRAFT.id);
  const reviewed = vi.fn();
  render(<DefinitionEditor client={client} view={client.getSnapshot()} onUpdated={reviewed} />);
  fireEvent.change(screen.getByLabelText("Instructions"), {target: {value: "Investigate safely"}});
  fireEvent.change(screen.getByLabelText("Fleet name"), {target: {value: "safer-fleet"}});
  fireEvent.change(runtimeConfig(), {target: {value: JSON.stringify({...REVIEW.effective.definition.config, name: "renamed-definition"})}});
  fireEvent.click(screen.getByRole("button", {name: "Update instructions"}));
  await waitFor(() => expect(reviewed).toHaveBeenCalledOnce());
  expect(fixture.writes()[0]?.body).toMatchObject({instance_name: "safer-fleet",
    config_ops: [{op: "replace", path: "/config/name", value: "renamed-definition"}], instruction_edits: [{start_byte: 0, end_byte: 22, text: "Investigate safely"}]});
  expect(client.getSnapshot().draft?.review?.review_digest).toBe(REVIEW.review_digest);
});

it("edits a selected preview and keeps a refused update visible", async () => {
  await client.initialize();
  client.fresh({kind: "definition", definition: REVIEW.effective.definition}); await client.refreshSetup();
  const reviewed = vi.fn();
  render(<DefinitionEditor client={client} view={client.getSnapshot()} onUpdated={reviewed} />);
  expect(screen.getByLabelText("Instructions")).toHaveProperty("value", REVIEW.effective.definition.instructions_md);
  fixture.fetch.mockResolvedValueOnce(response({}, 503));
  fireEvent.click(screen.getByRole("button", {name: "Update instructions"}));
  await screen.findByRole("alert");
  expect(reviewed).not.toHaveBeenCalled(); expect(client.getSnapshot().model).toBe(MODEL_REF);
});

it("prevents editing a draft without a candidate or after saving", () => {
  const view = render(<DefinitionEditor client={client} view={{...client.getSnapshot(), draft: {...DRAFT, candidate: undefined, review: undefined}}} onUpdated={vi.fn()} />);
  expect(screen.getByRole("button", {name: "Update instructions"})).toHaveProperty("disabled", true);
  view.rerender(<DefinitionEditor client={client} view={{...client.getSnapshot(), draft: {...DRAFT, state: "saving"}}} onUpdated={vi.fn()} />);
  expect(screen.getByRole("button", {name: "Update instructions"})).toHaveProperty("disabled", true);
});

const UPDATE_BUTTON = "Update instructions";
const CONFIG_LABEL = "Runtime config";
const REFUSED_WEBHOOK = "That webhook source cannot be verified. Choose a supported provider.";

it.each([
  {type: "webhook", source: "GitHub"},
  {type: "webhook", source: "zoho_desk"},
  {type: "webhook", source: "github/elsewhere"},
  {type: "webhook", source: "github\n"},
  {type: "webhook", source: "approval"},
  {type: "webhook", source: "approval", signature: {secret_ref: "signing-secret"}},
  {type: "webhook", source: "custom"},
  {type: "webhook", source: "custom", signature: {}},
  {type: "webhook", source: "custom", signature: {secret_ref: ""}},
  {type: "webhook", source: "github", signature: {secret_ref: "signing-secret", header: "removed"}},
  {type: "webhook", source: "github", signature: {secret_ref: "signing-secret", prefix: "removed"}},
  {type: "webhook", source: "github", signature: {secret_ref: "signing-secret", ts_header: "removed"}},
  {type: "retired-trigger"},
  null,
])("retains invalid trigger edits without writing (%j)", async (trigger) => {
  await client.initialize(); await client.resume(DRAFT.id);
  const reviewed = vi.fn();
  render(<DefinitionEditor client={client} view={client.getSnapshot()} onUpdated={reviewed} />);
  const config = JSON.stringify({...REVIEW.effective.definition.config, "x-agentsfleet": {triggers: [trigger]}});
  fireEvent.change(runtimeConfig(), {target: {value: config}});
  fireEvent.click(screen.getByRole("button", {name: UPDATE_BUTTON}));
  expect((await screen.findByRole("alert")).textContent).toMatch(/trigger|Webhook/i);
  expect(runtimeConfig()).toHaveProperty("value", config);
  expect(reviewed).not.toHaveBeenCalled();
  expect(fixture.writes()).toHaveLength(0);
});

it.each([
  undefined, [],
  [{type: "cron", schedule: "0 * * * *"}],
  [{type: "webhook", source: "github"}],
  [{type: "webhook", source: "linear"}],
  [{type: "webhook", source: "slack"}],
  [{type: "webhook", source: "custom-source", signature: {secret_ref: "signing-secret"}}],
  [{type: "mention", source: "Chat_Provider", channels: ["C123"]}],
].map((triggers) => ({triggers})))("updates supported trigger configurations (%j)", async ({triggers}) => {
  await client.initialize(); await client.resume(DRAFT.id);
  const reviewed = vi.fn();
  render(<DefinitionEditor client={client} view={client.getSnapshot()} onUpdated={reviewed} />);
  fireEvent.change(runtimeConfig(), {target: {value: JSON.stringify({...REVIEW.effective.definition.config,
    "x-agentsfleet": {triggers}})}});
  fireEvent.click(screen.getByRole("button", {name: UPDATE_BUTTON}));
  await waitFor(() => expect(reviewed).toHaveBeenCalledOnce());
  expect(screen.queryByRole("alert")).toBeNull();
});

it("shows a daemon webhook refusal verbatim and retains the authored configuration", async () => {
  await client.initialize(); await client.resume(DRAFT.id);
  const reviewed = vi.fn();
  render(<DefinitionEditor client={client} view={client.getSnapshot()} onUpdated={reviewed} />);
  const config = JSON.stringify({...REVIEW.effective.definition.config,
    "x-agentsfleet": {triggers: [{type: "webhook", source: "github"}]}});
  fireEvent.change(runtimeConfig(), {target: {value: config}});
  fixture.fetch.mockResolvedValueOnce(response({user_message: REFUSED_WEBHOOK}, 400));
  fireEvent.click(screen.getByRole("button", {name: UPDATE_BUTTON}));
  expect((await screen.findByRole("alert")).textContent).toBe(REFUSED_WEBHOOK);
  expect(runtimeConfig()).toHaveProperty("value", config);
  expect(reviewed).not.toHaveBeenCalled();
  expect(client.getSnapshot().draft?.candidate).toEqual(DRAFT.candidate);
});

it("accepts a configuration with no trigger extension", async () => {
  await client.initialize(); await client.resume(DRAFT.id);
  const reviewed = vi.fn();
  render(<DefinitionEditor client={client} view={client.getSnapshot()} onUpdated={reviewed} />);
  fireEvent.change(runtimeConfig(), {target: {value: JSON.stringify({name: "repairer"})}});
  fireEvent.click(screen.getByRole("button", {name: UPDATE_BUTTON}));
  await waitFor(() => expect(reviewed).toHaveBeenCalledOnce());
  expect(screen.queryByRole("alert")).toBeNull();
});

function runtimeConfig() {
  if (!screen.queryByLabelText(CONFIG_LABEL)) fireEvent.click(screen.getByRole("button", {name: "Advanced settings"}));
  return screen.getByLabelText(CONFIG_LABEL);
}
