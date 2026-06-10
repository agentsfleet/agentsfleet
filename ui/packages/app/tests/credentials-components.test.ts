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

// ── jsonParseErrorMessage ──────────────────────────────────────────────────

describe("jsonParseErrorMessage", () => {
  it("returns the message of a thrown Error (the JSON.parse SyntaxError path)", async () => {
    const { jsonParseErrorMessage } = await import(
      "../app/(dashboard)/credentials/components/AddCredentialForm"
    );
    expect(jsonParseErrorMessage(new SyntaxError("Unexpected token x"))).toBe(
      "Unexpected token x",
    );
  });

  it("falls back to a fixed label for a non-Error throw value", async () => {
    const { jsonParseErrorMessage } = await import(
      "../app/(dashboard)/credentials/components/AddCredentialForm"
    );
    expect(jsonParseErrorMessage("not-an-error")).toBe("Invalid JSON");
  });
});

// ── AddCredentialForm ──────────────────────────────────────────────────────

describe("AddCredentialForm component", () => {
  async function renderForm() {
    const { default: AddCredentialForm } = await import(
      "../app/(dashboard)/credentials/components/AddCredentialForm"
    );
    render(React.createElement(AddCredentialForm, { workspaceId: "ws_1" } as never));
  }

  it("renders name + data inputs + submit button", async () => {
    await renderForm();
    expect(screen.getByLabelText(/^name$/i)).toBeTruthy();
    expect(screen.getByLabelText(/data \(json object\)/i)).toBeTruthy();
    expect(screen.getByRole("button", { name: /^add$/i })).toBeTruthy();
  });

  it("submit with empty fields shows zod required errors", async () => {
    const user = userEvent.setup();
    await renderForm();
    await user.click(screen.getByRole("button", { name: /^add$/i }));
    await waitFor(() => {
      expect(screen.getByText(/Credential name is required/i)).toBeTruthy();
      expect(screen.getByText(/Credential data is required/i)).toBeTruthy();
    });
    expect(createCredentialActionMock).not.toHaveBeenCalled();
  });

  // `userEvent.type` interprets `{` and `[` as keyboard descriptors, so use
  // fireEvent.change to set the textarea value verbatim for these cases.

  it("submit with invalid JSON shows Invalid JSON error", async () => {
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/^name$/i), "fly");
    fireEvent.change(screen.getByLabelText(/data \(json object\)/i), {
      target: { value: "{not json" },
    });
    await user.click(screen.getByRole("button", { name: /^add$/i }));
    await waitFor(() =>
      expect(screen.getByText(/Invalid JSON:/i)).toBeTruthy(),
    );
    expect(createCredentialActionMock).not.toHaveBeenCalled();
  });

  it("submit with array JSON rejects (must be object)", async () => {
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/^name$/i), "fly");
    fireEvent.change(screen.getByLabelText(/data \(json object\)/i), {
      target: { value: "[1,2,3]" },
    });
    await user.click(screen.getByRole("button", { name: /^add$/i }));
    await waitFor(() =>
      expect(screen.getByText(/Data must be a JSON object/i)).toBeTruthy(),
    );
    expect(createCredentialActionMock).not.toHaveBeenCalled();
  });

  it("submit with empty object rejects (must have one field)", async () => {
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/^name$/i), "fly");
    fireEvent.change(screen.getByLabelText(/data \(json object\)/i), {
      target: { value: "{}" },
    });
    await user.click(screen.getByRole("button", { name: /^add$/i }));
    await waitFor(() =>
      expect(screen.getByText(/Object must have at least one field/i)).toBeTruthy(),
    );
    expect(createCredentialActionMock).not.toHaveBeenCalled();
  });

  it("happy path: createCredentialAction called with parsed data, then router refresh", async () => {
    createCredentialActionMock.mockResolvedValue({ ok: true, data: { name: "fly" } });
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/^name$/i), "fly");
    await user.type(
      screen.getByLabelText(/data \(json object\)/i),
      '{{"host":"api.machines.dev","api_token":"T"}',
    );
    await user.click(screen.getByRole("button", { name: /^add$/i }));
    await waitFor(() =>
      expect(createCredentialActionMock).toHaveBeenCalledWith(
        "ws_1",
        { name: "fly", data: { host: "api.machines.dev", api_token: "T" } },
      ),
    );
    await waitFor(() => expect(routerRefresh).toHaveBeenCalled());
    expect(captureProductEventMock).toHaveBeenCalledTimes(1);
    expect(captureProductEventMock).toHaveBeenCalledWith(EVENTS.credential_added, { credential_name: "fly" });
    // The secret payload must never reach analytics.
    expect(JSON.stringify(captureProductEventMock.mock.calls)).not.toContain("api_token");
  });

  it("API error renders apiError below the form", async () => {
    createCredentialActionMock.mockResolvedValue({
      ok: false,
      error: "data too large",
      status: 400,
    });
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/^name$/i), "fly");
    await user.type(
      screen.getByLabelText(/data \(json object\)/i),
      '{{"host":"x"}',
    );
    await user.click(screen.getByRole("button", { name: /^add$/i }));
    await waitFor(() =>
      expect(screen.getByText(/data too large/i)).toBeTruthy(),
    );
    expect(captureProductEventMock).not.toHaveBeenCalled();
  });

  it("API error with empty string falls back to default message (covers `||` short-circuit)", async () => {
    createCredentialActionMock.mockResolvedValue({
      ok: false,
      error: "",
      status: 500,
    });
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/^name$/i), "fly");
    await user.type(
      screen.getByLabelText(/data \(json object\)/i),
      '{{"host":"x"}',
    );
    await user.click(screen.getByRole("button", { name: /^add$/i }));
    // WS-G — empty error from the action falls through presentError's
    // default path ("Couldn't store the credential. Try again …").
    await waitFor(() =>
      expect(screen.getByText(/Couldn't store the credential/i)).toBeTruthy(),
    );
  });

  it("unauthenticated action result surfaces Not authenticated", async () => {
    createCredentialActionMock.mockResolvedValue({
      ok: false,
      error: "Not authenticated",
      status: 401,
    });
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/^name$/i), "fly");
    await user.type(
      screen.getByLabelText(/data \(json object\)/i),
      '{{"host":"x"}',
    );
    await user.click(screen.getByRole("button", { name: /^add$/i }));
    await waitFor(() =>
      expect(screen.getByText(/Not authenticated/i)).toBeTruthy(),
    );
  });
});
