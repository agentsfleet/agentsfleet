import { act, screen, waitFor, within } from "@testing-library/react";
import { expect, it } from "vitest";
import {
  createSecretActionMock, createModelEntryActionMock, setProviderSelfManagedActionMock,
  findRegisteredModelActionMock, readModelActivationActionMock, fillKnownForm, renderDialog, FORM_MODEL,
} from "./helpers/models-registry-add-fixtures";

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
