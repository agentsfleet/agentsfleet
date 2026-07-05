import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const { createCredentialActionMock, deleteCredentialActionMock, routerRefresh } = vi.hoisted(() => ({
  createCredentialActionMock: vi.fn(),
  deleteCredentialActionMock: vi.fn(),
  routerRefresh: vi.fn(),
}));

vi.mock("next/navigation", () => ({ useRouter: () => ({ refresh: routerRefresh }) }));
vi.mock("@/app/(dashboard)/secrets/actions", () => ({
  createSecretAction: createCredentialActionMock,
  deleteSecretAction: deleteCredentialActionMock,
}));

import RenameSecretDialog from "./RenameSecretDialog";

const WORKSPACE_ID = "ws_rename_test";

function renderDialog(onOpenChange = vi.fn()) {
  return render(
    React.createElement(RenameSecretDialog, {
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

function enterNewName(value: string) {
  fireEvent.change(screen.getByLabelText(/new name/i), { target: { value } });
}

beforeEach(() => {
  createCredentialActionMock.mockReset();
  deleteCredentialActionMock.mockReset();
  routerRefresh.mockReset();
});
afterEach(() => cleanup());

describe("RenameSecretDialog", () => {
  it("always shows the generic (no fleet-name) rename warning", () => {
    renderDialog();
    expect(screen.getByText(/renaming breaks fleets that reference this secret/i)).toBeTruthy();
    // Generic by design — no specific fleet is ever named.
    expect(screen.queryByText(/\$\{secrets\./i)).toBeNull();
  });

  it("creates the new name before deleting the old, then closes and refreshes", async () => {
    createCredentialActionMock.mockResolvedValue({ ok: true, data: { name: "fly-prod" } });
    deleteCredentialActionMock.mockResolvedValue({ ok: true, data: undefined });
    const onOpenChange = vi.fn();
    renderDialog(onOpenChange);

    enterNewName("fly-prod");
    enterData('{"api_token": "FLY"}');
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
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    expect(routerRefresh).toHaveBeenCalled();
  });

  it("rename to the same name is rejected (use Edit to rotate instead)", () => {
    renderDialog();
    enterData('{"api_token": "FLY"}');
    enterNewName("fly");
    fireEvent.click(screen.getByRole("button", { name: /^rename$/i }));
    expect(screen.getByText(/new name matches the current name/i)).toBeTruthy();
    expect(createCredentialActionMock).not.toHaveBeenCalled();
  });

  it("rename with an empty new name is rejected with a length message", () => {
    renderDialog();
    enterData('{"api_token": "FLY"}');
    // Leave the new-name field blank.
    fireEvent.click(screen.getByRole("button", { name: /^rename$/i }));
    expect(screen.getByText(/new name must be 1.?64 characters/i)).toBeTruthy();
    expect(createCredentialActionMock).not.toHaveBeenCalled();
  });

  it("rename with an over-long new name is rejected with a length message", () => {
    renderDialog();
    enterData('{"api_token": "FLY"}');
    enterNewName("a".repeat(65));
    fireEvent.click(screen.getByRole("button", { name: /^rename$/i }));
    expect(screen.getByText(/new name must be 1.?64 characters/i)).toBeTruthy();
    expect(createCredentialActionMock).not.toHaveBeenCalled();
  });

  it("rejects non-object / unparseable data before validating the name or calling the API", () => {
    renderDialog();
    enterData('"just a string"');
    enterNewName("fly-prod");
    fireEvent.click(screen.getByRole("button", { name: /^rename$/i }));
    expect(screen.getByText(/must be a json object/i)).toBeTruthy();
    expect(createCredentialActionMock).not.toHaveBeenCalled();
  });

  it("Cancel closes the dialog and resets without calling the API", () => {
    const onOpenChange = vi.fn();
    renderDialog(onOpenChange);
    enterData('{"api_token": "FLY"}');
    enterNewName("fly-prod");
    fireEvent.click(screen.getByRole("button", { name: /^cancel$/i }));
    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(createCredentialActionMock).not.toHaveBeenCalled();
  });

  it("surfaces a create failure and never deletes the old name", async () => {
    createCredentialActionMock.mockResolvedValue({
      ok: false,
      error: "name already taken",
      errorCode: "UZ-CRED-409",
      status: 409,
    });
    const onOpenChange = vi.fn();
    renderDialog(onOpenChange);
    enterData('{"api_token": "FLY"}');
    enterNewName("fly-prod");
    fireEvent.click(screen.getByRole("button", { name: /^rename$/i }));

    await waitFor(() => expect(createCredentialActionMock).toHaveBeenCalledTimes(1));
    // create failed → the old name is never deleted.
    expect(deleteCredentialActionMock).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.getByText(/name already taken/i)).toBeTruthy());
    expect(routerRefresh).not.toHaveBeenCalled();
    expect(onOpenChange).not.toHaveBeenCalledWith(false);
  });

  it("surfaces a delete failure (new name stored, old kept) and refreshes to show both", async () => {
    createCredentialActionMock.mockResolvedValue({ ok: true, data: { name: "fly-prod" } });
    deleteCredentialActionMock.mockResolvedValue({
      ok: false,
      error: "delete failed",
      status: 500,
    });
    renderDialog();
    enterData('{"api_token": "FLY"}');
    enterNewName("fly-prod");
    fireEvent.click(screen.getByRole("button", { name: /^rename$/i }));

    await waitFor(() => expect(deleteCredentialActionMock).toHaveBeenCalledTimes(1));
    // The new name was created, so the list is refreshed to surface it (and the
    // still-present old name), and the dialog stays open with a recovery message.
    await waitFor(() => expect(routerRefresh).toHaveBeenCalled());
    expect(screen.getByText(/delete failed/i)).toBeTruthy();
  });

  it("blocks dialog dismissal while a rename save is in flight", async () => {
    const onOpenChange = vi.fn();
    // A deferred create keeps the transition pending until we resolve it, so
    // `pending` is true when we attempt to dismiss; resolving at the end lets
    // the transition settle instead of leaking into later tests.
    let resolveSave!: (v: { ok: true; data: { name: string } }) => void;
    createCredentialActionMock.mockReturnValue(
      new Promise<{ ok: true; data: { name: string } }>((r) => {
        resolveSave = r;
      }),
    );
    deleteCredentialActionMock.mockResolvedValue({ ok: true, data: undefined });
    renderDialog(onOpenChange);

    enterData('{"api_token": "FLY"}');
    enterNewName("fly-prod");
    fireEvent.click(screen.getByRole("button", { name: /^rename$/i }));
    await waitFor(() => {
      expect(document.querySelector('button[aria-busy="true"]')).not.toBeNull();
    });
    // The dialog's Close affordance fires onOpenChange(false); handleOpenChange's
    // `if (pending) return` blocks propagation so the parent close handler never
    // fires mid-save.
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onOpenChange).not.toHaveBeenCalledWith(false);
    // Settle the in-flight save so the transition doesn't leak.
    await act(async () => {
      resolveSave({ ok: true, data: { name: "fly-prod" } });
    });
  });
});
