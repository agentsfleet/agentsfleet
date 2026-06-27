import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { EVENTS } from "../lib/analytics/events";
import { CREDENTIAL_FIELD, PROVIDER_MODE } from "../lib/types";

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

// ── ProviderCredentialRows ────────────────────────────────────────────────

describe("ProviderCredentialRows component", () => {
  async function renderRows(provider: unknown = null) {
    const { default: ProviderCredentialRows } = await import(
      "../app/(dashboard)/credentials/components/ProviderCredentialRows"
    );
    render(
      React.createElement(ProviderCredentialRows, {
        workspaceId: "ws_1",
        provider,
      } as never),
    );
  }

  // Key-only rows (design-preview parity): the credential stores provider +
  // api_key ONLY; the model is chosen in Models → own-key setup, never here.
  const ANTHROPIC_KEY_LABEL = "Anthropic API key value";

  it("stores an Anthropic key as key-only under the default name (no model)", async () => {
    createCredentialActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic" } });
    await renderRows();

    await userEvent.click(screen.getAllByRole("button", { name: "Add key" })[0]!);
    fireEvent.change(screen.getByLabelText(ANTHROPIC_KEY_LABEL), {
      target: { value: "sk-ant-test" },
    });
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(createCredentialActionMock).toHaveBeenCalledWith("ws_1", {
        name: "anthropic",
        data: {
          [CREDENTIAL_FIELD.provider]: "anthropic",
          [CREDENTIAL_FIELD.apiKey]: "sk-ant-test",
        },
      }),
    );
    // The model field is deliberately NOT written to the credential.
    const lastBody = createCredentialActionMock.mock.calls.at(-1)![1] as {
      data: Record<string, unknown>;
    };
    expect(lastBody.data).not.toHaveProperty(CREDENTIAL_FIELD.model);
    expect(captureProductEventMock).toHaveBeenCalledWith(EVENTS.credential_added, {
      credential_name: "anthropic",
    });
    expect(JSON.stringify(captureProductEventMock.mock.calls)).not.toContain("sk-ant-test");
    await waitFor(() => expect(routerRefresh).toHaveBeenCalled());
  });

  it("requires the api key before saving", async () => {
    await renderRows();

    await userEvent.click(screen.getAllByRole("button", { name: "Add key" })[0]!);
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(screen.getByText("API key is required")).toBeTruthy();
    expect(createCredentialActionMock).not.toHaveBeenCalled();

    createCredentialActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic" } });
    fireEvent.change(screen.getByLabelText(ANTHROPIC_KEY_LABEL), {
      target: { value: "sk-ant-test" },
    });
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(createCredentialActionMock).toHaveBeenCalled());
  });

  it("shows the provider-key save error", async () => {
    createCredentialActionMock.mockResolvedValue({
      ok: false,
      error: "vault offline",
      errorCode: "UZ-CRED-500",
      status: 500,
    });
    await renderRows();

    await userEvent.click(screen.getAllByRole("button", { name: "Add key" })[0]!);
    fireEvent.change(screen.getByLabelText(ANTHROPIC_KEY_LABEL), {
      target: { value: "sk-ant-test" },
    });
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(screen.getByText(/vault offline/i)).toBeTruthy());
    expect(routerRefresh).not.toHaveBeenCalled();
  });

  it("rotates a connected provider with Replace key under its existing ref", async () => {
    createCredentialActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic-prod" } });
    await renderRows({
      mode: PROVIDER_MODE.self_managed,
      provider: "anthropic",
      credential_ref: "anthropic-prod",
      model: "claude-sonnet-4-6",
      context_cap_tokens: null,
    });

    expect(screen.getByText("Connected")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Replace" }));
    fireEvent.change(screen.getByLabelText(ANTHROPIC_KEY_LABEL), {
      target: { value: "sk-ant-rotated" },
    });
    await userEvent.click(screen.getByRole("button", { name: "Replace key" }));

    await waitFor(() =>
      expect(createCredentialActionMock).toHaveBeenCalledWith("ws_1", {
        name: "anthropic-prod",
        data: {
          [CREDENTIAL_FIELD.provider]: "anthropic",
          [CREDENTIAL_FIELD.apiKey]: "sk-ant-rotated",
        },
      }),
    );
  });

  it("falls back when a self-managed provider has no matching credential ref", async () => {
    await renderRows({
      mode: PROVIDER_MODE.self_managed,
      provider: "gateway",
      credential_ref: null,
      model: null,
      context_cap_tokens: null,
    });

    expect(screen.getAllByText("Not connected")).toHaveLength(2);
  });

  it("rotates a connected provider even when its model is unset", async () => {
    createCredentialActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic-prod" } });
    await renderRows({
      mode: PROVIDER_MODE.self_managed,
      provider: "anthropic",
      credential_ref: "anthropic-prod",
      model: null,
      context_cap_tokens: null,
    });

    await userEvent.click(screen.getByRole("button", { name: "Replace" }));
    fireEvent.change(screen.getByLabelText(ANTHROPIC_KEY_LABEL), {
      target: { value: "sk-ant-x" },
    });
    await userEvent.click(screen.getByRole("button", { name: "Replace key" }));

    await waitFor(() =>
      expect(createCredentialActionMock).toHaveBeenCalledWith(
        "ws_1",
        expect.objectContaining({ name: "anthropic-prod" }),
      ),
    );
  });

  it("closes the open provider form when its action is clicked again", async () => {
    await renderRows();

    const firstAddButton = screen.getAllByRole("button", { name: "Add key" })[0]!;
    await userEvent.click(firstAddButton);
    expect(screen.getByLabelText(ANTHROPIC_KEY_LABEL)).toBeTruthy();

    await userEvent.click(firstAddButton);
    expect(screen.queryByLabelText(ANTHROPIC_KEY_LABEL)).toBeNull();
  });
});
