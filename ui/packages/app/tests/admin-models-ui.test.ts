import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TooltipProvider } from "@agentsfleet/design-system";

// Only the server-action module is stubbed; lib/api/admin_model_library (the $/1M⇄nanos
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
import { type AdminModel, type PlatformKey } from "@/lib/api/admin_model_library";
import { OPENAI_COMPATIBLE_PROVIDER } from "@/lib/types";
import { EVENTS } from "../lib/analytics/events";

function renderWithTooltipProvider(node: React.ReactElement) {
  return render(React.createElement(TooltipProvider, null, node));
}

// The catalogue is a design-system DataTable — scope a row by its (unique)
// model_id cell and walk up to its <tr>.
function rowFor(modelId: string): HTMLElement {
  return screen.getByText(modelId).closest("tr")!;
}

async function loadedEditDialog(label: string) {
  const field = await screen.findByLabelText(label);
  const dialog = field.closest('[role="dialog"]');
  if (!dialog) throw new Error("loaded edit field has no dialog owner");
  return within(dialog as HTMLElement);
}

const CATALOGUE: AdminModel[] = [
  { id: "u1", provider: "fireworks", model_id: "glm-5.2", context_cap_tokens: 128000, input_nanos_per_mtok: 550_000_000, cached_input_nanos_per_mtok: 140_000_000, output_nanos_per_mtok: 2_190_000_000 },
  { id: "u2", provider: "anthropic", model_id: "claude-opus-4-8", context_cap_tokens: 200000, input_nanos_per_mtok: 15_000_000_000, cached_input_nanos_per_mtok: 1_500_000_000, output_nanos_per_mtok: 75_000_000_000 },
];

const DEFAULT_FIREWORKS: PlatformKey = {
  provider: "fireworks", source_workspace_id: "ws1", model: "glm-5.2", active: true, updated_at: 1,
};

beforeEach(() => vi.clearAllMocks());
afterEach(() => {
  vi.restoreAllMocks();
  cleanup();
});

describe("AddModelDialog", () => {
  it("renders a PlusIcon on the create-model-library trigger (test_create_triggers_render_plus_icon)", () => {
    render(React.createElement(AddModelDialog, { onCreated: vi.fn() }));
    const trigger = screen.getByRole("button", { name: "Create model library" });
    expect(trigger.querySelector("svg.lucide-plus")).toBeTruthy();
  });

  it("describes the entry in user-facing pricing language", async () => {
    render(React.createElement(AddModelDialog, { onCreated: vi.fn() }));
    await userEvent.setup().click(screen.getByRole("button", { name: "Create model library" }));
    const dialog = within(screen.getByRole("dialog"));
    expect(dialog.getByText("Create a priced model users can choose. Prices are US dollars per 1M tokens.")).toBeTruthy();
  });

  it("should convert $/1M entry to integer nanos when creating a model", async () => {
    const user = userEvent.setup();
    createAdminModelActionMock.mockResolvedValue({ ok: true, data: { ...CATALOGUE[0] } });
    const onCreated = vi.fn();
    render(React.createElement(AddModelDialog, { onCreated }));

    await user.click(screen.getByRole("button", { name: "Create model library" }));
    fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "fireworks" } });
    fireEvent.change(screen.getByLabelText("Model"), { target: { value: "glm-5.2" } });
    fireEvent.change(screen.getByLabelText("Input"), { target: { value: "0.55" } });

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
    await user.click(within(dialog).getByRole("button", { name: "Create" }));
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

  it("closes from Cancel without creating a model", async () => {
    const user = userEvent.setup();
    render(React.createElement(AddModelDialog, { onCreated: vi.fn() }));

    await user.click(screen.getByRole("button", { name: "Create model library" }));
    const dialog = screen.getByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: /^cancel$/i }));

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(createAdminModelActionMock).not.toHaveBeenCalled();
  });
});

describe("CatalogueList — rows + rates + empty state", () => {
  it("renders a priced row per catalogue model with $/1M rates", () => {
    renderWithTooltipProvider(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted: vi.fn(), onUpdated: vi.fn() }));
    expect(screen.getByTestId("data-table")).toBeTruthy();
    expect(screen.getByText("glm-5.2")).toBeTruthy();
    expect(screen.getByText("0.55 / 0.14 / 2.19")).toBeTruthy();
  });

  it("sorts each catalogue data column from its header arrow", () => {
    renderWithTooltipProvider(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted: vi.fn(), onUpdated: vi.fn() }));

    for (const name of ["Provider", "Model", "Context", "Rates $/1M (in / cached / out)"]) {
      fireEvent.click(screen.getByRole("button", { name }));
      expect(screen.getByRole("columnheader", { name }).getAttribute("aria-sort")).not.toBe("none");
    }
  });

  it("shows the empty state when there are no models", () => {
    renderWithTooltipProvider(React.createElement(CatalogueList, { models: [], activeDefault: null, onDeleted: vi.fn(), onUpdated: vi.fn() }));
    expect(screen.getByText("No models yet")).toBeTruthy();
    expect(screen.getByText("Add a model to price it and make it the platform default.")).toBeTruthy();
    expect(screen.queryByTestId("data-table")).toBeNull();
  });

  it("does not speculate on the editor from a coarse-pointer hover", () => {
    vi.spyOn(window, "matchMedia").mockReturnValue({
      matches: true,
    } as MediaQueryList);
    renderWithTooltipProvider(
      React.createElement(CatalogueList, {
        models: CATALOGUE,
        activeDefault: null,
        onDeleted: vi.fn(),
        onUpdated: vi.fn(),
      }),
    );

    fireEvent.pointerEnter(
      within(rowFor("glm-5.2")).getByRole("button", {
        name: "Edit glm-5.2",
      }),
    );
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

describe("CatalogueList — Delete (icon-only, confirm-gated)", () => {
  it("does not delete on the icon click alone — a confirm dialog gates the irreversible action", async () => {
    const onDeleted = vi.fn();
    renderWithTooltipProvider(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted, onUpdated: vi.fn() }));

    fireEvent.click(within(rowFor("glm-5.2")).getByRole("button", { name: "Delete glm-5.2" }));

    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    expect(deleteAdminModelActionMock).not.toHaveBeenCalled();
    expect(onDeleted).not.toHaveBeenCalled();
  });

  it("removes a row from the parent on a successful delete, after confirming", async () => {
    deleteAdminModelActionMock.mockResolvedValue({ ok: true, data: undefined });
    const onDeleted = vi.fn();
    renderWithTooltipProvider(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted, onUpdated: vi.fn() }));

    fireEvent.click(within(rowFor("glm-5.2")).getByRole("button", { name: "Delete glm-5.2" }));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Delete" }));

    await waitFor(() => expect(deleteAdminModelActionMock).toHaveBeenCalledWith("u1"));
    await waitFor(() => expect(onDeleted).toHaveBeenCalledWith("u1"));
  });

  it("delete failure surfaces errorMessage inline and keeps the dialog open (test_catalogue_row_delete_is_icon_only_same_behavior)", async () => {
    deleteAdminModelActionMock.mockResolvedValue({ ok: false, error: "model is the active platform default" });
    renderWithTooltipProvider(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted: vi.fn(), onUpdated: vi.fn() }));

    fireEvent.click(within(rowFor("claude-opus-4-8")).getByRole("button", { name: "Delete claude-opus-4-8" }));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Delete" }));

    await waitFor(() => expect(screen.getByRole("alert").textContent).toMatch(/model is the active platform default/i));
    expect(screen.queryByRole("alertdialog")).toBeTruthy();
  });

  it("Delete is an icon-only destructive button with an aria-label, not text", () => {
    renderWithTooltipProvider(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted: vi.fn(), onUpdated: vi.fn() }));
    const del = within(rowFor("glm-5.2")).getByRole("button", { name: "Delete glm-5.2" });
    expect(del.className).toContain("bg-destructive");
    expect(del.querySelector("svg.lucide-trash")).toBeTruthy();
    // No visible "Delete" text label — the trigger is icon-only now.
    expect(del.textContent?.trim()).toBe("");
  });

  it("cancel on the dialog clears the target without invoking the delete action", async () => {
    const user = userEvent.setup();
    renderWithTooltipProvider(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted: vi.fn(), onUpdated: vi.fn() }));

    await user.click(within(rowFor("glm-5.2")).getByRole("button", { name: "Delete glm-5.2" }));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    await user.click(screen.getByRole("button", { name: /^cancel$/i }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(deleteAdminModelActionMock).not.toHaveBeenCalled();
  });
});
