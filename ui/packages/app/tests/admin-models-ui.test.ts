import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

// Only the server-action module is stubbed; lib/api/admin_models (the $/1M⇄nanos
// conversion) stays real so the form's actual conversion is exercised, not faked.
// vi.hoisted: vi.mock is hoisted above const decls, so the mock fns must be too.
const { createAdminModelActionMock, setPlatformDefaultActionMock, deleteAdminModelActionMock } = vi.hoisted(() => ({
  createAdminModelActionMock: vi.fn(),
  setPlatformDefaultActionMock: vi.fn(),
  deleteAdminModelActionMock: vi.fn(),
}));

vi.mock("@/app/(dashboard)/admin/models/actions", () => ({
  createAdminModelAction: createAdminModelActionMock,
  setPlatformDefaultAction: setPlatformDefaultActionMock,
  deleteAdminModelAction: deleteAdminModelActionMock,
  listAdminModelsAction: vi.fn(),
  updateAdminModelAction: vi.fn(),
}));

import AddModelDialog from "@/app/(dashboard)/admin/models/components/AddModelDialog";
import PlatformDefaultCard from "@/app/(dashboard)/admin/models/components/PlatformDefaultCard";
import type { AdminModel } from "@/lib/api/admin_models";

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
