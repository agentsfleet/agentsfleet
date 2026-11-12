import { describe, expect, it } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { listModelEntriesActionMock, setProviderSelfManagedActionMock, resetProviderActionMock, deleteModelEntryActionMock, entry, registry, renderTable } from "./helpers/models-registry-fixtures";
import {clickRowAction} from "./helpers/models-registry-edit-fixtures";

const activate = /make active: claude-sonnet-5/i;
const defaultAction = /make active: platform default/i;

describe("Models registry actions", () => {
  it("keeps the platform default above every returned model", async () => {
    await renderTable(registry(Array.from({length: 9}, (_, i) => entry({id: `e${i}`, model_id: `model-${i}`}))));
    expect(screen.getByLabelText("Platform default")).toBeTruthy();
    expect(screen.getAllByRole("listitem")).toHaveLength(9);
    expect(screen.queryByRole("searchbox")).toBeNull();
  });

  it("activates the row's workspace credential without requesting its bytes", async () => {
    setProviderSelfManagedActionMock.mockResolvedValue({ok: true, data: {mode: "self_managed"}});
    listModelEntriesActionMock.mockResolvedValue({ok: true, data: registry([entry({active: true})])});
    await renderTable(registry([entry({})]));
    await userEvent.setup().click(screen.getByRole("button", {name: activate}));
    expect(setProviderSelfManagedActionMock).toHaveBeenCalledWith({workspace_id: "ws_1", secret_ref: "anthropic-prod", model: "claude-sonnet-5"});
    expect(screen.queryByLabelText(/api key/i)).toBeNull();
    await waitFor(() => expect(within(screen.getByRole("listitem")).getByText("Active")).toBeTruthy());
  });

  it("disables an unconfigured platform default and never labels it active", async () => {
    await renderTable(registry([], false));
    const panel = within(screen.getByLabelText("Platform default"));
    expect(panel.getByText("No platform default is configured.")).toBeTruthy();
    expect(panel.queryByText("Active")).toBeNull();
    expect(panel.getByRole("button", {name: defaultAction})).toHaveProperty("disabled", true);
  });

  it("marks the configured platform default active when no custom model is active", async () => {
    await renderTable(registry([]));
    expect(within(screen.getByLabelText("Platform default")).getByText("Active")).toBeTruthy();
    expect(screen.queryByRole("button", {name: defaultAction})).toBeNull();
  });

  it("reports a refused activation and re-reads the registry", async () => {
    setProviderSelfManagedActionMock.mockResolvedValue({ok: false, error: "rejected", errorCode: "UZ-PROVIDER-003"});
    listModelEntriesActionMock.mockResolvedValue({ok: true, data: registry([entry({})])});
    await renderTable(registry([entry({})]));
    await userEvent.setup().click(screen.getByRole("button", {name: activate}));
    expect(await screen.findByText(/rejected/i)).toBeTruthy();
    expect(listModelEntriesActionMock).toHaveBeenCalled();
  });

  it.each([true, false])("re-reads after a platform default switch (accepted=%s)", async accepted => {
    resetProviderActionMock.mockResolvedValue(accepted ? {ok: true, data: {mode: "platform"}} : {ok: false, error: "refused"});
    listModelEntriesActionMock.mockResolvedValue({ok: true, data: registry(accepted ? [] : [entry({active: true})])});
    await renderTable(registry([entry({active: true})]));
    await userEvent.setup().click(screen.getByRole("button", {name: defaultAction}));
    expect(resetProviderActionMock).toHaveBeenCalledOnce();
    await waitFor(() => expect(listModelEntriesActionMock).toHaveBeenCalledOnce());
    if (!accepted) expect(await screen.findByText(/refused/)).toBeTruthy();
  });

  it("keeps the existing rows when the post-activation read fails", async () => {
    setProviderSelfManagedActionMock.mockResolvedValue({ok: true, data: {mode: "self_managed"}});
    listModelEntriesActionMock.mockResolvedValue({ok: false, error: "unavailable", status: 503});
    await renderTable(registry([entry({})]));
    await userEvent.setup().click(screen.getByRole("button", {name: activate}));
    expect(await screen.findByText("Models are temporarily unavailable. Your entries are safe.")).toBeTruthy();
    expect(screen.getByTitle("claude-sonnet-5")).toBeTruthy();
  });

  it("opens details through the model menu", async () => {
    await renderTable(registry([entry({})]));
    await clickRowAction(userEvent.setup(), "claude-sonnet-5", "Model details");
    expect(within(await screen.findByRole("dialog")).getByRole("heading", {name: "Sonnet 5"})).toBeTruthy();
  });

  it("keeps removal disabled for the active model", async () => {
    await renderTable(registry([entry({active: true})]));
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", {name: "Manage claude-sonnet-5"}));
    const remove = await screen.findByRole("menuitem", {name: "Remove model"});
    expect(remove.getAttribute("aria-disabled")).toBe("true");
    await user.click(remove);
    expect(deleteModelEntryActionMock).not.toHaveBeenCalled();
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });
});
