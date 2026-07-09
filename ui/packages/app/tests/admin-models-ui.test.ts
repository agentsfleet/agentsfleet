import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

// Only the server-action module is stubbed; lib/api/admin_models (the $/1M⇄nanos
// conversion) stays real so the form's actual conversion is exercised, not faked.
// vi.hoisted: vi.mock is hoisted above const decls, so the mock fns must be too.
const {
  createAdminModelActionMock,
  setPlatformDefaultActionMock,
  deleteAdminModelActionMock,
  updateAdminModelActionMock,
  captureProductEventMock,
  routerRefreshMock,
} = vi.hoisted(() => ({
  createAdminModelActionMock: vi.fn(),
  setPlatformDefaultActionMock: vi.fn(),
  deleteAdminModelActionMock: vi.fn(),
  updateAdminModelActionMock: vi.fn(),
  captureProductEventMock: vi.fn(),
  routerRefreshMock: vi.fn(),
}));

vi.mock("@/app/(dashboard)/admin/models/actions", () => ({
  createAdminModelAction: createAdminModelActionMock,
  setPlatformDefaultAction: setPlatformDefaultActionMock,
  deleteAdminModelAction: deleteAdminModelActionMock,
  updateAdminModelAction: updateAdminModelActionMock,
  listAdminModelsAction: vi.fn(),
  listPlatformKeysAction: vi.fn(),
}));
vi.mock("@/lib/analytics/posthog", () => ({ captureProductEvent: captureProductEventMock }));
vi.mock("next/navigation", () => ({ useRouter: () => ({ refresh: routerRefreshMock }) }));

import AddModelDialog from "@/app/(dashboard)/admin/models/components/AddModelDialog";
import CatalogueList from "@/app/(dashboard)/admin/models/components/CatalogueList";
import ModelsView from "@/app/(dashboard)/admin/models/components/ModelsView";
import { type AdminModel, type PlatformKey, OPENAI_COMPATIBLE_PROVIDER } from "@/lib/api/admin_models";
import { EVENTS } from "../lib/analytics/events";

// The catalogue is a design-system DataTable — scope a row by its (unique)
// model_id cell and walk up to its <tr>.
function rowFor(modelId: string): HTMLElement {
  return screen.getByText(modelId).closest("tr")!;
}

const CATALOGUE: AdminModel[] = [
  { uid: "u1", provider: "fireworks", model_id: "glm-5.2", context_cap_tokens: 128000, input_nanos_per_mtok: 550_000_000, cached_input_nanos_per_mtok: 140_000_000, output_nanos_per_mtok: 2_190_000_000 },
  { uid: "u2", provider: "anthropic", model_id: "claude-opus-4-8", context_cap_tokens: 200000, input_nanos_per_mtok: 15_000_000_000, cached_input_nanos_per_mtok: 1_500_000_000, output_nanos_per_mtok: 75_000_000_000 },
];

const DEFAULT_FIREWORKS: PlatformKey = {
  provider: "fireworks", source_workspace_id: "ws1", model: "glm-5.2", active: true, updated_at: 1,
};

beforeEach(() => vi.clearAllMocks());
afterEach(() => cleanup());

describe("AddModelDialog", () => {
  it("renders a PlusIcon on the create-model-library trigger (test_create_triggers_render_plus_icon)", () => {
    render(React.createElement(AddModelDialog, { onCreated: vi.fn() }));
    const trigger = screen.getByRole("button", { name: "Create model library" });
    expect(trigger.querySelector("svg.lucide-plus")).toBeTruthy();
  });

  it("describes the entry exactly: prices a model per token, rates per 1M tokens", async () => {
    render(React.createElement(AddModelDialog, { onCreated: vi.fn() }));
    await userEvent.setup().click(screen.getByRole("button", { name: "Create model library" }));
    const dialog = within(screen.getByRole("dialog"));
    expect(dialog.getByText("A model library entry prices a model per token. Rates are per 1M tokens.")).toBeTruthy();
  });

  it("should convert $/1M entry to integer nanos when creating a model", async () => {
    const user = userEvent.setup();
    createAdminModelActionMock.mockResolvedValue({ ok: true, data: { ...CATALOGUE[0] } });
    const onCreated = vi.fn();
    render(React.createElement(AddModelDialog, { onCreated }));

    await user.click(screen.getByRole("button", { name: "Create model library" }));
    fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "fireworks" } });
    fireEvent.change(screen.getByLabelText("Model"), { target: { value: "glm-5.2" } });
    fireEvent.change(screen.getByLabelText("Input $/1M"), { target: { value: "0.55" } });

    const dialog = screen.getByRole("dialog");
    fireEvent.submit(dialog.querySelector("form")!);

    await waitFor(() => expect(createAdminModelActionMock).toHaveBeenCalledTimes(1));
    const arg = createAdminModelActionMock.mock.calls[0]![0];
    expect(arg.provider).toBe("fireworks");
    expect(arg.input_nanos_per_mtok).toBe(550_000_000);
    expect(onCreated).toHaveBeenCalledTimes(1);
  });

  it("should reject an empty provider and not call the create action", async () => {
    const user = userEvent.setup();
    render(React.createElement(AddModelDialog, { onCreated: vi.fn() }));
    await user.click(screen.getByRole("button", { name: "Create model library" }));
    await user.type(screen.getByLabelText("Model"), "glm-5.2");
    const dialog = screen.getByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Create model library" }));
    await new Promise((r) => setTimeout(r, 50));
    expect(createAdminModelActionMock).not.toHaveBeenCalled();
  });

  it("surfaces the action error and keeps the dialog open when the create fails", async () => {
    createAdminModelActionMock.mockResolvedValue({ ok: false, error: "model exists" });
    const onCreated = vi.fn();
    render(React.createElement(AddModelDialog, { onCreated }));

    await userEvent.setup().click(screen.getByRole("button", { name: "Create model library" }));
    fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "fireworks" } });
    fireEvent.change(screen.getByLabelText("Model"), { target: { value: "glm-5.2" } });

    const dialog = screen.getByRole("dialog");
    fireEvent.submit(dialog.querySelector("form")!);

    await waitFor(() => expect(within(screen.getByRole("dialog")).getByText(/model exists/i)).toBeTruthy());
    expect(onCreated).not.toHaveBeenCalled();
  });
});

describe("CatalogueList — rows + rates + empty state", () => {
  it("renders a priced row per catalogue model with $/1M rates", () => {
    render(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted: vi.fn(), onUpdated: vi.fn() }));
    expect(screen.getByTestId("data-table")).toBeTruthy();
    expect(screen.getByText("glm-5.2")).toBeTruthy();
    expect(screen.getByText("0.55 / 0.14 / 2.19")).toBeTruthy();
  });

  it("shows the empty state when there are no models", () => {
    render(React.createElement(CatalogueList, { models: [], activeDefault: null, onDeleted: vi.fn(), onUpdated: vi.fn() }));
    expect(screen.getByText("No models yet")).toBeTruthy();
    expect(screen.getByText("Add a model to price it and make it the platform default.")).toBeTruthy();
    expect(screen.queryByTestId("data-table")).toBeNull();
  });
});

describe("CatalogueList — Delete (icon-only, confirm-gated)", () => {
  it("does not delete on the icon click alone — a confirm dialog gates the irreversible action", async () => {
    const onDeleted = vi.fn();
    render(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted, onUpdated: vi.fn() }));

    fireEvent.click(within(rowFor("glm-5.2")).getByRole("button", { name: "Delete glm-5.2" }));

    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    expect(deleteAdminModelActionMock).not.toHaveBeenCalled();
    expect(onDeleted).not.toHaveBeenCalled();
  });

  it("removes a row from the parent on a successful delete, after confirming", async () => {
    deleteAdminModelActionMock.mockResolvedValue({ ok: true, data: undefined });
    const onDeleted = vi.fn();
    render(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted, onUpdated: vi.fn() }));

    fireEvent.click(within(rowFor("glm-5.2")).getByRole("button", { name: "Delete glm-5.2" }));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Delete" }));

    await waitFor(() => expect(deleteAdminModelActionMock).toHaveBeenCalledWith("u1"));
    await waitFor(() => expect(onDeleted).toHaveBeenCalledWith("u1"));
  });

  it("delete failure surfaces errorMessage inline and keeps the dialog open (test_catalogue_row_delete_is_icon_only_same_behavior)", async () => {
    deleteAdminModelActionMock.mockResolvedValue({ ok: false, error: "model is the active platform default" });
    render(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted: vi.fn(), onUpdated: vi.fn() }));

    fireEvent.click(within(rowFor("claude-opus-4-8")).getByRole("button", { name: "Delete claude-opus-4-8" }));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Delete" }));

    await waitFor(() => expect(screen.getByRole("alert").textContent).toMatch(/model is the active platform default/i));
    expect(screen.queryByRole("alertdialog")).toBeTruthy();
  });

  it("Delete is an icon-only destructive button with an aria-label, not text", () => {
    render(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted: vi.fn(), onUpdated: vi.fn() }));
    const del = within(rowFor("glm-5.2")).getByRole("button", { name: "Delete glm-5.2" });
    expect(del.className).toContain("bg-destructive");
    expect(del.querySelector("svg.lucide-trash-2")).toBeTruthy();
    // No visible "Delete" text label — the trigger is icon-only now.
    expect(del.textContent?.trim()).toBe("");
  });

  it("cancel on the dialog clears the target without invoking the delete action", async () => {
    const user = userEvent.setup();
    render(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted: vi.fn(), onUpdated: vi.fn() }));

    await user.click(within(rowFor("glm-5.2")).getByRole("button", { name: "Delete glm-5.2" }));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    await user.click(screen.getByRole("button", { name: /^cancel$/i }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(deleteAdminModelActionMock).not.toHaveBeenCalled();
  });
});

describe("CatalogueList — Edit (rates dialog wired to updateAdminModelAction)", () => {
  it("opens a dialog pre-filled with the row's rates and PATCHes the new rates by uid (test_catalogue_row_edit_dialog_updates_rates)", async () => {
    updateAdminModelActionMock.mockResolvedValue({ ok: true, data: { uid: "u1", updated: true } });
    const onUpdated = vi.fn();
    render(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted: vi.fn(), onUpdated }));

    fireEvent.click(within(rowFor("glm-5.2")).getByRole("button", { name: "Edit glm-5.2" }));
    const dialog = within(await screen.findByRole("dialog"));
    // Pre-filled from the row: 550_000_000 nanos → $0.55, cap 128000.
    expect((dialog.getByLabelText("Input $/1M") as HTMLInputElement).value).toBe("0.55");
    expect((dialog.getByLabelText("Context cap (tokens)") as HTMLInputElement).value).toBe("128000");

    fireEvent.change(dialog.getByLabelText("Input $/1M"), { target: { value: "0.99" } });
    fireEvent.submit(screen.getByRole("dialog").querySelector("form")!);

    await waitFor(() => expect(updateAdminModelActionMock).toHaveBeenCalledTimes(1));
    expect(updateAdminModelActionMock).toHaveBeenCalledWith("u1", {
      context_cap_tokens: 128000,
      input_nanos_per_mtok: 990_000_000, // 0.99 $/1M
      cached_input_nanos_per_mtok: 140_000_000,
      output_nanos_per_mtok: 2_190_000_000,
    });
    // The parent gets the row's new shape (identity + edited rates).
    await waitFor(() => expect(onUpdated).toHaveBeenCalledWith(expect.objectContaining({ uid: "u1", input_nanos_per_mtok: 990_000_000 })));
  });

  it("shows the immutable provider + model as disabled fields", async () => {
    render(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted: vi.fn(), onUpdated: vi.fn() }));
    fireEvent.click(within(rowFor("glm-5.2")).getByRole("button", { name: "Edit glm-5.2" }));
    const dialog = within(await screen.findByRole("dialog"));
    const provider = dialog.getByLabelText("Provider (locked)") as HTMLInputElement;
    const modelId = dialog.getByLabelText("Model (locked)") as HTMLInputElement;
    expect(provider.disabled).toBe(true);
    expect(provider.value).toBe("fireworks");
    expect(modelId.disabled).toBe(true);
    expect(modelId.value).toBe("glm-5.2");
  });

  it("surfaces the update error inline and keeps the edit dialog open on failure", async () => {
    updateAdminModelActionMock.mockResolvedValue({ ok: false, error: "rate rejected" });
    const onUpdated = vi.fn();
    render(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted: vi.fn(), onUpdated }));

    fireEvent.click(within(rowFor("glm-5.2")).getByRole("button", { name: "Edit glm-5.2" }));
    await screen.findByRole("dialog");
    fireEvent.submit(screen.getByRole("dialog").querySelector("form")!);

    await waitFor(() => expect(within(screen.getByRole("dialog")).getByText(/rate rejected/i)).toBeTruthy());
    expect(onUpdated).not.toHaveBeenCalled();
  });

  it("rejects a non-positive context cap without calling the action", async () => {
    updateAdminModelActionMock.mockResolvedValue({ ok: true, data: { uid: "u1", updated: true } });
    render(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted: vi.fn(), onUpdated: vi.fn() }));

    fireEvent.click(within(rowFor("glm-5.2")).getByRole("button", { name: "Edit glm-5.2" }));
    const dialog = within(await screen.findByRole("dialog"));
    fireEvent.change(dialog.getByLabelText("Context cap (tokens)"), { target: { value: "0" } });
    fireEvent.change(dialog.getByLabelText("Input $/1M"), { target: { value: "-1" } });
    fireEvent.submit(screen.getByRole("dialog").querySelector("form")!);

    await new Promise((r) => setTimeout(r, 50));
    expect(updateAdminModelActionMock).not.toHaveBeenCalled();
  });
});

describe("CatalogueList — Make default (★ minimal key dialog)", () => {
  it("★ opens a minimal key dialog; saving activates the row's (provider, model) as the default", async () => {
    setPlatformDefaultActionMock.mockResolvedValue({ ok: true, data: { provider: "fireworks", model: "glm-5.2", active: true } });
    render(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted: vi.fn(), onUpdated: vi.fn() }));

    fireEvent.click(within(rowFor("glm-5.2")).getByRole("button", { name: "Make glm-5.2 the platform default" }));
    const dialog = within(await screen.findByRole("dialog"));
    // No provider/model selects — identity comes from the row.
    fireEvent.change(dialog.getByLabelText("API key"), { target: { value: "sk-secret" } });
    fireEvent.click(dialog.getByRole("button", { name: "Make default" }));

    await waitFor(() => expect(setPlatformDefaultActionMock).toHaveBeenCalledTimes(1));
    expect(setPlatformDefaultActionMock).toHaveBeenCalledWith({
      provider: "fireworks",
      model: "glm-5.2",
      api_key: "sk-secret",
      base_url: undefined,
    });
    expect(captureProductEventMock).toHaveBeenCalledWith(EVENTS.platform_default_set, {
      provider: "fireworks",
      model: "glm-5.2",
      is_custom: false,
    });
    // On success it re-reads the server so the Default badge moves to this row.
    await waitFor(() => expect(routerRefreshMock).toHaveBeenCalled());
  });

  it("treats a whitespace-only API key as empty (Make default stays disabled)", async () => {
    render(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted: vi.fn(), onUpdated: vi.fn() }));
    fireEvent.click(within(rowFor("glm-5.2")).getByRole("button", { name: "Make glm-5.2 the platform default" }));
    const dialog = within(await screen.findByRole("dialog"));
    fireEvent.change(dialog.getByLabelText("API key"), { target: { value: "   " } });
    expect((dialog.getByRole("button", { name: "Make default" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("requires a base URL for an openai-compatible row and threads it into the activation", async () => {
    setPlatformDefaultActionMock.mockResolvedValue({ ok: true, data: { provider: OPENAI_COMPATIBLE_PROVIDER, model: "glm-5.2", active: true } });
    const custom: AdminModel[] = [
      { uid: "c1", provider: OPENAI_COMPATIBLE_PROVIDER, model_id: "glm-5.2", context_cap_tokens: 128000, input_nanos_per_mtok: 0, cached_input_nanos_per_mtok: 0, output_nanos_per_mtok: 0 },
    ];
    render(React.createElement(CatalogueList, { models: custom, activeDefault: null, onDeleted: vi.fn(), onUpdated: vi.fn() }));

    fireEvent.click(within(rowFor("glm-5.2")).getByRole("button", { name: "Make glm-5.2 the platform default" }));
    const dialog = within(await screen.findByRole("dialog"));
    fireEvent.change(dialog.getByLabelText("API key"), { target: { value: "sk-secret" } });
    const save = dialog.getByRole("button", { name: "Make default" }) as HTMLButtonElement;
    // Save stays disabled until the base URL is filled (canSave's isCustom branch).
    expect(save.disabled).toBe(true);
    fireEvent.change(dialog.getByLabelText("Base URL"), { target: { value: "https://endpoint.example/v1" } });
    fireEvent.click(save);

    await waitFor(() => expect(setPlatformDefaultActionMock).toHaveBeenCalledWith({
      provider: OPENAI_COMPATIBLE_PROVIDER,
      model: "glm-5.2",
      api_key: "sk-secret",
      base_url: "https://endpoint.example/v1",
    }));
    expect(captureProductEventMock).toHaveBeenCalledWith(EVENTS.platform_default_set, {
      provider: OPENAI_COMPATIBLE_PROVIDER,
      model: "glm-5.2",
      is_custom: true,
    });
  });

  it("surfaces the activation error inline and does not refresh when the make-default fails", async () => {
    setPlatformDefaultActionMock.mockResolvedValue({ ok: false, error: "rate gate rejected the model" });
    render(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted: vi.fn(), onUpdated: vi.fn() }));

    fireEvent.click(within(rowFor("glm-5.2")).getByRole("button", { name: "Make glm-5.2 the platform default" }));
    const dialog = within(await screen.findByRole("dialog"));
    fireEvent.change(dialog.getByLabelText("API key"), { target: { value: "sk-secret" } });
    fireEvent.click(dialog.getByRole("button", { name: "Make default" }));

    await waitFor(() => expect(screen.getByText(/rate gate rejected the model/i)).toBeTruthy());
    expect(captureProductEventMock).not.toHaveBeenCalled();
    expect(routerRefreshMock).not.toHaveBeenCalled();
  });
});

describe("CatalogueList — Default badge", () => {
  it("badges the active default's row and hides its ★, while other rows keep ★", () => {
    render(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: DEFAULT_FIREWORKS, onDeleted: vi.fn(), onUpdated: vi.fn() }));

    const activeRow = within(rowFor("glm-5.2"));
    expect(activeRow.getByText("Default")).toBeTruthy();
    expect(activeRow.queryByRole("button", { name: "Make glm-5.2 the platform default" })).toBeNull();

    const otherRow = within(rowFor("claude-opus-4-8"));
    expect(otherRow.queryByText("Default")).toBeNull();
    expect(otherRow.getByRole("button", { name: "Make claude-opus-4-8 the platform default" })).toBeTruthy();
  });
});

describe("ModelsView", () => {
  const initial = { models: CATALOGUE };

  it("renders the catalogue with no separate platform-default section", () => {
    render(React.createElement(ModelsView, { initial, activeDefault: null }));
    expect(screen.getByRole("heading", { level: 1, name: "Model library" })).toBeTruthy();
    expect(screen.getByText("glm-5.2")).toBeTruthy();
    expect(screen.getByText("claude-opus-4-8")).toBeTruthy();
    // The old Platform Default form (its "Default provider" select) is gone.
    expect(screen.queryByLabelText("Default provider")).toBeNull();
  });

  it("passes the active default through so the matching row is badged", () => {
    render(React.createElement(ModelsView, { initial, activeDefault: DEFAULT_FIREWORKS }));
    expect(within(rowFor("glm-5.2")).getByText("Default")).toBeTruthy();
  });

  it("appends a newly created model to the catalogue without a round-trip", async () => {
    const created: AdminModel = {
      uid: "u3", provider: "moonshot", model_id: "kimi-k2.6", context_cap_tokens: 256000,
      input_nanos_per_mtok: 600_000_000, cached_input_nanos_per_mtok: 150_000_000, output_nanos_per_mtok: 2_300_000_000,
    };
    createAdminModelActionMock.mockResolvedValue({ ok: true, data: created });
    render(React.createElement(ModelsView, { initial, activeDefault: null }));

    await userEvent.setup().click(screen.getByRole("button", { name: "Create model library" }));
    const dialog = within(screen.getByRole("dialog"));
    fireEvent.change(dialog.getByLabelText("Provider"), { target: { value: "moonshot" } });
    fireEvent.change(dialog.getByLabelText("Model"), { target: { value: "kimi-k2.6" } });
    fireEvent.submit(screen.getByRole("dialog").querySelector("form")!);

    await waitFor(() => expect(screen.getByText("kimi-k2.6")).toBeTruthy());
  });

  it("drops a deleted model from the catalogue", async () => {
    deleteAdminModelActionMock.mockResolvedValue({ ok: true, data: undefined });
    render(React.createElement(ModelsView, { initial, activeDefault: null }));

    fireEvent.click(within(rowFor("glm-5.2")).getByRole("button", { name: "Delete glm-5.2" }));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Delete" }));

    await waitFor(() => expect(screen.queryByText("glm-5.2")).toBeNull());
    expect(screen.getByText("claude-opus-4-8")).toBeTruthy();
  });

  it("reflects an edited model's new rates in the table without a round-trip", async () => {
    updateAdminModelActionMock.mockResolvedValue({ ok: true, data: { uid: "u1", updated: true } });
    render(React.createElement(ModelsView, { initial, activeDefault: null }));

    fireEvent.click(within(rowFor("glm-5.2")).getByRole("button", { name: "Edit glm-5.2" }));
    const dialog = within(await screen.findByRole("dialog"));
    fireEvent.change(dialog.getByLabelText("Input $/1M"), { target: { value: "0.99" } });
    fireEvent.submit(screen.getByRole("dialog").querySelector("form")!);

    // 0.99 $/1M → $0.99 in the rates cell; the old 0.55 is gone.
    await waitFor(() => expect(screen.getByText("0.99 / 0.14 / 2.19")).toBeTruthy());
    expect(screen.queryByText("0.55 / 0.14 / 2.19")).toBeNull();
  });
});
