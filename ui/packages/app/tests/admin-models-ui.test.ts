import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

// Only the server-action module is stubbed; lib/api/admin_models (the $/1M⇄nanos
// conversion) stays real so the form's actual conversion is exercised, not faked.
// vi.hoisted: vi.mock is hoisted above const decls, so the mock fns must be too.
const { createAdminModelActionMock, setPlatformDefaultActionMock, deleteAdminModelActionMock, captureProductEventMock } = vi.hoisted(() => ({
  createAdminModelActionMock: vi.fn(),
  setPlatformDefaultActionMock: vi.fn(),
  deleteAdminModelActionMock: vi.fn(),
  captureProductEventMock: vi.fn(),
}));

vi.mock("@/app/(dashboard)/admin/models/actions", () => ({
  createAdminModelAction: createAdminModelActionMock,
  setPlatformDefaultAction: setPlatformDefaultActionMock,
  deleteAdminModelAction: deleteAdminModelActionMock,
  listAdminModelsAction: vi.fn(),
  updateAdminModelAction: vi.fn(),
}));
vi.mock("@/lib/analytics/posthog", () => ({ captureProductEvent: captureProductEventMock }));

import AddModelDialog from "@/app/(dashboard)/admin/models/components/AddModelDialog";
import PlatformDefaultCard from "@/app/(dashboard)/admin/models/components/PlatformDefaultCard";
import CatalogueList from "@/app/(dashboard)/admin/models/components/CatalogueList";
import ModelsView from "@/app/(dashboard)/admin/models/components/ModelsView";
import { type AdminModel, OPENAI_COMPATIBLE_PROVIDER } from "@/lib/api/admin_models";
import { EVENTS } from "../lib/analytics/events";

// Open a design-system (Radix) Select and click one of its options. Mirrors the
// pointerDown→click→Enter sequence provider-selector.test.ts uses — Radix only
// mounts SelectContent (and its items) once the trigger is activated, so the
// option's render is uncovered until the select is actually opened.
function pickOption(trigger: HTMLElement, optionText: string) {
  fireEvent.pointerDown(trigger, { button: 0, pointerType: "mouse" });
  fireEvent.click(trigger);
  fireEvent.keyDown(trigger, { key: "Enter" });
  fireEvent.click(screen.getByText(optionText));
}

const CATALOGUE: AdminModel[] = [
  { uid: "u1", provider: "fireworks", model_id: "glm-5.2", context_cap_tokens: 128000, input_nanos_per_mtok: 550_000_000, cached_input_nanos_per_mtok: 140_000_000, output_nanos_per_mtok: 2_190_000_000 },
  { uid: "u2", provider: "anthropic", model_id: "claude-opus-4-8", context_cap_tokens: 200000, input_nanos_per_mtok: 15_000_000_000, cached_input_nanos_per_mtok: 1_500_000_000, output_nanos_per_mtok: 75_000_000_000 },
];

beforeEach(() => vi.clearAllMocks());
afterEach(() => cleanup());

describe("AddModelDialog", () => {
  it("should convert $/1M entry to integer nanos when creating a model", async () => {
    const user = userEvent.setup();
    createAdminModelActionMock.mockResolvedValue({ ok: true, data: { ...CATALOGUE[0] } });
    const onCreated = vi.fn();
    render(React.createElement(AddModelDialog, { onCreated }));

    await user.click(screen.getByRole("button", { name: "Add model" }));
    // Set values directly: happy-dom drops the intermediate invalid state when
    // typing decimals char-by-char into a type=number input. fireEvent.change
    // still drives react-hook-form's onChange — exactly what a paste would.
    fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "fireworks" } });
    fireEvent.change(screen.getByLabelText("Model id"), { target: { value: "glm-5.2" } });
    fireEvent.change(screen.getByLabelText("Input $/1M"), { target: { value: "0.55" } });

    const dialog = screen.getByRole("dialog");
    fireEvent.submit(dialog.querySelector("form")!);

    await waitFor(() => expect(createAdminModelActionMock).toHaveBeenCalledTimes(1));
    const arg = createAdminModelActionMock.mock.calls[0]![0];
    expect(arg.provider).toBe("fireworks");
    expect(arg.model_id).toBe("glm-5.2");
    expect(arg.input_nanos_per_mtok).toBe(550_000_000); // 0.55 $/1M → nanos
    expect(onCreated).toHaveBeenCalledTimes(1);
  });

  it("should reject an empty provider and not call the create action", async () => {
    const user = userEvent.setup();
    render(React.createElement(AddModelDialog, { onCreated: vi.fn() }));
    await user.click(screen.getByRole("button", { name: "Add model" }));
    await user.type(screen.getByLabelText("Model id"), "glm-5.2"); // provider left blank
    const dialog = screen.getByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Add model" }));
    // Zod min(1) on provider blocks submit; the action never fires.
    await new Promise((r) => setTimeout(r, 50));
    expect(createAdminModelActionMock).not.toHaveBeenCalled();
  });

  it("surfaces the action error and keeps the dialog open when the create fails", async () => {
    // No errorCode → presentError falls back to surfacing the raw server message.
    createAdminModelActionMock.mockResolvedValue({ ok: false, error: "model exists" });
    const onCreated = vi.fn();
    render(React.createElement(AddModelDialog, { onCreated }));

    await userEvent.setup().click(screen.getByRole("button", { name: "Add model" }));
    fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "fireworks" } });
    fireEvent.change(screen.getByLabelText("Model id"), { target: { value: "glm-5.2" } });

    const dialog = screen.getByRole("dialog");
    fireEvent.submit(dialog.querySelector("form")!);

    // The failure renders an error string (lines 79-81) without closing the dialog
    // or appending a row — onCreated never fires.
    await waitFor(() => expect(within(screen.getByRole("dialog")).getByText(/model exists/i)).toBeTruthy());
    expect(onCreated).not.toHaveBeenCalled();
  });
});

describe("PlatformDefaultCard", () => {
  it("should disable Save until a provider, model, and key are chosen", () => {
    render(React.createElement(PlatformDefaultCard, { models: CATALOGUE }));
    const save = screen.getByRole("button", { name: /Save default/ }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);
  });

  it("should derive the provider options from the catalogue (no free-text default)", () => {
    render(React.createElement(PlatformDefaultCard, { models: CATALOGUE }));
    // The card never renders a free-text provider/model input — only catalogue-backed
    // selects — so an uncatalogued default is unselectable by construction.
    expect(screen.queryByPlaceholderText(/free.?text/i)).toBeNull();
    expect(screen.getByLabelText("Default provider")).toBeTruthy();
    expect(screen.getByLabelText("Default model")).toBeTruthy();
  });
});

describe("PlatformDefaultCard — save flow", () => {
  it("stores the chosen provider/model/key, clears the key, and confirms on success", async () => {
    setPlatformDefaultActionMock.mockResolvedValue({ ok: true, data: { provider: "fireworks", model: "glm-5.2", active: true } });
    render(React.createElement(PlatformDefaultCard, { models: CATALOGUE }));

    pickOption(screen.getByLabelText("Default provider"), "fireworks"); // covers onValueChange (resets model) + provider option
    pickOption(screen.getByLabelText("Default model"), "glm-5.2"); // covers the catalogue-filtered model option
    const key = screen.getByLabelText("API key") as HTMLInputElement;
    fireEvent.change(key, { target: { value: "sk-secret" } });

    fireEvent.click(screen.getByRole("button", { name: /Save default/ }));

    await waitFor(() => expect(setPlatformDefaultActionMock).toHaveBeenCalledTimes(1));
    expect(setPlatformDefaultActionMock).toHaveBeenCalledWith({
      provider: "fireworks",
      model: "glm-5.2",
      api_key: "sk-secret",
      base_url: undefined, // not a custom endpoint → no base_url
    });
    await waitFor(() => expect(screen.getByText("Platform default updated.")).toBeTruthy());
    // The key is cleared from the field after a successful store (it lives in the vault now).
    expect(key.value).toBe("");
    // Telemetry: a platform_default_set product event fires with the priced
    // model + a non-custom flag, and never the key material.
    expect(captureProductEventMock).toHaveBeenCalledWith(EVENTS.platform_default_set, {
      provider: "fireworks",
      model: "glm-5.2",
      is_custom: false,
    });
  });

  it("surfaces the action error and leaves the key in place when activation fails", async () => {
    setPlatformDefaultActionMock.mockResolvedValue({ ok: false, error: "rate gate rejected the model" });
    render(React.createElement(PlatformDefaultCard, { models: CATALOGUE }));

    pickOption(screen.getByLabelText("Default provider"), "fireworks");
    pickOption(screen.getByLabelText("Default model"), "glm-5.2");
    fireEvent.change(screen.getByLabelText("API key"), { target: { value: "sk-secret" } });
    fireEvent.click(screen.getByRole("button", { name: /Save default/ }));

    await waitFor(() => expect(screen.getByText(/rate gate rejected the model/i)).toBeTruthy());
    expect(screen.queryByText("Platform default updated.")).toBeNull();
    // Telemetry sits on the success path only — a rejected save fires no event.
    expect(captureProductEventMock).not.toHaveBeenCalled();
  });

  it("requires a base URL for an openai-compatible endpoint and threads it into the save", async () => {
    setPlatformDefaultActionMock.mockResolvedValue({ ok: true, data: { provider: OPENAI_COMPATIBLE_PROVIDER, model: "glm-5.2", active: true } });
    const custom: AdminModel[] = [
      { uid: "c1", provider: OPENAI_COMPATIBLE_PROVIDER, model_id: "glm-5.2", context_cap_tokens: 128000, input_nanos_per_mtok: 0, cached_input_nanos_per_mtok: 0, output_nanos_per_mtok: 0 },
    ];
    render(React.createElement(PlatformDefaultCard, { models: custom }));

    pickOption(screen.getByLabelText("Default provider"), OPENAI_COMPATIBLE_PROVIDER);
    pickOption(screen.getByLabelText("Default model"), "glm-5.2");
    fireEvent.change(screen.getByLabelText("API key"), { target: { value: "sk-secret" } });

    // The base-URL field only renders for the openai-compatible provider, and Save
    // stays disabled until it is filled (canSave's isCustom branch).
    const baseUrl = screen.getByLabelText("Base URL");
    const save = screen.getByRole("button", { name: /Save default/ }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);
    fireEvent.change(baseUrl, { target: { value: "https://endpoint.example/v1" } });
    fireEvent.click(save);

    await waitFor(() => expect(setPlatformDefaultActionMock).toHaveBeenCalledTimes(1));
    expect(setPlatformDefaultActionMock).toHaveBeenCalledWith({
      provider: OPENAI_COMPATIBLE_PROVIDER,
      model: "glm-5.2",
      api_key: "sk-secret",
      base_url: "https://endpoint.example/v1",
    });
    // The custom-endpoint path flags the event is_custom: true (the other save
    // test covers the is_custom: false branch).
    expect(captureProductEventMock).toHaveBeenCalledWith(EVENTS.platform_default_set, {
      provider: OPENAI_COMPATIBLE_PROVIDER,
      model: "glm-5.2",
      is_custom: true,
    });
  });
});

describe("CatalogueList", () => {
  it("renders a priced row per catalogue model with $/1M rates", () => {
    render(React.createElement(CatalogueList, { models: CATALOGUE, onDeleted: vi.fn() }));
    expect(screen.getByText("Model rates · 2 models")).toBeTruthy();
    expect(screen.getByLabelText("fireworks glm-5.2 catalogue row")).toBeTruthy();
    // 550_000_000 nanos/Mtok → $0.55, two decimals.
    expect(screen.getByText("0.55 / 0.14 / 2.19")).toBeTruthy();
  });

  it("uses the singular noun for a one-model catalogue", () => {
    render(React.createElement(CatalogueList, { models: [CATALOGUE[0]!], onDeleted: vi.fn() }));
    expect(screen.getByText("Model rates · 1 model")).toBeTruthy();
  });

  it("shows the empty state when there are no models", () => {
    render(React.createElement(CatalogueList, { models: [], onDeleted: vi.fn() }));
    expect(screen.getByText("No models yet")).toBeTruthy();
    expect(screen.queryByText(/catalogue row/)).toBeNull();
  });

  it("does not delete on the row click alone — a confirm dialog gates the irreversible action", async () => {
    const onDeleted = vi.fn();
    render(React.createElement(CatalogueList, { models: CATALOGUE, onDeleted }));

    const row = screen.getByLabelText("fireworks glm-5.2 catalogue row");
    fireEvent.click(within(row).getByRole("button", { name: "Delete" }));

    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    expect(deleteAdminModelActionMock).not.toHaveBeenCalled();
    expect(onDeleted).not.toHaveBeenCalled();
  });

  it("removes a row from the parent on a successful delete, after confirming", async () => {
    deleteAdminModelActionMock.mockResolvedValue({ ok: true, data: undefined });
    const onDeleted = vi.fn();
    render(React.createElement(CatalogueList, { models: CATALOGUE, onDeleted }));

    const row = screen.getByLabelText("fireworks glm-5.2 catalogue row");
    fireEvent.click(within(row).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Delete" }));

    await waitFor(() => expect(deleteAdminModelActionMock).toHaveBeenCalledWith("u1"));
    await waitFor(() => expect(onDeleted).toHaveBeenCalledWith("u1"));
  });

  it("delete failure surfaces errorMessage inline and keeps the dialog open", async () => {
    deleteAdminModelActionMock.mockResolvedValue({ ok: false, error: "model is the active platform default" });
    const onDeleted = vi.fn();
    render(React.createElement(CatalogueList, { models: CATALOGUE, onDeleted }));

    const row = screen.getByLabelText("anthropic claude-opus-4-8 catalogue row");
    fireEvent.click(within(row).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Delete" }));

    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/model is the active platform default/i),
    );
    // The dialog is still open (not silently closed on failure) and the row
    // is untouched — matches SecretsList's "keeps the dialog open" pattern.
    expect(screen.queryByRole("alertdialog")).toBeTruthy();
    expect(onDeleted).not.toHaveBeenCalled();
  });

  it("renders the Delete button with the destructive variant, matching RunnerList's row-action pattern", () => {
    render(React.createElement(CatalogueList, { models: CATALOGUE, onDeleted: vi.fn() }));
    const row = screen.getByLabelText("fireworks glm-5.2 catalogue row");
    expect(within(row).getByRole("button", { name: "Delete" }).className).toContain("bg-destructive");
  });

  it("cancel on the dialog clears the target without invoking the delete action", async () => {
    const user = userEvent.setup();
    const onDeleted = vi.fn();
    render(React.createElement(CatalogueList, { models: CATALOGUE, onDeleted }));

    const row = screen.getByLabelText("fireworks glm-5.2 catalogue row");
    await user.click(within(row).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    await user.click(screen.getByRole("button", { name: /^cancel$/i }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(deleteAdminModelActionMock).not.toHaveBeenCalled();
    expect(onDeleted).not.toHaveBeenCalled();
  });
});

describe("ModelsView", () => {
  const initial = { models: CATALOGUE };

  it("renders the catalogue and the platform-default surface from the seeded list", () => {
    render(React.createElement(ModelsView, { initial }));
    expect(screen.getByText("Models")).toBeTruthy();
    expect(screen.getByText("Model rates · 2 models")).toBeTruthy();
    // The platform-default card reads the same catalogue for its picker.
    expect(screen.getByLabelText("Default provider")).toBeTruthy();
  });

  it("appends a newly created model to the catalogue without a round-trip", async () => {
    const created: AdminModel = {
      uid: "u3", provider: "moonshot", model_id: "kimi-k2.6", context_cap_tokens: 256000,
      input_nanos_per_mtok: 600_000_000, cached_input_nanos_per_mtok: 150_000_000, output_nanos_per_mtok: 2_300_000_000,
    };
    createAdminModelActionMock.mockResolvedValue({ ok: true, data: created });
    render(React.createElement(ModelsView, { initial }));

    await userEvent.setup().click(screen.getByRole("button", { name: "Add model" }));
    // Scope to the dialog: PlatformDefaultCard also renders a "Provider" label.
    const dialog = within(screen.getByRole("dialog"));
    fireEvent.change(dialog.getByLabelText("Provider"), { target: { value: "moonshot" } });
    fireEvent.change(dialog.getByLabelText("Model id"), { target: { value: "kimi-k2.6" } });
    fireEvent.submit(screen.getByRole("dialog").querySelector("form")!);

    // ModelsView's onCreated callback appends the row → count goes 2 → 3.
    await waitFor(() => expect(screen.getByText("Model rates · 3 models")).toBeTruthy());
    expect(screen.getByLabelText("moonshot kimi-k2.6 catalogue row")).toBeTruthy();
  });

  it("drops a deleted model from the catalogue", async () => {
    deleteAdminModelActionMock.mockResolvedValue({ ok: true, data: undefined });
    render(React.createElement(ModelsView, { initial }));

    const row = screen.getByLabelText("fireworks glm-5.2 catalogue row");
    fireEvent.click(within(row).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Delete" }));

    // ModelsView's onDeleted callback filters the row out → count goes 2 → 1.
    await waitFor(() => expect(screen.getByText("Model rates · 1 model")).toBeTruthy());
    expect(screen.queryByLabelText("fireworks glm-5.2 catalogue row")).toBeNull();
  });
});
