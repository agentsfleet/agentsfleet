import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { EVENTS } from "../lib/analytics/events";

// ── Shared mocks ───────────────────────────────────────────────────────────

const routerRefresh = vi.fn();

vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: routerRefresh, push: vi.fn() }),
}));

const createCredentialActionMock = vi.fn();
const deleteCredentialActionMock = vi.fn();
vi.mock("@/app/(dashboard)/credentials/actions", () => ({
  createCredentialAction: createCredentialActionMock,
  deleteCredentialAction: deleteCredentialActionMock,
}));

const captureProductEventMock = vi.fn();
vi.mock("@/lib/analytics/posthog", () => ({
  captureProductEvent: captureProductEventMock,
}));

// Use the real ConfirmDialog (for errorMessage rendering) and lucide stubs;
// stub only the form primitives that pull radix client-only providers we
// don't need at unit level.
vi.mock("lucide-react", () => {
  const make = (name: string) => {
    const C = (p: Record<string, unknown>) =>
      React.createElement("svg", { ...p, "data-icon": name });
    C.displayName = name;
    return C;
  };
  return {
    Trash2Icon: make("Trash2Icon"),
    Loader2Icon: make("Loader2Icon"),
    KeyRoundIcon: make("KeyRoundIcon"),
    PencilIcon: make("PencilIcon"),
    XIcon: make("XIcon"),
  };
});

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => cleanup());

// ── EditCredentialDialog — dismiss guard ────────────────────────────────────

describe("EditCredentialDialog dismiss guard", () => {
  afterEach(() => createCredentialActionMock.mockReset());

  it("blocks dialog dismissal while a rotate save is in flight", async () => {
    const onOpenChange = vi.fn();
    // A deferred result keeps the save transition pending until we resolve it,
    // so `pending` is true when we attempt to dismiss; resolving at the end
    // lets the transition settle instead of leaking into later tests.
    let resolveSave!: (v: { ok: true; data: { name: string } }) => void;
    createCredentialActionMock.mockReturnValue(
      new Promise<{ ok: true; data: { name: string } }>((r) => {
        resolveSave = r;
      }),
    );
    const { default: EditCredentialDialog } = await import(
      "../app/(dashboard)/credentials/components/EditCredentialDialog"
    );
    render(
      React.createElement(EditCredentialDialog, {
        workspaceId: "ws_1",
        name: "fly",
        open: true,
        onOpenChange,
      }),
    );
    fireEvent.change(screen.getByLabelText(/Data \(JSON object\)/i), {
      target: { value: '{"api_key":"sk-x"}' },
    });
    fireEvent.click(screen.getByRole("button", { name: "Rotate" }));
    await waitFor(() => {
      expect(document.querySelector('button[aria-busy="true"]')).not.toBeNull();
    });
    // The dialog's Close affordance fires onOpenChange(false); handleOpenChange's
    // `if (pending) return` blocks propagation so the parent close handler — and
    // thus the dismissal — never fires mid-save.
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onOpenChange).not.toHaveBeenCalled();
    // Settle the in-flight save so the transition doesn't leak.
    await act(async () => {
      resolveSave({ ok: true, data: { name: "fly" } });
    });
  });
});

// ── AddCredentialForm (field/value builder) ─────────────────────────────────
// (jsonParseErrorMessage + parseCredentialDataObject now live in
// tests/credential-data.test.ts, co-located with the pure module they cover.)

describe("AddCredentialForm component", () => {
  async function renderForm() {
    const { default: AddCredentialForm } = await import(
      "../app/(dashboard)/credentials/components/AddCredentialForm"
    );
    render(React.createElement(AddCredentialForm, { workspaceId: "ws_1" } as never));
  }

  const ADD_SECRET = { name: /^add secret$/i } as const;
  const ADD_FIELD = { name: /\+ add field/i } as const;

  it("renders secret name, one field row, add-field, and submit", async () => {
    await renderForm();
    expect(screen.getByLabelText(/secret name/i)).toBeTruthy();
    expect(screen.getByLabelText("Field 1 name")).toBeTruthy();
    expect(screen.getByLabelText("Field 1 value")).toBeTruthy();
    expect(screen.getByRole("button", ADD_FIELD)).toBeTruthy();
    expect(screen.getByRole("button", ADD_SECRET)).toBeTruthy();
  });

  it("submit while empty shows required errors and does not call the action", async () => {
    const user = userEvent.setup();
    await renderForm();
    await user.click(screen.getByRole("button", ADD_SECRET));
    await waitFor(() => {
      expect(screen.getByText(/Secret name is required/i)).toBeTruthy();
      expect(screen.getByText(/Field name is required/i)).toBeTruthy();
      expect(screen.getByText(/Value is required/i)).toBeTruthy();
    });
    expect(createCredentialActionMock).not.toHaveBeenCalled();
  });

  it("rejects an invalid (non-identifier) field name", async () => {
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/secret name/i), "stripe");
    await user.type(screen.getByLabelText("Field 1 name"), "bad name");
    await user.type(screen.getByLabelText("Field 1 value"), "v");
    await user.click(screen.getByRole("button", ADD_SECRET));
    await waitFor(() =>
      expect(screen.getByText(/Letters, numbers, and underscores only/i)).toBeTruthy(),
    );
    expect(createCredentialActionMock).not.toHaveBeenCalled();
  });

  it("rejects an invalid secret name", async () => {
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/secret name/i), "has space");
    await user.type(screen.getByLabelText("Field 1 name"), "api_key");
    await user.type(screen.getByLabelText("Field 1 value"), "v");
    await user.click(screen.getByRole("button", ADD_SECRET));
    await waitFor(() =>
      expect(screen.getByText(/Letters, numbers, dashes, and underscores only/i)).toBeTruthy(),
    );
    expect(createCredentialActionMock).not.toHaveBeenCalled();
  });

  it("adds and removes field rows (remove disabled at one row)", async () => {
    const user = userEvent.setup();
    await renderForm();
    expect((screen.getByLabelText("Remove field 1") as HTMLButtonElement).disabled).toBe(true);
    await user.click(screen.getByRole("button", ADD_FIELD));
    expect(screen.getByLabelText("Field 2 name")).toBeTruthy();
    expect((screen.getByLabelText("Remove field 1") as HTMLButtonElement).disabled).toBe(false);
    await user.click(screen.getByLabelText("Remove field 2"));
    await waitFor(() => expect(screen.queryByLabelText("Field 2 name")).toBeNull());
  });

  it("rejects duplicate field names", async () => {
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/secret name/i), "stripe");
    await user.type(screen.getByLabelText("Field 1 name"), "api_key");
    await user.type(screen.getByLabelText("Field 1 value"), "a");
    await user.click(screen.getByRole("button", ADD_FIELD));
    await user.type(screen.getByLabelText("Field 2 name"), "api_key");
    await user.type(screen.getByLabelText("Field 2 value"), "b");
    await user.click(screen.getByRole("button", ADD_SECRET));
    await waitFor(() => expect(screen.getByText(/Duplicate field name/i)).toBeTruthy());
    expect(createCredentialActionMock).not.toHaveBeenCalled();
  });

  it("happy path: assembles the JSON object, calls the action, refreshes; no secret in analytics", async () => {
    createCredentialActionMock.mockResolvedValue({ ok: true, data: { name: "stripe" } });
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/secret name/i), "stripe");
    await user.type(screen.getByLabelText("Field 1 name"), "api_key");
    await user.type(screen.getByLabelText("Field 1 value"), "sk-test");
    await user.click(screen.getByRole("button", ADD_FIELD));
    await user.type(screen.getByLabelText("Field 2 name"), "webhook");
    await user.type(screen.getByLabelText("Field 2 value"), "whsec");
    await user.click(screen.getByRole("button", ADD_SECRET));
    await waitFor(() =>
      expect(createCredentialActionMock).toHaveBeenCalledWith("ws_1", {
        name: "stripe",
        data: { api_key: "sk-test", webhook: "whsec" },
      }),
    );
    await waitFor(() => expect(routerRefresh).toHaveBeenCalled());
    expect(captureProductEventMock).toHaveBeenCalledWith(EVENTS.credential_added, {
      credential_name: "stripe",
    });
    // The secret value must never reach analytics.
    expect(JSON.stringify(captureProductEventMock.mock.calls)).not.toContain("sk-test");
  });

  it("API error renders below the form", async () => {
    createCredentialActionMock.mockResolvedValue({ ok: false, error: "data too large", status: 400 });
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/secret name/i), "stripe");
    await user.type(screen.getByLabelText("Field 1 name"), "api_key");
    await user.type(screen.getByLabelText("Field 1 value"), "v");
    await user.click(screen.getByRole("button", ADD_SECRET));
    await waitFor(() => expect(screen.getByText(/data too large/i)).toBeTruthy());
    expect(captureProductEventMock).not.toHaveBeenCalled();
  });

  it("API error with empty string falls back to the default message", async () => {
    createCredentialActionMock.mockResolvedValue({ ok: false, error: "", status: 500 });
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/secret name/i), "stripe");
    await user.type(screen.getByLabelText("Field 1 name"), "api_key");
    await user.type(screen.getByLabelText("Field 1 value"), "v");
    await user.click(screen.getByRole("button", ADD_SECRET));
    await waitFor(() => expect(screen.getByText(/Couldn't store the credential/i)).toBeTruthy());
  });

  it("unauthenticated action surfaces Not authenticated", async () => {
    createCredentialActionMock.mockResolvedValue({ ok: false, error: "Not authenticated", status: 401 });
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/secret name/i), "stripe");
    await user.type(screen.getByLabelText("Field 1 name"), "api_key");
    await user.type(screen.getByLabelText("Field 1 value"), "v");
    await user.click(screen.getByRole("button", ADD_SECRET));
    await waitFor(() => expect(screen.getByText(/Not authenticated/i)).toBeTruthy());
  });
});
