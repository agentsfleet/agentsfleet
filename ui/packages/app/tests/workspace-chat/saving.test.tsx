import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { saveInChat, ChatFixture, CHAT_PROPS, chatRouter, chooseSetupAction } from "./fixture";
import { WorkspaceChat } from "@/components/domain/setup/conversation/WorkspaceChat";
import { CHAT_PLACEHOLDER } from "@/components/domain/setup/chat-defaults";
import { FLEET_ID, WORKSPACE } from "@/tests/helpers/draft-fixtures";

const plugins = vi.hoisted(() => ({enabled: false}));
vi.mock("@/lib/plugin-ui", () => ({pluginsUiEnabled: () => plugins.enabled}));
let fixture: ChatFixture;
beforeEach(() => { plugins.enabled = false; fixture = new ChatFixture(); fixture.mount(); });
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

async function converse(): Promise<void> {
  render(<WorkspaceChat {...CHAT_PROPS} />);
  const input = await screen.findByLabelText(CHAT_PLACEHOLDER);
  await waitFor(() => expect(input).toHaveProperty("disabled", false));
  expect(screen.getByRole("button", {name: /^(Send message|New fleet from message)$/})).toHaveProperty("disabled", true);
  fireEvent.change(input, {target: {value: "Investigate failures"}});
  fireEvent.click(screen.getByRole("button", {name: /^(Send message|New fleet from message)$/}));
  await screen.findByText("Your setup is ready.");
}

async function setup() {
  const region = await screen.findByRole("region", {name: "Fleet setup"});
  expect(within(region).queryByRole("button", {name: "Save fleet"})).toBeNull();
  expect(await screen.findByRole("region", {name: "Repositories"})).toBeTruthy();
  expect(screen.queryByRole("dialog", {name: "Fleet setup"})).toBeNull();
  expect(screen.queryByRole("dialog")).toBeNull();
  return region;
}

describe("private chat Save", () => {
  it("restores the editable Plugin starter only when the preserved UI tree is enabled", async () => {
    plugins.enabled = true;
    render(<WorkspaceChat {...CHAT_PROPS} />);
    const starter = await screen.findByRole("button", {name: "Create a plugin"});
    await waitFor(() => expect(starter).toHaveProperty("disabled", false));
    fireEvent.click(starter);
    expect(screen.getByLabelText(CHAT_PLACEHOLDER)).toHaveProperty("value", expect.stringContaining("reusable fleet setup"));
    expect(fixture.writes()).toEqual([]);
    expect(fixture.calls.some(call => call.path.includes("/plugins"))).toBe(false);
  });
  it("loads editable starters and sends the reviewed questions only after the user submits", async () => {
    render(<WorkspaceChat {...CHAT_PROPS} />);
    const suggestion = await screen.findByRole("button", {name: /Watch a pull request/});
    await waitFor(() => expect(suggestion).toHaveProperty("disabled", false));
    for (const [title, text] of [
      ["Check a deploy", "Check a deployment and tell me whether it is healthy. Ask which environment and release to check, and which evidence should count as success."],
    ]) {
      fireEvent.click(screen.getByRole("button", {name: title}));
      expect(screen.getByLabelText(CHAT_PLACEHOLDER)).toHaveProperty("value", text);
    }
    expect(screen.queryByRole("button", {name: "Create a plugin"})).toBeNull();
    fireEvent.click(suggestion);
    const input = screen.getByLabelText(CHAT_PLACEHOLDER);
    if (!(input instanceof HTMLTextAreaElement)) throw new Error("The setup composer must expose editable text");
    expect(input.value).toContain("Does the review cover the requested pull request at its current head?");
    expect(input.value).toContain("Are the findings supported by concrete code or check evidence?");
    expect(input.value).toContain("Do not push code, approve, merge, close, or deploy.");
    const edited = `${input.value}\n\nOnly review the API changes. Add a completion question about session expiry.`;
    fireEvent.change(input, {target: {value: edited}});
    expect(fixture.writes()).toHaveLength(0);
    expect(fixture.saveAttempts).toBe(0);
    fireEvent.click(screen.getByRole("button", {name: /^(Send message|New fleet from message)$/}));
    await screen.findByText("Your setup is ready.");
    expect(fixture.writes()).toHaveLength(1);
    expect(fixture.writes()[0]?.body).toMatchObject({message: edited, source: {kind: "blank"}});
    expect(fixture.saveAttempts).toBe(0);
    await waitFor(() => expect(screen.getByRole("button", {name: "Save and publish"})).toHaveProperty("disabled", false));
  });
  it("sends ordinary text and opens the confirmed fleet after the native chat Save", async () => {
    await converse();
    const message = fixture.writes()[0];
    expect(message?.body).toMatchObject({message: "Investigate failures", source: {kind: "blank"}});
    expect(fixture.saveAttempts).toBe(0);
    await setup();
    await saveInChat();
    await screen.findByRole("heading", {name: "Setup saved · v1"});
    expect(fixture.saveAttempts).toBe(1);
    expect(chatRouter.push).not.toHaveBeenCalled();
    await waitFor(() => expect(chatRouter.replace).toHaveBeenCalledExactlyOnceWith(`/w/${WORKSPACE}/fleets/${FLEET_ID}`, {scroll: false}));
    expect((await screen.findByRole("link", {name: "Open fleet"})).getAttribute("href")).toBe(`/w/${WORKSPACE}/fleets/${FLEET_ID}`);
    expect(fixture.writes().filter((call) => call.path.endsWith("/plugins"))).toHaveLength(0);
    const save = fixture.writes().find((call) => call.path.endsWith("/messages"));
    expect(save?.body).toMatchObject({message: "Save", save: true, operation_id: expect.any(String)});
  });

  it("retries an uncertain Save with the original key and body", async () => {
    fixture.failSave = true;
    await converse();
    await setup();
    await saveInChat();
    await screen.findAllByText("The request could not be confirmed. Retry the same operation.");
    const writes = fixture.writes().length;
    expect(fixture.writes().filter(call => call.path.endsWith("/messages"))).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", {name: "Dismiss error notice"}));
    expect(screen.getByRole("alert", {name: "Chat recovery"}).textContent).toContain("The request could not be confirmed. Retry the same operation.");
    expect(fixture.writes()).toHaveLength(writes);
    fireEvent.click(screen.getByRole("button", {name: "Retry"}));
    await screen.findByRole("heading", {name: "Setup saved · v1"});
    const attempts = fixture.writes().filter((call) => call.path.endsWith("/messages"));
    expect(attempts).toHaveLength(2);
    expect(attempts[1]?.body).toEqual(attempts[0]?.body);
    expect(attempts[1]?.body).toMatchObject({operation_id: expect.any(String), save: true});
  });

  it("saves a fleet version from the flap without publishing a Plugin or starting a run", async () => {
    await converse();
    await setup();
    fireEvent.click(screen.getByRole("button", {name: "Save and publish"}));
    expect(screen.queryByRole("dialog")).toBeNull();
    await screen.findByRole("heading", {name: "Setup saved · v1"});
    expect(fixture.calls.some(call => /plugins|fleet-librar/.test(call.path))).toBe(false);
    expect(fixture.saveAttempts).toBe(1);
    expect(fixture.writes().find(call => call.path.endsWith("/fleets"))?.body).toMatchObject({name: "repairer", start: {kind: "none"}});
  });

  it("edits instructions through the menu and keeps saving the fleet explicit", async () => {
    await converse();
    await chooseSetupAction("Instructions");
    const dialog = screen.getByRole("dialog", {name: "Instructions"});
    expect(within(dialog).getByRole("textbox", {name: "Instructions"})).toHaveProperty("value", "Investigate failures.\n");
    fireEvent.change(within(dialog).getByRole("textbox", {name: "Instructions"}), {target: {value: "Inspect failures safely"}});
    fireEvent.click(within(dialog).getByRole("button", {name: "Save changes"}));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(fixture.saveAttempts).toBe(0);
  });

  it("clears private messages when the account changes", async () => {
    const view = render(<WorkspaceChat {...CHAT_PROPS} draft={fixture.draft.id} />);
    await screen.findByText("Your setup is ready.");
    fixture.messages = [];
    view.rerender(<WorkspaceChat account="account-two" workspace={WORKSPACE} />);
    await waitFor(() => expect(screen.queryByText("Your setup is ready.")).toBeNull());
    expect(await screen.findByText("What should your fleet do?")).not.toBeNull();
  });
});
