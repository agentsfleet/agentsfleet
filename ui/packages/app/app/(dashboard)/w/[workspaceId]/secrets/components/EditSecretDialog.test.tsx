import React from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const {replace, create, refresh, analytics} = vi.hoisted(() => ({replace: vi.fn(), create: vi.fn(), refresh: vi.fn(), analytics: vi.fn()}));
vi.mock("next/navigation", () => ({useRouter: () => ({refresh})}));
vi.mock("../actions", () => ({replaceSecretAction: replace, createSecretAction: create}));
vi.mock("@/lib/analytics/posthog", () => ({captureProductEvent: analytics}));
import EditSecretDialog from "./EditSecretDialog";

const WORKSPACE = "ws_edit_test";
const NAME = "fly";
const TOKEN = "replacement-test-value";
const SAVE = "Save changes";
const INPUT_NAME = "Field 1 name";
const INPUT_VALUE = "Field 1 value";
function dialog(onOpenChange = vi.fn()) {
  return render(<EditSecretDialog workspaceId={WORKSPACE} name={NAME} open onOpenChange={onOpenChange} />);
}
function enter(key = "api_token", value = TOKEN) {
  fireEvent.change(screen.getByLabelText(INPUT_NAME), {target: {value: key}});
  fireEvent.change(screen.getByLabelText(INPUT_VALUE), {target: {value}});
}
const submit = () => fireEvent.click(screen.getByRole("button", {name: SAVE}));
beforeEach(() => vi.resetAllMocks());
afterEach(cleanup);

it("replaces through the existing update action with a locked name and masked field values", async () => {
  replace.mockResolvedValue({ok: true, data: {name: NAME}});
  const close = vi.fn(); dialog(close);
  expect(screen.getByText(/Values are encrypted on save/)).toBeTruthy();
  expect(screen.getByText("but never view them again.")).toBeTruthy();
  expect(screen.getByRole("link", {name: /Learn more/}).getAttribute("href")).toBe("https://docs.agentsfleet.net/fleets/secrets");
  expect(screen.getByLabelText("Secret name").getAttribute("readonly")).not.toBeNull();
  expect(screen.queryByRole("textbox", {name: /JSON/})).toBeNull();
  enter();
  fireEvent.click(screen.getByRole("button", {name: /Add field/}));
  fireEvent.change(screen.getByLabelText("Field 2 name"), {target: {value: "host"}});
  fireEvent.change(screen.getByLabelText("Field 2 value"), {target: {value: "api.fly.io"}});
  expect(screen.getByLabelText(INPUT_VALUE).getAttribute("type")).toBe("password");
  expect(document.body.textContent).not.toContain(TOKEN);
  submit();
  await waitFor(() => expect(replace).toHaveBeenCalledWith(WORKSPACE, NAME, {api_token: TOKEN, host: "api.fly.io"}));
  await waitFor(() => expect(close).toHaveBeenCalledWith(false));
  expect(refresh).toHaveBeenCalledTimes(1);
  expect(create).not.toHaveBeenCalled();
  expect(analytics).not.toHaveBeenCalled();
});

it("validates required values and duplicate field names without writing", async () => {
  dialog(); submit();
  expect(await screen.findByText("Field name is required")).toBeTruthy();
  expect(screen.getByText("Value is required")).toBeTruthy();
  enter(); fireEvent.click(screen.getByRole("button", {name: /Add field/}));
  fireEvent.change(screen.getByLabelText("Field 2 name"), {target: {value: "api_token"}});
  fireEvent.change(screen.getByLabelText("Field 2 value"), {target: {value: "another-value"}});
  submit();
  expect(await screen.findByText("Duplicate field name")).toBeTruthy();
  expect(replace).not.toHaveBeenCalled();
});

it("preserves a valid field name that also names an object prototype property", async () => {
  replace.mockResolvedValue({ok: true, data: {name: NAME}});
  dialog(); enter("__proto__"); submit();
  await waitFor(() => expect(replace).toHaveBeenCalledTimes(1));
  const data = replace.mock.calls[0]![2];
  expect(Object.hasOwn(data, "__proto__")).toBe(true);
  expect(data.__proto__).toBe(TOKEN);
});

it.each([403, 404, 409, 503])("keeps replacement fields for correction when saving is refused (%i)", async status => {
  replace.mockResolvedValue({ok: false, error: "Secret update refused", status});
  const close = vi.fn(); dialog(close); enter(); submit();
  expect(await screen.findByText(/Secret update refused/)).toBeTruthy();
  expect(screen.getByLabelText(INPUT_VALUE).getAttribute("type")).toBe("password");
  expect(screen.getByLabelText(INPUT_VALUE)).toHaveProperty("value", TOKEN);
  expect(close).not.toHaveBeenCalled(); expect(refresh).not.toHaveBeenCalled();
});

it("preserves input and allows retry after an uncertain save", async () => {
  replace.mockRejectedValueOnce(new Error("Network unavailable")).mockResolvedValueOnce({ok: true, data: {name: NAME}});
  dialog(); enter(); submit();
  expect(await screen.findByText(/Check Manage secrets before trying again/)).toBeTruthy();
  expect(screen.getByLabelText(INPUT_VALUE)).toHaveProperty("value", TOKEN);
  await waitFor(() => expect(screen.getByRole("button", {name: SAVE})).toHaveProperty("disabled", false));
  submit(); await waitFor(() => expect(replace).toHaveBeenCalledTimes(2));
});

it("blocks Escape and close while saving, then clears entered values on reopen", async () => {
  const pending = Promise.withResolvers<{ok: true; data: {name: string}}>();
  replace.mockReturnValue(pending.promise);
  const close = vi.fn(); const view = dialog(close); enter(); submit();
  await waitFor(() => expect(screen.getByRole("button", {name: SAVE})).toHaveProperty("disabled", true));
  fireEvent.click(screen.getByRole("button", {name: "Close"}));
  fireEvent.keyDown(screen.getByRole("dialog"), {key: "Escape"});
  expect(close).not.toHaveBeenCalled();
  await act(async () => pending.resolve({ok: true, data: {name: NAME}}));
  view.rerender(<EditSecretDialog workspaceId={WORKSPACE} name={NAME} open={false} onOpenChange={close} />);
  view.rerender(<EditSecretDialog workspaceId={WORKSPACE} name={NAME} open onOpenChange={close} />);
  expect(screen.getByLabelText(INPUT_VALUE)).toHaveProperty("value", "");
});

it("cancels without writing and discards replacement fields when reopened", () => {
  const close = vi.fn(); const view = dialog(close); enter();
  fireEvent.click(screen.getByRole("button", {name: "Cancel"}));
  expect(close).toHaveBeenCalledWith(false); expect(replace).not.toHaveBeenCalled();
  view.rerender(<EditSecretDialog workspaceId={WORKSPACE} name={NAME} open={false} onOpenChange={close} />);
  view.rerender(<EditSecretDialog workspaceId={WORKSPACE} name={NAME} open onOpenChange={close} />);
  expect(screen.getByLabelText(INPUT_VALUE)).toHaveProperty("value", "");
});
