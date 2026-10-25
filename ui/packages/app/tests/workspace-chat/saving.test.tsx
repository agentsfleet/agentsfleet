import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { ChatFixture, CHAT_PROPS, chatRouter, chooseSetupAction, openSetupReview } from "./fixture";
import { WorkspaceChat } from "@/components/domain/chat/WorkspaceChat";
import { CHAT_EXAMPLES, CHAT_PLACEHOLDER } from "@/components/domain/chat/chat-defaults";
import { FLEET_ID, WORKSPACE } from "@/tests/helpers/draft-fixtures";

let fixture: ChatFixture;
beforeEach(() => { fixture = new ChatFixture(); fixture.mount(); });
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

async function converse(): Promise<void> {
  render(<WorkspaceChat {...CHAT_PROPS} />);
  const input = await screen.findByLabelText(CHAT_PLACEHOLDER);
  await waitFor(() => expect(input).toHaveProperty("disabled", false));
  expect(screen.getByRole("button", {name: "Send message"})).toHaveProperty("disabled", true);
  fireEvent.change(input, {target: {value: "Investigate failures"}});
  fireEvent.click(screen.getByRole("button", {name: "Send message"}));
  await screen.findByText("Your setup is ready.");
}

async function setup() {
  const region = await screen.findByRole("region", {name: "Fleet setup"});
  expect(within(region).queryByRole("button", {name: "Save fleet"})).toBeNull();
  expect(screen.queryByRole("button", {name: "Review setup"})).toBeNull();
  expect(screen.queryByRole("dialog")).toBeNull();
  return region;
}

describe("private chat Save", () => {
  it("lets the user edit a suggested job before sending it", async () => {
    render(<WorkspaceChat {...CHAT_PROPS} />);
    const suggestion = await screen.findByRole("button", {name: /Review pull requests/});
    await waitFor(() => expect(suggestion).toHaveProperty("disabled", false));
    fireEvent.click(suggestion);
    expect(screen.getByLabelText(CHAT_PLACEHOLDER)).toHaveProperty("value", CHAT_EXAMPLES[0]);
    expect(fixture.writes()).toHaveLength(0);
    fireEvent.click(screen.getByRole("button", {name: "Send message"}));
    await screen.findByText("Your setup is ready.");
    expect(fixture.writes()).toHaveLength(1);
    expect(fixture.writes()[0]?.body).toMatchObject({message: CHAT_EXAMPLES[0]});
  });
  it("sends ordinary text and saves from the header, then opens Fleet Chat", async () => {
    await converse();
    const message = fixture.writes()[0];
    expect(message?.body).toMatchObject({message: "Investigate failures", source: {kind: "blank"}});
    expect(fixture.saveAttempts).toBe(0);
    await setup();
    await chooseSetupAction("Save fleet");
    await screen.findByRole("heading", {name: "Saved v1"});
    expect(fixture.saveAttempts).toBe(1);
    await waitFor(() => expect(chatRouter.push).toHaveBeenCalledWith(`/w/${WORKSPACE}/fleets/${FLEET_ID}`));
    expect(fixture.writes().filter((call) => call.path.endsWith("/fleet-libraries"))).toHaveLength(0);
    const save = fixture.writes().find((call) => call.path.endsWith("/fleets"));
    expect(save?.headers.get("Idempotency-Key")).toBeTruthy();
    expect(save?.body).toMatchObject({review_digest: "review-one", start: {kind: "none"}});
  });

  it("retries an uncertain Save with the original key and body", async () => {
    fixture.failSave = true;
    await converse();
    await setup();
    await chooseSetupAction("Save fleet");
    await screen.findAllByText("The request could not be confirmed. Retry the same operation.");
    fireEvent.click(screen.getByRole("button", {name: "Retry"}));
    await screen.findByRole("heading", {name: "Saved v1"});
    const attempts = fixture.writes().filter((call) => call.path.endsWith("/fleets"));
    expect(attempts).toHaveLength(2);
    expect(attempts[1]?.body).toEqual(attempts[0]?.body);
    expect(attempts[1]?.headers.get("Idempotency-Key")).toBe(attempts[0]?.headers.get("Idempotency-Key"));
  });

  it("publishes explicitly without creating a running fleet", async () => {
    await converse();
    await setup();
    await openSetupReview();
    const dialog = screen.getByRole("dialog", {name: "Edit instructions"});
    fireEvent.click(within(dialog).getByRole("button", {name: "Publish to Fleet library"}));
    fireEvent.click(await within(dialog).findByRole("button", {name: "Publish new entry"}));
    await waitFor(() => expect(fixture.writes().filter((call) => call.path.endsWith("/fleet-libraries"))).toHaveLength(1));
    expect(fixture.saveAttempts).toBe(0);
    expect(fixture.writes().find((call) => call.path.endsWith("/fleet-libraries"))?.body).toMatchObject({label: "repairer"});
  });

  it("shows proposed instructions and returns from the editor after an instructions update", async () => {
    await converse();
    await setup();
    await openSetupReview();
    const dialog = screen.getByRole("dialog", {name: "Edit instructions"});
    expect(within(dialog).getByRole("region", {name: "Setup instructions"}).textContent).toContain("Investigate failures.");
    expect(dialog.className).toContain("ph-no-capture"); expect(dialog.className).toContain("ph-no-record");

    const editor = await screen.findByRole("dialog", {name: "Edit instructions"});
    expect(editor.className).toContain("ph-no-capture"); expect(editor.className).toContain("ph-no-record");
    fireEvent.click(within(editor).getByRole("button", {name: "Update instructions"}));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(within(dialog).queryByLabelText("Runtime config")).toBeNull();
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
