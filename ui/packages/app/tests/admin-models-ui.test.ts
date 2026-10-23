import { ADMIN_CATALOGUE, SOURCE_URL, VERIFIED_AT } from "./helpers/catalogue-fixtures";
import React from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
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
import { type AdminModel } from "@/lib/api/admin_model_library";

function renderWithTooltipProvider(node: React.ReactElement) {
  return render(React.createElement(TooltipProvider, null, node));
}

// The catalogue is a design-system DataTable — scope a row by its (unique)
// model_id cell and walk up to its <tr>.
function fillModelSettings() {
  for (const [label, value] of [
    ["Catalogue display name", "GLM 5.2"], ["Context window (tokens)", "128000"],
    ["Context source URL", SOURCE_URL], ["Context verified at", VERIFIED_AT],
    ["Effective from", VERIFIED_AT], ["Price source URL", SOURCE_URL], ["Price verified at", VERIFIED_AT],
    ["Input ($/1M)", "0.55"], ["Cached ($/1M)", "0.14"], ["Output ($/1M)", "2.19"],
  ]) fireEvent.change(screen.getByLabelText(label!), { target: { value } });
}

function rowFor(modelId: string): HTMLElement {
  return screen.getByTitle(modelId).closest("tr")!;
}

async function loadedEditDialog(label: string) {
  await screen.findByRole("button", { name: "Catalogue details" });
  fireEvent.click(screen.getByRole("button", { name: "Catalogue details" }));
  fireEvent.click(screen.getByRole("button", { name: "Pricing" }));
  const field = await screen.findByLabelText(label);
  const dialog = field.closest('[role="dialog"]');
  if (!dialog) throw new Error("loaded edit field has no dialog owner");
  return within(dialog as HTMLElement);
}

const CATALOGUE: AdminModel[] = ADMIN_CATALOGUE;

beforeEach(() => vi.clearAllMocks());
afterEach(() => {
  vi.restoreAllMocks();
  cleanup();
});

it("AddModelDialog > describes the entry in user-facing pricing language", async () => {
  render(React.createElement(AddModelDialog, { onCreated: vi.fn() }));
  await userEvent.setup().click(screen.getByRole("button", { name: "Create model library" }));
  const dialog = within(screen.getByRole("dialog"));
  expect(dialog.getByText("Create a priced model users can choose. Prices are US dollars per 1M tokens.")).toBeTruthy();
});

it("AddModelDialog > should convert $/1M entry to integer nanos when creating a model", async () => {
  const user = userEvent.setup();
  createAdminModelActionMock.mockResolvedValue({ ok: true, data: { ...CATALOGUE[0] } });
  const onCreated = vi.fn();
  render(React.createElement(AddModelDialog, { onCreated }));

  await user.click(screen.getByRole("button", { name: "Create model library" }));
  fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "fireworks" } });
  fireEvent.change(screen.getByLabelText("Model"), { target: { value: "glm-5.2" } });
  fillModelSettings();
  fireEvent.change(screen.getByLabelText("Input ($/1M)"), { target: { value: "0.55" } });

  const dialog = screen.getByRole("dialog");
  fireEvent.submit(dialog.querySelector("form")!);

  await waitFor(() => expect(createAdminModelActionMock).toHaveBeenCalledTimes(1));
  const arg = createAdminModelActionMock.mock.calls[0]![0];
  expect(arg.provider).toBe("fireworks");
  expect(arg.prices[0].input_nanos_per_mtok).toBe("550000000");
  expect(arg.metadata.display_name).toBe("GLM 5.2");
  expect(onCreated).toHaveBeenCalledTimes(1);
});

it("AddModelDialog > should reject an empty provider and not call the create action", async () => {
  const user = userEvent.setup();
  render(React.createElement(AddModelDialog, { onCreated: vi.fn() }));
  await user.click(screen.getByRole("button", { name: "Create model library" }));
  await user.type(screen.getByLabelText("Model"), "glm-5.2");
  fillModelSettings();
  const dialog = screen.getByRole("dialog");
  await user.click(within(dialog).getByRole("button", { name: "Create" }));
  expect(await within(dialog).findByText("Name the provider")).toBeTruthy();
  expect(createAdminModelActionMock).not.toHaveBeenCalled();
});

it("AddModelDialog > surfaces the action error and keeps the dialog open when the create fails", async () => {
  createAdminModelActionMock.mockResolvedValue({ ok: false, error: "model exists" });
  const onCreated = vi.fn();
  render(React.createElement(AddModelDialog, { onCreated }));

  await userEvent.setup().click(screen.getByRole("button", { name: "Create model library" }));
  fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "fireworks" } });
  fireEvent.change(screen.getByLabelText("Model"), { target: { value: "glm-5.2" } });
  fillModelSettings();

  const dialog = screen.getByRole("dialog");
  fireEvent.submit(dialog.querySelector("form")!);

  await waitFor(() => expect(within(screen.getByRole("dialog")).getByText(/model exists/i)).toBeTruthy());
  expect(onCreated).not.toHaveBeenCalled();
});

it("AddModelDialog > closes from Cancel without creating a model", async () => {
  const user = userEvent.setup();
  render(React.createElement(AddModelDialog, { onCreated: vi.fn() }));

  await user.click(screen.getByRole("button", { name: "Create model library" }));
  const dialog = screen.getByRole("dialog");
  await user.click(within(dialog).getByRole("button", { name: /^cancel$/i }));

  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(createAdminModelActionMock).not.toHaveBeenCalled();
});

it("CatalogueList — rows + rates + empty state > shows exact prices, the model name and an accessible model id", () => {
  renderWithTooltipProvider(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted: vi.fn(), onUpdated: vi.fn() }));
  expect(screen.getByTestId("data-table")).toBeTruthy();
  expect(screen.getByText("0.55 / 0.14 / 2.19")).toBeTruthy();
  // The marketed name reads; the provider id a fleet config needs is the hover.
  const cell = screen.getByTitle("glm-5.2");
  expect(cell.textContent).toBe("GLM 5.2");
  // Actions still name the row by its id, which is what a reader acts on.
  expect(within(rowFor("glm-5.2")).getByRole("button", { name: "Edit glm-5.2" })).toBeTruthy();
  expect(within(rowFor("glm-5.2")).queryByRole("button", { name: "Copy model id: glm-5.2" })).toBeNull();
});

it("CatalogueList > keeps the summary compact with pricing changes available through Edit", () => {
  renderWithTooltipProvider(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted: vi.fn(), onUpdated: vi.fn() }));
  const rates = screen.getByText("0.55 / 0.14 / 2.19").closest("td")!;
  expect(rates.classList.contains("hidden")).toBe(false);
  expect(screen.getAllByRole("columnheader")).toHaveLength(5);
  expect(screen.queryByRole("columnheader", { name: "Context source" })).toBeNull();
  expect(screen.queryByRole("columnheader", { name: "Pricing details" })).toBeNull();
  expect(within(rowFor("glm-5.2")).getByRole("button", { name: "Edit glm-5.2" })).toBeTruthy();
});

it("CatalogueList — rows + rates + empty state > sorts each catalogue data column from its header arrow", () => {
  renderWithTooltipProvider(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted: vi.fn(), onUpdated: vi.fn() }));

  for (const name of ["Provider", "Model", "Context", "Rates $/1M (in / cached / out)"]) {
    fireEvent.click(screen.getByRole("button", { name }));
    expect(screen.getByRole("columnheader", { name }).getAttribute("aria-sort")).not.toBe("none");
  }
});

it("CatalogueList — rows + rates + empty state > sorts the Model column by the name it shows, not the id", () => {
  const base = CATALOGUE[0]!;
  // By id `accounts/...` sorts first; by the name a reader sees, Haiku 5.5 precedes Kimi K3.
  const kimi = "accounts/fireworks/models/kimi-k3";
  const haiku = "claude-haiku-5-5";
  const models = [
    { ...base, id: "k", model_id: kimi, metadata: { ...base.metadata, display_name: "Kimi K3" } },
    { ...base, id: "h", model_id: haiku, metadata: { ...base.metadata, display_name: "Haiku 5.5" } },
  ];
  renderWithTooltipProvider(React.createElement(CatalogueList, { models, activeDefault: null, onDeleted: vi.fn(), onUpdated: vi.fn() }));

  fireEvent.click(screen.getByRole("button", { name: "Model" }));
  expect(screen.getByRole("columnheader", { name: "Model" }).getAttribute("aria-sort")).toBe("ascending");
  const order = screen.getAllByTitle(/^(claude-haiku-5-5|accounts\/fireworks\/models\/kimi-k3)$/).map((cell) => cell.getAttribute("title"));
  expect(order).toEqual([haiku, kimi]);
});

it("CatalogueList — rows + rates + empty state > sorts the Provider column by the name it shows, not the slug", () => {
  const base = CATALOGUE[0]!;
  // By slug `hunyuan` sorts before `openai`; by the name a reader sees,
  // OpenAI precedes Tencent Hunyuan.
  const models = [
    { ...base, id: "t", provider: "hunyuan", model_id: "hunyuan-t1" },
    { ...base, id: "o", provider: "openai", model_id: "gpt-6" },
  ];
  renderWithTooltipProvider(React.createElement(CatalogueList, { models, activeDefault: null, onDeleted: vi.fn(), onUpdated: vi.fn() }));

  fireEvent.click(screen.getByRole("button", { name: "Provider" }));
  const [openai, hunyuan] = ["OpenAI", "Tencent Hunyuan"];
  const shown = screen.getAllByText(new RegExp(`^(${openai}|${hunyuan})$`)).map((badge) => badge.textContent);
  expect(shown).toEqual([openai, hunyuan]);
});

it("CatalogueList — rows + rates + empty state > shows the empty state when there are no models", () => {
  renderWithTooltipProvider(React.createElement(CatalogueList, { models: [], activeDefault: null, onDeleted: vi.fn(), onUpdated: vi.fn() }));
  expect(screen.getByText("No models yet")).toBeTruthy();
  expect(screen.getByText("Add a model to price it and make it the platform default.")).toBeTruthy();
  expect(screen.queryByTestId("data-table")).toBeNull();
});

it("CatalogueList — rows + rates + empty state > does not speculate on the editor from a coarse-pointer hover", () => {
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

it("CatalogueList — Delete (icon-only, confirm-gated) > does not delete on the icon click alone — a confirm dialog gates the irreversible action", async () => {
  const onDeleted = vi.fn();
  renderWithTooltipProvider(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted, onUpdated: vi.fn() }));

  fireEvent.click(within(rowFor("glm-5.2")).getByRole("button", { name: "Delete glm-5.2" }));

  await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
  expect(deleteAdminModelActionMock).not.toHaveBeenCalled();
  expect(onDeleted).not.toHaveBeenCalled();
});

it("CatalogueList — Delete (icon-only, confirm-gated) > removes a row from the parent on a successful delete, after confirming", async () => {
  deleteAdminModelActionMock.mockResolvedValue({ ok: true, data: undefined });
  const onDeleted = vi.fn();
  renderWithTooltipProvider(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted, onUpdated: vi.fn() }));

  fireEvent.click(within(rowFor("glm-5.2")).getByRole("button", { name: "Delete glm-5.2" }));
  await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
  fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Delete" }));

  await waitFor(() => expect(deleteAdminModelActionMock).toHaveBeenCalledWith("u1"));
  await waitFor(() => expect(onDeleted).toHaveBeenCalledWith("u1"));
});

it("CatalogueList — Delete (icon-only, confirm-gated) > delete failure surfaces errorMessage inline and keeps the dialog open (test_catalogue_row_delete_is_icon_only_same_behavior)", async () => {
  deleteAdminModelActionMock.mockResolvedValue({ ok: false, error: "model is the active platform default" });
  renderWithTooltipProvider(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted: vi.fn(), onUpdated: vi.fn() }));

  fireEvent.click(within(rowFor("claude-opus-4-8")).getByRole("button", { name: "Delete claude-opus-4-8" }));
  await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
  fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Delete" }));

  await waitFor(() => expect(screen.getByRole("alert").textContent).toMatch(/model is the active platform default/i));
  expect(screen.queryByRole("alertdialog")).toBeTruthy();
});

it("CatalogueList — Delete (icon-only, confirm-gated) > Delete is an icon-only button named by its aria-label, not text", () => {
  renderWithTooltipProvider(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted: vi.fn(), onUpdated: vi.fn() }));
  const del = within(rowFor("glm-5.2")).getByRole("button", { name: "Delete glm-5.2" });
  // No visible "Delete" text label — the trigger is icon-only now.
  expect(del.textContent?.trim()).toBe("");
});

it("CatalogueList — Delete (icon-only, confirm-gated) > cancel on the dialog clears the target without invoking the delete action", async () => {
  const user = userEvent.setup();
  renderWithTooltipProvider(React.createElement(CatalogueList, { models: CATALOGUE, activeDefault: null, onDeleted: vi.fn(), onUpdated: vi.fn() }));

  await user.click(within(rowFor("glm-5.2")).getByRole("button", { name: "Delete glm-5.2" }));
  await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
  await user.click(screen.getByRole("button", { name: /^cancel$/i }));
  await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  expect(deleteAdminModelActionMock).not.toHaveBeenCalled();
});

it("sorts unknown prices separately and preserves exact integer ordering in both directions", () => {
  const base = CATALOGUE[0]!;
  const unknown = {...base, id: "unknown", model_id: "unpriced", prices: []};
  const cheap = {...base, id: "cheap", model_id: "cheap", prices: base.prices.map(price => ({...price, input_nanos_per_mtok: "1"}))};
  const models = [base, unknown, cheap];
  renderWithTooltipProvider(React.createElement(CatalogueList, {models, activeDefault: null, onDeleted: vi.fn(), onUpdated: vi.fn()}));
  const order = () => screen.getAllByTitle(/^(glm-5.2|unpriced|cheap)$/).map(cell => cell.getAttribute("title"));
  const sort = screen.getByRole("button", {name: "Rates $/1M (in / cached / out)"});
  fireEvent.click(sort);
  expect(order()).toEqual([unknown.model_id, cheap.model_id, base.model_id]);
  fireEvent.click(sort);
  expect(order()).toEqual([base.model_id, cheap.model_id, unknown.model_id]);
});
