import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const { createCredentialActionMock, deleteCredentialActionMock, routerRefresh } = vi.hoisted(() => ({
  createCredentialActionMock: vi.fn(),
  deleteCredentialActionMock: vi.fn(),
  routerRefresh: vi.fn(),
}));

vi.mock("next/navigation", () => ({ useRouter: () => ({ refresh: routerRefresh }) }));
vi.mock("@/app/(dashboard)/credentials/actions", () => ({
  createCredentialAction: createCredentialActionMock,
  deleteCredentialAction: deleteCredentialActionMock,
}));

import EditCredentialDialog from "./EditCredentialDialog";

const WORKSPACE_ID = "ws_edit_test";

function renderDialog(onOpenChange = vi.fn()) {
  return render(
    React.createElement(EditCredentialDialog, {
      workspaceId: WORKSPACE_ID,
      name: "fly",
      open: true,
      onOpenChange,
    }),
  );
}

function enterData(json: string) {
  fireEvent.change(screen.getByLabelText(/data \(json object\)/i), { target: { value: json } });
}

beforeEach(() => {
  createCredentialActionMock.mockReset();
  deleteCredentialActionMock.mockReset();
  routerRefresh.mockReset();
});
afterEach(() => cleanup());

describe("EditCredentialDialog", () => {
  it("rotate: upserts under the same name and never deletes", async () => {
    createCredentialActionMock.mockResolvedValue({ ok: true, data: { name: "fly" } });
    const onOpenChange = vi.fn();
    renderDialog(onOpenChange);

    enterData('{"api_token": "FLY_NEW"}');
    fireEvent.click(screen.getByRole("button", { name: /^rotate$/i }));

    await waitFor(() => expect(createCredentialActionMock).toHaveBeenCalledTimes(1));
    expect(createCredentialActionMock).toHaveBeenCalledWith(WORKSPACE_ID, {
      name: "fly",
      data: { api_token: "FLY_NEW" },
    });
    expect(deleteCredentialActionMock).not.toHaveBeenCalled();
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    expect(routerRefresh).toHaveBeenCalled();
  });

  it("rename: warns, then creates the new name before deleting the old", async () => {
    createCredentialActionMock.mockResolvedValue({ ok: true, data: { name: "fly-prod" } });
    deleteCredentialActionMock.mockResolvedValue({ ok: true, data: undefined });
    renderDialog();

    fireEvent.click(screen.getByRole("button", { name: /advanced — rename/i }));
    expect(screen.getByText(/renaming breaks agents/i)).toBeTruthy();

    enterData('{"api_token": "FLY"}');
    fireEvent.change(screen.getByLabelText(/new name/i), { target: { value: "fly-prod" } });
    fireEvent.click(screen.getByRole("button", { name: /^rename$/i }));

    await waitFor(() => expect(deleteCredentialActionMock).toHaveBeenCalledTimes(1));
    expect(createCredentialActionMock).toHaveBeenCalledWith(WORKSPACE_ID, {
      name: "fly-prod",
      data: { api_token: "FLY" },
    });
    expect(deleteCredentialActionMock).toHaveBeenCalledWith(WORKSPACE_ID, "fly");
    // Create must precede delete so a failure never strands both names.
    const createOrder = createCredentialActionMock.mock.invocationCallOrder[0] ?? Infinity;
    const deleteOrder = deleteCredentialActionMock.mock.invocationCallOrder[0] ?? -Infinity;
    expect(createOrder).toBeLessThan(deleteOrder);
  });

  it("rename to the same name is rejected (use rotate instead)", () => {
    renderDialog();
    fireEvent.click(screen.getByRole("button", { name: /advanced — rename/i }));
    enterData('{"api_token": "FLY"}');
    fireEvent.change(screen.getByLabelText(/new name/i), { target: { value: "fly" } });
    fireEvent.click(screen.getByRole("button", { name: /^rename$/i }));
    expect(screen.getByText(/new name matches the current name/i)).toBeTruthy();
    expect(createCredentialActionMock).not.toHaveBeenCalled();
  });

  it("rename with an empty new name is rejected with a length message", () => {
    renderDialog();
    fireEvent.click(screen.getByRole("button", { name: /advanced — rename/i }));
    enterData('{"api_token": "FLY"}');
    // Leave the new-name field blank.
    fireEvent.click(screen.getByRole("button", { name: /^rename$/i }));
    expect(screen.getByText(/new name must be 1.?64 characters/i)).toBeTruthy();
    expect(createCredentialActionMock).not.toHaveBeenCalled();
  });

  it("Cancel closes the dialog and resets without calling the API", () => {
    const onOpenChange = vi.fn();
    renderDialog(onOpenChange);
    enterData('{"api_token": "FLY"}');
    fireEvent.click(screen.getByRole("button", { name: /^cancel$/i }));
    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(createCredentialActionMock).not.toHaveBeenCalled();
  });

  it("rejects non-object / unparseable data before calling the API", () => {
    renderDialog();
    enterData('"just a string"');
    fireEvent.click(screen.getByRole("button", { name: /^rotate$/i }));
    expect(screen.getByText(/must be a json object/i)).toBeTruthy();
    expect(createCredentialActionMock).not.toHaveBeenCalled();
  });

  it("surfaces a create error and does not refresh or close", async () => {
    createCredentialActionMock.mockResolvedValue({
      ok: false,
      error: "credential store unavailable",
      errorCode: "UZ-VAULT-001",
      status: 503,
    });
    const onOpenChange = vi.fn();
    renderDialog(onOpenChange);

    enterData('{"api_token": "FLY"}');
    fireEvent.click(screen.getByRole("button", { name: /^rotate$/i }));

    await waitFor(() => expect(screen.getAllByText(/credential|unavailable/i).length).toBeGreaterThan(0));
    expect(routerRefresh).not.toHaveBeenCalled();
    expect(onOpenChange).not.toHaveBeenCalledWith(false);
  });

  it("surfaces a create failure during rename and never deletes the old name", async () => {
    createCredentialActionMock.mockResolvedValue({
      ok: false,
      error: "name already taken",
      errorCode: "UZ-CRED-409",
      status: 409,
    });
    renderDialog();
    fireEvent.click(screen.getByRole("button", { name: /advanced — rename/i }));
    enterData('{"api_token": "FLY"}');
    fireEvent.change(screen.getByLabelText(/new name/i), { target: { value: "fly-prod" } });
    fireEvent.click(screen.getByRole("button", { name: /^rename$/i }));

    await waitFor(() => expect(createCredentialActionMock).toHaveBeenCalledTimes(1));
    // create failed → the old name is never deleted.
    expect(deleteCredentialActionMock).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.getByText(/name already taken/i)).toBeTruthy());
  });

  it("surfaces a delete failure during rename (new name stored, old kept)", async () => {
    createCredentialActionMock.mockResolvedValue({ ok: true, data: { name: "fly-prod" } });
    deleteCredentialActionMock.mockResolvedValue({
      ok: false,
      error: "delete failed",
      status: 500,
    });
    renderDialog();
    fireEvent.click(screen.getByRole("button", { name: /advanced — rename/i }));
    enterData('{"api_token": "FLY"}');
    fireEvent.change(screen.getByLabelText(/new name/i), { target: { value: "fly-prod" } });
    fireEvent.click(screen.getByRole("button", { name: /^rename$/i }));

    await waitFor(() => expect(deleteCredentialActionMock).toHaveBeenCalledTimes(1));
    // The new name was created, so the list is refreshed to surface it (and the
    // still-present old name), and the dialog stays open with a recovery message.
    await waitFor(() => expect(routerRefresh).toHaveBeenCalled());
    expect(screen.getByText(/delete failed/i)).toBeTruthy();
  });
});
