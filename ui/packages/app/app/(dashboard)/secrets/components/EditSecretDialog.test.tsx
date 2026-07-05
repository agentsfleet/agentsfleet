import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const { createCredentialActionMock, routerRefresh } = vi.hoisted(() => ({
  createCredentialActionMock: vi.fn(),
  routerRefresh: vi.fn(),
}));

vi.mock("next/navigation", () => ({ useRouter: () => ({ refresh: routerRefresh }) }));
vi.mock("@/app/(dashboard)/secrets/actions", () => ({
  createSecretAction: createCredentialActionMock,
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
  createCredentialActionMock.mockReset();
  routerRefresh.mockReset();
});
afterEach(() => cleanup());

// EditSecretDialog is rotate-only. Renaming lives in RenameSecretDialog (its own
// test file); this dialog never deletes and never mints a new name.
describe("EditSecretDialog (rotate-only)", () => {
  it("rotate: upserts under the same name and refreshes", async () => {
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
    expect(createCredentialActionMock).not.toHaveBeenCalled();
  });

  it("rejects non-object / unparseable data before calling the API", () => {
    renderDialog();
    enterData('"just a string"');
    fireEvent.click(screen.getByRole("button", { name: /^rotate$/i }));
    expect(screen.getByText(/must be a json object/i)).toBeTruthy();
    expect(createCredentialActionMock).not.toHaveBeenCalled();
  });

  it("surfaces a create error's friendly copy (UZ-VAULT-001's user_message, curated server-side, error_entries.zig) and does not refresh or close", async () => {
    // A real server action's ActionResult.error is ApiError.message, which
    // client.ts now resolves as user_message ?? detail ?? title — the mock
    // stands in for that resolved value (the backend registry migration;
    // UZ-VAULT-001 is no longer curated in frontend CODE_MAP).
    createCredentialActionMock.mockResolvedValue({
      ok: false,
      error: "That secret needs at least one field. Enter it as a JSON object with one or more keys — not a bare string or list.",
      errorCode: "UZ-VAULT-001",
      status: 400,
    });
    const onOpenChange = vi.fn();
    renderDialog(onOpenChange);

    enterData('{"api_token": "FLY"}');
    fireEvent.click(screen.getByRole("button", { name: /^rotate$/i }));

    await waitFor(() => expect(screen.getAllByText(/needs at least one field/i).length).toBeGreaterThan(0));
    // The raw backend detail never reaches the DOM — only the curated copy does.
    expect(screen.queryByText(/POST body must include/i)).toBeNull();
    expect(routerRefresh).not.toHaveBeenCalled();
    expect(onOpenChange).not.toHaveBeenCalledWith(false);
  });
});
