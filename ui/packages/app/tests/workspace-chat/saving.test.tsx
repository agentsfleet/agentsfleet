import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { ChatFixture, CHAT_PROPS } from "./fixture";
import { WorkspaceChat } from "@/components/domain/chat/WorkspaceChat";
import { CHAT_EXAMPLES, CHAT_PLACEHOLDER } from "@/components/domain/chat/chat-defaults";
import { WORKSPACE } from "@/tests/helpers/draft-fixtures";

let fixture: ChatFixture;
beforeEach(() => { fixture = new ChatFixture(); fixture.mount(); });
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

async function converse(): Promise<void> {
  render(<WorkspaceChat {...CHAT_PROPS} />);
  const input = await screen.findByLabelText(CHAT_PLACEHOLDER);
  await waitFor(() => expect(screen.getByRole("button", {name: "Send message"})).toHaveProperty("disabled", false));
  fireEvent.change(input, {target: {value: "Investigate failures"}});
  fireEvent.click(screen.getByRole("button", {name: "Send message"}));
  await screen.findByText("Your setup is ready.");
}

async function setup() {
  const region = await screen.findByRole("region", {name: "Fleet setup"});
  await within(region).findByRole("button", {name: "Save fleet"});
  expect(screen.queryByRole("button", {name: "Review setup"})).toBeNull();
  expect(screen.queryByRole("dialog")).toBeNull();
  return region;
}

describe("private chat Save", () => {
  it("lets the user edit a suggested job before sending it", async () => {
    render(<WorkspaceChat {...CHAT_PROPS} />);
    const suggestion = await screen.findByRole("button", {name: /Fix a build/});
    await waitFor(() => expect(suggestion).toHaveProperty("disabled", false));
    fireEvent.click(suggestion);
    expect(screen.getByLabelText(CHAT_PLACEHOLDER)).toHaveProperty("value", CHAT_EXAMPLES[0]);
    expect(fixture.writes()).toHaveLength(0);
    fireEvent.click(screen.getByRole("button", {name: "Send message"}));
    await screen.findByText("Your setup is ready.");
    expect(fixture.writes()).toHaveLength(1);
    expect(fixture.writes()[0]?.body).toMatchObject({message: CHAT_EXAMPLES[0]});
  });
  it("sends ordinary text and saves directly from the setup summary without publishing", async () => {
    await converse();
    const message = fixture.writes()[0];
    expect(message?.body).toMatchObject({message: "Investigate failures", source: {kind: "blank"}});
    expect(fixture.saveAttempts).toBe(0);
    const dialog = await setup();
    fireEvent.click(within(dialog).getByRole("button", {name: "Save fleet"}));
    await screen.findAllByText("repairer is ready");
    expect(fixture.saveAttempts).toBe(1);
    expect(fixture.writes().filter((call) => call.path.endsWith("/fleet-libraries"))).toHaveLength(0);
    const save = fixture.writes().find((call) => call.path.endsWith("/fleets"));
    expect(save?.headers.get("Idempotency-Key")).toBeTruthy();
    expect(save?.body).toMatchObject({review_digest: "review-one", start: {kind: "none"}});
  });

  it("retries an uncertain Save with the original key and body", async () => {
    fixture.failSave = true;
    await converse();
    const dialog = await setup();
    fireEvent.click(within(dialog).getByRole("button", {name: "Save fleet"}));
    await screen.findAllByText("The request could not be confirmed. Retry the same operation.");
    fireEvent.click(screen.getByRole("button", {name: "Retry"}));
    await screen.findAllByText("repairer is ready");
    const attempts = fixture.writes().filter((call) => call.path.endsWith("/fleets"));
    expect(attempts).toHaveLength(2);
    expect(attempts[1]?.body).toEqual(attempts[0]?.body);
    expect(attempts[1]?.headers.get("Idempotency-Key")).toBe(attempts[0]?.headers.get("Idempotency-Key"));
  });

  it("publishes explicitly without creating a running fleet", async () => {
    await converse();
    const dialog = await setup();
    fireEvent.click(within(dialog).getByRole("button", {name: "Publish to Fleet library"}));
    fireEvent.click(await within(dialog).findByRole("button", {name: "Publish new entry"}));
    await waitFor(() => expect(fixture.writes().filter((call) => call.path.endsWith("/fleet-libraries"))).toHaveLength(1));
    expect(fixture.saveAttempts).toBe(0);
    expect(fixture.writes().find((call) => call.path.endsWith("/fleet-libraries"))?.body).toMatchObject({label: "repairer"});
  });

  it("shows proposed instructions and returns from the editor after an instructions update", async () => {
    await converse();
    const dialog = await setup();
    expect(within(dialog).getByRole("region", {name: "Setup instructions"}).textContent).toContain("Investigate failures.");
    expect(dialog.className).toContain("ph-no-capture"); expect(dialog.className).toContain("ph-no-record");

    fireEvent.click(within(dialog).getByRole("button", {name: "Edit instructions"}));
    fireEvent.click(within(dialog).getByRole("button", {name: "Update instructions"}));
    await within(dialog).findByRole("button", {name: "Save fleet"});
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
