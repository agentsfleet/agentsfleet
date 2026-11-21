import { useSyncExternalStore } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { SetupReview } from "@/components/domain/setup/summary/SetupReview";
import { DraftClient } from "@/lib/api/drafts";
import { ChatFixture, CHAT_PROPS } from "./fixture";
import { DRAFT, REVIEW, response } from "@/tests/helpers/draft-fixtures";

let fixture: ChatFixture;
let client: DraftClient;
const {secretList} = vi.hoisted(() => ({secretList: vi.fn()}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/actions", () => ({listSecretsAction: secretList}));
const network = {http_egress: "restricted", package_manager_domains: true, allow: ["api.github.com"], read_only: true};
const baseConfig = {tools: ["shell"], triggers: [], budget: {daily_nanos: "1000000000"}, network};
beforeEach(() => {fixture = new ChatFixture();fixture.mount();client = new DraftClient({...CHAT_PROPS, subject: "creator"});});
afterEach(() => {cleanup();client.dispose();vi.unstubAllGlobals();});
async function open(extension: Record<string, unknown> = baseConfig) {
  const definition = {...REVIEW.effective.definition, config: {name: "repairer", "x-agentsfleet": extension}};
  fixture.draft = {...DRAFT, candidate: {...DRAFT.candidate!, definition}, review: {...REVIEW,
    effective: {...REVIEW.effective, definition}, requirements: {...baseConfig, repositories: [], credentials: []}}};
  await client.initialize();await client.resume(DRAFT.id);
  render(<LiveSummary />);
}
function LiveSummary() {
  const view = useSyncExternalStore(client.subscribe, client.getSnapshot, client.getSnapshot);
  return <SetupReview client={client} view={view} workspace={CHAT_PROPS.workspace} />;
}
const patch = () => fixture.writes().find(call => call.method === "PATCH");

// Value: protects=policy edits preserve existing restrictions and package choices;
// fails_when=toggling public access replaces the policy object; seam=native draft edit; no_claim=server authorization.
it("preserves the domain list, read-only flag, packages, tools and budget when toggling Allow all", async () => {
  await open();
  fireEvent.click(screen.getByRole("switch", {name: "Allow all public internet access"}));
  await waitFor(() => expect(client.getSnapshot().busy).toBe(false));
  expect(patch()?.body).toMatchObject({config_ops: [{path: "/config/x-agentsfleet", value: {
    ...baseConfig, network: {...network, http_egress: "public"}}}]});
  expect(fixture.saveAttempts).toBe(0);
});

it("changes package registry access without changing existing destination restrictions", async () => {
  await open();
  fireEvent.click(screen.getByRole("switch", {name: "Enable package manager domains"}));
  await waitFor(() => expect(client.getSnapshot().busy).toBe(false));
  expect(patch()?.body).toMatchObject({config_ops: [{value: {...baseConfig, network: {...network, package_manager_domains: false}}}]});
});

it.each([false, true])("adds repositories with a valid paired binding while preserving existing write policy (%s)", async existing => {
  const access = existing ? {repositories: ["agentsfleet/web"], repository_access: "write", repository_base: "main"} : {};
  await open({...baseConfig, ...access});
  fireEvent.click(screen.getByRole("button", {name: "Manage repositories"}));
  const field = await screen.findByRole("textbox", {name: "Repositories"});
  fireEvent.change(field, {target: {value: "agentsfleet/web\n agentsfleet/docs \n"}});
  fireEvent.click(screen.getByRole("button", {name: "Apply selection"}));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(patch()?.body).toMatchObject({config_ops: [{value: {...baseConfig, repositories: ["agentsfleet/web", "agentsfleet/docs"], repository_access: existing ? "write" : "read", ...(existing ? {repository_base: "main"} : {})}}]});
  expect(fixture.saveAttempts).toBe(0);
});

it("clears an empty repository binding as a whole rather than sending an invalid empty list", async () => {
  await open({...baseConfig, repositories: ["agentsfleet/web"], repository_access: "write", repository_base: "main", repository_publication: "branch"});
  fireEvent.click(screen.getByRole("button", {name: "Manage repositories"}));
  fireEvent.change(await screen.findByRole("textbox", {name: "Repositories"}), {target: {value: ""}});
  fireEvent.click(screen.getByRole("button", {name: "Apply selection"}));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(patch()?.body).toMatchObject({config_ops: [{value: baseConfig}]});
});

it("keeps refused repository edits available for correction and reload", async () => {
  await open();fireEvent.click(screen.getByRole("button", {name: "Manage repositories"}));
  const field = await screen.findByRole("textbox", {name: "Repositories"});
  fireEvent.change(field, {target: {value: "agentsfleet/docs"}});
  fixture.fetch.mockResolvedValueOnce(response({user_message: "Repository access changed"}, 412));
  fireEvent.click(screen.getByRole("button", {name: "Apply selection"}));
  expect((await screen.findByRole("alert")).textContent).toContain("Repository access changed");
  expect(field).toHaveProperty("value", "agentsfleet/docs");
  fireEvent.click(screen.getByRole("button", {name: "Refresh setup"}));
  await waitFor(() => expect(client.getSnapshot().busy).toBe(false));
  fireEvent.click(screen.getByRole("button", {name: "Cancel"}));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
});

it("keeps native failures visible without acknowledging a network change", async () => {
  await open();fixture.fetch.mockResolvedValueOnce(response({user_message: "Policy could not be changed"}, 422));
  fireEvent.click(screen.getByRole("switch", {name: "Allow all public internet access"}));
  await waitFor(() => expect(client.getSnapshot().error).toBe("Policy could not be changed"));
  expect(screen.getByRole("switch", {name: "Allow all public internet access"}).getAttribute("aria-checked")).toBe("false");
});

it("selects stored secret names through the existing scoped catalog and native edit", async () => {
  secretList.mockResolvedValue({ok: true, data: {secrets: [{name: "github-token", kind: "custom_secret", created_at: "0"}]}});
  await open();fireEvent.click(screen.getByRole("button", {name: "Manage secrets"}));
  fireEvent.click(await screen.findByRole("checkbox", {name: "github-token"}));
  fireEvent.click(screen.getByRole("button", {name: "Apply selection"}));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(patch()?.body).toMatchObject({config_ops: [{value: {...baseConfig, credentials: ["github-token"]}}]});
  expect(secretList).toHaveBeenCalledWith(CHAT_PROPS.workspace);
});

it("closes selection with Escape and keeps the draft unchanged", async () => {
  await open();fireEvent.click(screen.getByRole("button", {name: "Manage repositories"}));
  const dialog = await screen.findByRole("dialog");
  fireEvent.keyDown(dialog, {key: "Escape"});
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(fixture.writes()).toEqual([]);
});

it("retries source materialization with its original identity after a lost response", async () => {
  await client.initialize();
  client.fresh({kind: "definition", definition: REVIEW.effective.definition});await client.refreshSetup();
  const base = fixture.fetch.getMockImplementation()!;
  let lost = true;
  fixture.fetch.mockImplementation(async (input, init) => {
    if (lost && init?.method === "POST") {lost = false;await base(input, init);throw new TypeError("lost turn response");}
    return base(input, init);
  });
  render(<LiveSummary />);fireEvent.click(screen.getByRole("button", {name: "Manage repositories"}));
  fireEvent.change(await screen.findByRole("textbox", {name: "Repositories"}), {target: {value: "agentsfleet/docs"}});
  fireEvent.click(screen.getByRole("button", {name: "Apply selection"}));
  fireEvent.click(await screen.findByRole("button", {name: "Retry update"}));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  const creations = fixture.writes().filter(call => call.path.endsWith("/drafts"));
  expect(creations).toHaveLength(2);expect(creations[1]?.body).toEqual(creations[0]?.body);
  expect(fixture.saveAttempts).toBe(0);
});

it("retains the selection dialog while its native patch is still running", async () => {
  await open();fireEvent.click(screen.getByRole("button", {name: "Manage repositories"}));
  fireEvent.change(await screen.findByRole("textbox", {name: "Repositories"}), {target: {value: "agentsfleet/docs"}});
  const pending = Promise.withResolvers<Response>();const base = fixture.fetch.getMockImplementation()!;
  fixture.fetch.mockImplementation((input, init) => init?.method === "PATCH" ? pending.promise : base(input, init));
  fireEvent.click(screen.getByRole("button", {name: "Apply selection"}));
  const dialog = screen.getByRole("dialog");
  fireEvent.keyDown(dialog, {key: "Escape"});
  expect(screen.getByRole("dialog")).toBe(dialog);
  await act(async () => {pending.resolve(response(fixture.draft));});
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
});
