import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { expect, it } from "vitest";
import {
  createSecretActionMock, createModelEntryActionMock, setProviderSelfManagedActionMock,
  findRegisteredModelActionMock, readModelActivationActionMock, fillKnownForm, renderDialog, FORM_MODEL, modelIdentity, ANTHROPIC_SECRET,
} from "./helpers/models-registry-add-fixtures";
import { SECRETS_LOAD } from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/secrets-load";

const CREDENTIAL = {name: "anthropic", key: "sample-provider-key"};
async function start(activate = false) {
  const fixture = await renderDialog();
  const dialog = screen.getByRole("dialog");
  await fillKnownForm(fixture.user, dialog, CREDENTIAL);
  if (activate) await fixture.user.click(within(dialog).getByRole("switch", {name: "Make active"}));
  await fixture.user.click(within(dialog).getByRole("button", {name: "Create model"}));
  return {...fixture, dialog: within(dialog)};
}

it("reconciles a lost registration response without another secret or model write", async () => {
  createModelEntryActionMock.mockResolvedValueOnce({ok: false, error: "Response lost"});
  findRegisteredModelActionMock.mockResolvedValueOnce({ok: true, data: {
    id: "entry", model_id: FORM_MODEL, secret_ref: CREDENTIAL.name, active: false,
    provider: "anthropic", kind: "provider_key", has_key: true, created_at: "2026-04-30T00:00:00Z",
  }});
  const {user, dialog} = await start();
  await user.click(await dialog.findByRole("button", {name: "Check status"}));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(findRegisteredModelActionMock).toHaveBeenCalledWith({workspace_id: "ws_1", model_id: FORM_MODEL, secret_ref: CREDENTIAL.name});
  expect(createSecretActionMock).toHaveBeenCalledTimes(1);
  expect(createModelEntryActionMock).toHaveBeenCalledTimes(1);
});

it("offers an explicit registration retry only after a complete read found no entry", async () => {
  createModelEntryActionMock.mockResolvedValueOnce({ok: false, error: "Response lost"});
  const {user, dialog} = await start();
  await user.click(await dialog.findByRole("button", {name: "Check status"}));
  const retry = await dialog.findByRole("button", {name: "Retry registration"});
  expect(createModelEntryActionMock).toHaveBeenCalledTimes(1);
  await user.click(retry);
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(createSecretActionMock).toHaveBeenCalledTimes(1);
  expect(createModelEntryActionMock).toHaveBeenCalledTimes(2);
});

it("confirms a lost activation acknowledgement without repeating activation", async () => {
  setProviderSelfManagedActionMock.mockResolvedValueOnce({ok: false, error: "Response lost"});
  readModelActivationActionMock.mockResolvedValueOnce({ok: true, data: true});
  const {user, dialog, onCreated} = await start(true);
  expect(onCreated).toHaveBeenCalledTimes(1);
  await user.click(await dialog.findByRole("button", {name: "Check status"}));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(setProviderSelfManagedActionMock).toHaveBeenCalledTimes(1);
  expect(createModelEntryActionMock).toHaveBeenCalledTimes(1);
});

it("keeps a failed reconciliation unconfirmed and performs no additional writes", async () => {
  createModelEntryActionMock.mockResolvedValueOnce({ok: false, error: "Response lost"});
  findRegisteredModelActionMock.mockResolvedValueOnce({ok: false, error: "Unavailable", status: 503});
  const {user, dialog} = await start();
  await user.click(await dialog.findByRole("button", {name: "Check status"}));
  await waitFor(() => expect(dialog.getByRole("alert").textContent).toContain("Unavailable"));
  expect(dialog.getByRole("button", {name: "Check status"})).toBeTruthy();
  expect(createSecretActionMock).toHaveBeenCalledTimes(1);
  expect(createModelEntryActionMock).toHaveBeenCalledTimes(1);
});

it("excludes duplicate clicks and stops the sequence when the form unmounts", async () => {
  let complete!: (value: {ok: true; data: {name: string}}) => void;
  createSecretActionMock.mockReturnValueOnce(new Promise(resolve => { complete = resolve; }));
  const fixture = await renderDialog();
  const dialog = screen.getByRole("dialog");
  await fillKnownForm(fixture.user, dialog, CREDENTIAL);
  await fixture.user.dblClick(within(dialog).getByRole("button", {name: "Create model"}));
  expect(createSecretActionMock).toHaveBeenCalledTimes(1);
  fixture.unmount();
  await act(async () => { complete({ok: true, data: {name: CREDENTIAL.name}}); });
  expect(createModelEntryActionMock).not.toHaveBeenCalled();
  expect(setProviderSelfManagedActionMock).not.toHaveBeenCalled();
});

it.each(["workspace", "account", "sign-out"])("clears the key and closes the form after a %s change", async scope => {
  const fixture = await renderDialog();
  await fillKnownForm(fixture.user, screen.getByRole("dialog"), CREDENTIAL);
  if (scope !== "workspace") modelIdentity.userId = scope === "account" ? "another-person" : null;
  fixture.rerender(scope === "workspace" ? {workspaceId: "ws_2"} : {});
  expect(screen.queryByRole("dialog")).toBeNull();
  if (scope === "sign-out") {
    expect(screen.getByRole("button", {name: "Create model"}).hasAttribute("disabled")).toBe(true);
    await fixture.user.click(screen.getByRole("button", {name: "Create model"}));
    expect(screen.queryByRole("dialog")).toBeNull();
    Object.assign(modelIdentity, {isLoaded: true, isSignedIn: true, userId: "model-owner"});
    fixture.rerender();
  }
  await fixture.user.click(screen.getByRole("button", {name: "Create model"}));
  expect((screen.getByLabelText(/^api key$/i) as HTMLInputElement).value).toBe("");
  expect(createSecretActionMock).not.toHaveBeenCalled();
});

it("keeps an in-flight write open but abandons its continuation when the workspace changes", async () => {
  const pending = Promise.withResolvers<{ok: true; data: {name: string}}>();
  createSecretActionMock.mockReturnValueOnce(pending.promise);
  const fixture = await renderDialog();
  await fillKnownForm(fixture.user, screen.getByRole("dialog"), CREDENTIAL);
  await fixture.user.click(within(screen.getByRole("dialog")).getByRole("button", {name: "Create model"}));
  await fixture.user.keyboard("{Escape}");
  await fixture.user.click(screen.getByRole("button", {name: "Close"}));
  const overlay = document.querySelector(".bg-overlay");
  if (!overlay) throw new Error("The model dialog must have its native overlay");
  await fixture.user.click(overlay);
  expect(screen.getByRole("dialog")).toBeTruthy();
  fixture.rerender({workspaceId: "ws_2"});
  expect(screen.queryByRole("dialog")).toBeNull();
  await act(async () => {pending.resolve({ok: true, data: {name: CREDENTIAL.name}});});
  expect(createModelEntryActionMock).not.toHaveBeenCalled();
  expect(fixture.onCreated).not.toHaveBeenCalled();
});

it("dismisses an editing model form with Escape and refuses an empty native form submission", async () => {
  const fixture = await renderDialog();
  const form = screen.getByRole("dialog").querySelector("form");
  if (!form) throw new Error("The model dialog must render its native form");
  await act(async () => {fireEvent.submit(form);});
  expect(createSecretActionMock).not.toHaveBeenCalled(); expect(createModelEntryActionMock).not.toHaveBeenCalled();
  await fixture.user.keyboard("{Escape}");
  expect(screen.queryByRole("dialog")).toBeNull();
});

it("requires both identity and saved-key readiness before rendering credential inputs", async () => {
  const fixture = await renderDialog([], SECRETS_LOAD.loading);
  expect(screen.getByText("Loading model credentials…")).toBeTruthy();
  expect(screen.queryByLabelText(/^api key$/i)).toBeNull();
  fixture.rerender({secretsLoad: SECRETS_LOAD.error});
  await fixture.user.click(screen.getByRole("button", {name: "Try again"}));
  expect(fixture.onSecretsNeeded).toHaveBeenCalledTimes(2);
  modelIdentity.isLoaded = false;
  fixture.rerender({secretsLoad: SECRETS_LOAD.ready});
  expect(screen.getByText("Loading model credentials…")).toBeTruthy();
  modelIdentity.isLoaded = true;
  fixture.rerender({secretsLoad: SECRETS_LOAD.ready});
  expect(screen.getByLabelText(/^api key$/i)).toBeTruthy();
  await fixture.user.click(screen.getByRole("button", {name: "Cancel"}));
  expect(screen.queryByRole("dialog")).toBeNull();
});

it("offers saved-key selection after an uncertain storage response without replaying storage", async () => {
  createSecretActionMock.mockResolvedValueOnce({ok: false, error: "Response lost"});
  const fixture = await renderDialog([ANTHROPIC_SECRET]); const dialog = within(screen.getByRole("dialog"));
  await fillKnownForm(fixture.user, screen.getByRole("dialog"), {name: "new-provider-key", key: CREDENTIAL.key});
  await fixture.user.click(dialog.getByRole("button", {name: "Create model"}));
  await fixture.user.click(await dialog.findByRole("button", {name: "Choose credential"}));
  expect(await dialog.findByLabelText("Saved key", {exact: true})).toBeTruthy();
  const overlay = document.querySelector(".bg-overlay");
  if (!overlay) throw new Error("The model dialog must have its native overlay");
  await fixture.user.click(overlay);
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(createSecretActionMock).toHaveBeenCalledOnce(); expect(createModelEntryActionMock).not.toHaveBeenCalled();
});
