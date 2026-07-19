import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { EVENTS } from "../lib/analytics/events";
import { subscribeOnboardingRefresh } from "@/lib/onboarding-refresh";

// ── Shared mocks ───────────────────────────────────────────────────────────

const routerRefresh = vi.fn();

vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: routerRefresh, push: vi.fn() }),
}));

const createSecretActionMock = vi.fn();
const deleteSecretActionMock = vi.fn();
vi.mock("@/app/(dashboard)/w/[workspaceId]/secrets/actions", () => ({
  createSecretAction: createSecretActionMock,
  deleteSecretAction: deleteSecretActionMock,
}));

const captureProductEventMock = vi.fn();
vi.mock("@/lib/analytics/posthog", () => ({
  captureProductEvent: captureProductEventMock,
}));

// Dialog tests own dialog/form wiring; island-dynamic.test covers next/dynamic,
// so this suite renders the real form without the framework loader race.
vi.mock("@/components/domain/island-dynamic/AddSecretFormDynamic", async () => {
  const ReactModule = await import("react");
  const { default: AddSecretForm } = await import(
    "@/app/(dashboard)/w/[workspaceId]/secrets/components/AddSecretForm"
  );
  type AddSecretFormProps = Parameters<typeof AddSecretForm>[0];
  const AddSecretFormDynamicMock = (props: AddSecretFormProps) =>
    ReactModule.createElement(AddSecretForm, props);
  return { default: AddSecretFormDynamicMock };
});

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
    PlusIcon: make("PlusIcon"),
    CircleHelpIcon: make("CircleHelpIcon"),
    XIcon: make("XIcon"),
  };
});

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => cleanup());

// ── EditSecretDialog — dismiss guard ────────────────────────────────────

describe("EditSecretDialog dismiss guard", () => {
  afterEach(() => createSecretActionMock.mockReset());

  it("blocks dialog dismissal while a rotate save is in flight", async () => {
    const onOpenChange = vi.fn();
    // A deferred result keeps the save transition pending until we resolve it,
    // so `pending` is true when we attempt to dismiss; resolving at the end
    // lets the transition settle instead of leaking into later tests.
    let resolveSave!: (v: { ok: true; data: { name: string } }) => void;
    createSecretActionMock.mockReturnValue(
      new Promise<{ ok: true; data: { name: string } }>((r) => {
        resolveSave = r;
      }),
    );
    const { default: EditSecretDialog } = await import(
      "../app/(dashboard)/w/[workspaceId]/secrets/components/EditSecretDialog"
    );
    render(
      React.createElement(EditSecretDialog, {
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

// ── AddSecretDialog — trigger open/close wiring ─────────────────────────
// Trigger, dialog title, and the form's own submit button all share the
// label "Create secret" (a follow-up: consistent with "Create key" /
// "Create workspace" / "Create fleet library" elsewhere in the product) —
// once the dialog is open, both the still-mounted trigger and the submit
// button match by that name, so tests scope to `within(dialog)` to
// disambiguate the submit, same pattern as api-keys-components.test.ts's
// "Create key" trigger/submit pair.

describe("AddSecretDialog", () => {
  afterEach(() => createSecretActionMock.mockReset());

  async function renderDialog() {
    const { default: AddSecretDialog } = await import(
      "../app/(dashboard)/w/[workspaceId]/secrets/components/AddSecretDialog"
    );
    render(React.createElement(AddSecretDialog, { workspaceId: "ws_1" }));
  }

  it("does not mount dialog content until the trigger is clicked", async () => {
    await renderDialog();
    expect(screen.getByRole("button", { name: "Create secret" })).toBeTruthy();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("renders a PlusIcon on the create-secret trigger (test_create_triggers_render_plus_icon)", async () => {
    await renderDialog();
    const trigger = screen.getByRole("button", { name: "Create secret" });
    // This file mocks lucide-react as <svg data-icon={name}> (see top-of-file
    // vi.mock), so assert on the mock's marker, not the real lucide-* class.
    expect(trigger.querySelector('[data-icon="PlusIcon"]')).toBeTruthy();
  });

  it("opens the create-secret form when the trigger is clicked", async () => {
    const user = userEvent.setup();
    await renderDialog();
    await user.click(screen.getByRole("button", { name: "Create secret" }));
    await waitFor(() => expect(screen.getByRole("dialog")).toBeTruthy());
    expect(
      within(screen.getByRole("dialog")).getByRole("heading", { name: "Create secret" }),
    ).toBeTruthy();
    await waitFor(() => expect(screen.getByLabelText(/secret name/i)).toBeTruthy());
  });

  it("links out to the docs from the dialog description", async () => {
    const user = userEvent.setup();
    await renderDialog();
    await user.click(screen.getByRole("button", { name: "Create secret" }));
    const link = await screen.findByRole("link", { name: /learn more/i });
    expect(link.getAttribute("href")).toBe("https://docs.agentsfleet.net/fleets/credentials");
    expect(link.getAttribute("target")).toBe("_blank");
  });

  it("closes the dialog after a successful submit (onDone wiring)", async () => {
    createSecretActionMock.mockResolvedValue({ ok: true, data: { name: "stripe" } });
    const user = userEvent.setup();
    await renderDialog();
    await user.click(screen.getByRole("button", { name: "Create secret" }));
    await waitFor(() => expect(screen.getByLabelText(/secret name/i)).toBeTruthy());
    await user.type(screen.getByLabelText(/secret name/i), "stripe");
    await user.type(screen.getByLabelText("Field 1 name"), "api_key");
    await user.type(screen.getByLabelText("Field 1 value"), "sk-test");
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Create secret" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(routerRefresh).toHaveBeenCalled();
  });

  it("stays open and never calls the action when dismissed via Close", async () => {
    const user = userEvent.setup();
    await renderDialog();
    await user.click(screen.getByRole("button", { name: "Create secret" }));
    await waitFor(() => expect(screen.getByRole("dialog")).toBeTruthy());
    await user.click(screen.getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(createSecretActionMock).not.toHaveBeenCalled();
  });

  it("closes from Cancel without creating a secret", async () => {
    const user = userEvent.setup();
    await renderDialog();
    await user.click(screen.getByRole("button", { name: "Create secret" }));
    await waitFor(() => expect(screen.getByLabelText(/secret name/i)).toBeTruthy());
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: /^cancel$/i }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(createSecretActionMock).not.toHaveBeenCalled();
  });
});

// ── AddSecretForm (field/value builder) ─────────────────────────────────
// (jsonParseErrorMessage + parseSecretDataObject now live in
// tests/secret-data.test.ts, co-located with the pure module they cover.)

describe("AddSecretForm component", () => {
  async function renderForm() {
    const { default: AddSecretForm } = await import(
      "../app/(dashboard)/w/[workspaceId]/secrets/components/AddSecretForm"
    );
    render(React.createElement(AddSecretForm, { workspaceId: "ws_1" } as never));
  }

  const ADD_SECRET = { name: /^create secret$/i } as const;
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
    expect(createSecretActionMock).not.toHaveBeenCalled();
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
    expect(createSecretActionMock).not.toHaveBeenCalled();
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
    expect(createSecretActionMock).not.toHaveBeenCalled();
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
    expect(createSecretActionMock).not.toHaveBeenCalled();
  });

  it("happy path: assembles the JSON object, calls the action, refreshes; no secret in analytics", async () => {
    const refreshed = vi.fn();
    const unsubscribe = subscribeOnboardingRefresh("ws_1", refreshed);
    createSecretActionMock.mockResolvedValue({ ok: true, data: { name: "stripe" } });
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
      expect(createSecretActionMock).toHaveBeenCalledWith("ws_1", {
        name: "stripe",
        data: { api_key: "sk-test", webhook: "whsec" },
      }),
    );
    await waitFor(() => expect(routerRefresh).toHaveBeenCalled());
    expect(refreshed).toHaveBeenCalledTimes(1);
    expect(captureProductEventMock).toHaveBeenCalledWith(EVENTS.secret_added, {
      secret_name: "stripe",
    });
    // The secret value must never reach analytics.
    expect(JSON.stringify(captureProductEventMock.mock.calls)).not.toContain("sk-test");
    unsubscribe();
  });

  it("API error renders below the form", async () => {
    const refreshed = vi.fn();
    const unsubscribe = subscribeOnboardingRefresh("ws_1", refreshed);
    createSecretActionMock.mockResolvedValue({ ok: false, error: "data too large", status: 400 });
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/secret name/i), "stripe");
    await user.type(screen.getByLabelText("Field 1 name"), "api_key");
    await user.type(screen.getByLabelText("Field 1 value"), "v");
    await user.click(screen.getByRole("button", ADD_SECRET));
    await waitFor(() => expect(screen.getByText(/data too large/i)).toBeTruthy());
    expect(captureProductEventMock).not.toHaveBeenCalled();
    expect(refreshed).not.toHaveBeenCalled();
    unsubscribe();
  });

  it("API error with empty string falls back to the default message", async () => {
    createSecretActionMock.mockResolvedValue({ ok: false, error: "", status: 500 });
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/secret name/i), "stripe");
    await user.type(screen.getByLabelText("Field 1 name"), "api_key");
    await user.type(screen.getByLabelText("Field 1 value"), "v");
    await user.click(screen.getByRole("button", ADD_SECRET));
    await waitFor(() => expect(screen.getByText(/Couldn't store the secret/i)).toBeTruthy());
  });

  it("unauthenticated action surfaces Not authenticated", async () => {
    createSecretActionMock.mockResolvedValue({ ok: false, error: "Not authenticated", status: 401 });
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/secret name/i), "stripe");
    await user.type(screen.getByLabelText("Field 1 name"), "api_key");
    await user.type(screen.getByLabelText("Field 1 value"), "v");
    await user.click(screen.getByRole("button", ADD_SECRET));
    await waitFor(() => expect(screen.getByText(/Not authenticated/i)).toBeTruthy());
  });
});
