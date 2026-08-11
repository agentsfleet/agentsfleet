import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const { replaceSecretActionMock, createSecretActionMock, routerRefresh } = vi.hoisted(() => ({
  replaceSecretActionMock: vi.fn(),
  createSecretActionMock: vi.fn(),
  routerRefresh: vi.fn(),
}));

vi.mock("next/navigation", () => ({ useRouter: () => ({ refresh: routerRefresh }) }));
vi.mock("@/app/(dashboard)/w/[workspaceId]/secrets/actions", () => ({
  createSecretAction: createSecretActionMock,
  replaceSecretAction: replaceSecretActionMock,
}));

import EditSecretDialog from "./EditSecretDialog";

const WORKSPACE_ID = "ws_edit_test";

function renderDialog(onOpenChange = vi.fn()) {
  return render(
    React.createElement(EditSecretDialog, {
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
  replaceSecretActionMock.mockReset();
  createSecretActionMock.mockReset();
  routerRefresh.mockReset();
});
afterEach(() => cleanup());

// EditSecretDialog is rotate-only. Renaming lives in RenameSecretDialog (its own
// test file); this dialog never deletes and never mints a new name.
describe("EditSecretDialog (rotate-only)", () => {
  it("rotate: replaces the named secret via PUT (never create) and refreshes", async () => {
    replaceSecretActionMock.mockResolvedValue({ ok: true, data: { name: "fly" } });
    const onOpenChange = vi.fn();
    renderDialog(onOpenChange);

    enterData('{"api_token": "FLY_NEW"}');
    fireEvent.click(screen.getByRole("button", { name: /^rotate$/i }));

    await waitFor(() => expect(replaceSecretActionMock).toHaveBeenCalledTimes(1));
    expect(replaceSecretActionMock).toHaveBeenCalledWith(WORKSPACE_ID, "fly", {
      api_token: "FLY_NEW",
    });
    // Creation claims a free name and 409s on a held one — rotating an
    // existing secret must never route through it (the regression that kept
    // this dialog answering "name already exists").
    expect(createSecretActionMock).not.toHaveBeenCalled();
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    expect(routerRefresh).toHaveBeenCalled();
  });

  it("has no rename affordance — one job", () => {
    renderDialog();
    expect(screen.queryByRole("button", { name: /rename/i })).toBeNull();
    expect(screen.queryByLabelText(/new name/i)).toBeNull();
    expect(screen.getByRole("button", { name: /^rotate$/i })).toBeTruthy();
  });

  it("Cancel closes the dialog and resets without calling the API", () => {
    const onOpenChange = vi.fn();
    renderDialog(onOpenChange);
    enterData('{"api_token": "FLY"}');
    fireEvent.click(screen.getByRole("button", { name: /^cancel$/i }));
    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(replaceSecretActionMock).not.toHaveBeenCalled();
  });

  it("rejects non-object / unparseable data before calling the API", () => {
    renderDialog();
    enterData('"just a string"');
    fireEvent.click(screen.getByRole("button", { name: /^rotate$/i }));
    expect(screen.getByText(/must be a json object/i)).toBeTruthy();
    expect(replaceSecretActionMock).not.toHaveBeenCalled();
  });

  it("surfaces a replace error's friendly copy and does not refresh or close", async () => {
    // A real server action's ActionResult.error is ApiError.message, which
    // client.ts resolves as user_message ?? detail ?? title — the mock stands
    // in for that resolved value. The replace path's own failure (a rotated
    // name that no longer exists) renders in place; the dialog stays open.
    replaceSecretActionMock.mockResolvedValue({
      ok: false,
      error: "That secret doesn't exist anymore. Refresh the list and try again.",
      errorCode: "UZ-VAULT-004",
      status: 404,
    });
    const onOpenChange = vi.fn();
    renderDialog(onOpenChange);

    enterData('{"api_token": "FLY"}');
    fireEvent.click(screen.getByRole("button", { name: /^rotate$/i }));

    await waitFor(() => expect(screen.getAllByText(/doesn't exist anymore/i).length).toBeGreaterThan(0));
    expect(routerRefresh).not.toHaveBeenCalled();
    expect(onOpenChange).not.toHaveBeenCalledWith(false);
  });
});
