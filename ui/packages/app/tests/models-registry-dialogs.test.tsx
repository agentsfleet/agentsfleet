import React from "react";
import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { withTooltipProvider, listModelEntriesActionMock, updateModelEntryActionMock, deleteModelEntryActionMock, entry, registry, renderTable } from "./helpers/models-registry-fixtures";
import { clickRowAction } from "./helpers/models-registry-edit-fixtures";

describe("Row actions — dialog dismissal wiring (via the full table)", () => {
  it("closes the View details dialog on its own Close button", async () => {
    await renderTable(registry([entry({})]));
    const user = userEvent.setup();
    await clickRowAction(user, /view details for claude-sonnet-5/i);
    const dialog = await screen.findByRole("dialog");

    await user.click(within(dialog).getByRole("button", { name: /^close$/i }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("closes the Edit dialog via Cancel, wired through the table's own state", async () => {
    await renderTable(registry([entry({})]));
    const user = userEvent.setup();
    await clickRowAction(user, /edit claude-sonnet-5/i);
    const dialog = await screen.findByRole("dialog");

    await user.click(within(dialog).getByRole("button", { name: /^cancel$/i }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(updateModelEntryActionMock).not.toHaveBeenCalled();
  });

  it("dismissing the Remove confirm without confirming clears the pending target", async () => {
    await renderTable(registry([entry({})]));
    const user = userEvent.setup();
    await clickRowAction(user, /^remove claude-sonnet-5$/i);
    await screen.findByRole("alertdialog");

    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(deleteModelEntryActionMock).not.toHaveBeenCalled();
  });
});

describe("Row actions — View details", () => {
  it("renders nothing when no row is selected", async () => {
    const { default: ModelDetailsDialog } = await import(
      "../app/(dashboard)/w/[workspaceId]/settings/models/components/ModelDetailsDialog"
    );
    const { container } = render(
      React.createElement(ModelDetailsDialog, { target: null, onOpenChange: vi.fn() } as never),
    );
    expect(container.firstChild).toBeNull();
  });

  it("shows provider, endpoint, secret ref, and the In vault badge for a full custom-endpoint entry", async () => {
    const { default: ModelDetailsDialog } = await import(
      "../app/(dashboard)/w/[workspaceId]/settings/models/components/ModelDetailsDialog"
    );
    const target = entry({
      model_id: "vllm-model",
      secret_ref: "vllm-gateway",
      provider: "openai-compatible",
      kind: "custom_endpoint",
      base_url: "https://vllm.corp/v1",
      has_key: true,
    });
    render(withTooltipProvider(React.createElement(ModelDetailsDialog, { target, onOpenChange: vi.fn() } as never)));

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("https://vllm.corp/v1")).toBeTruthy();
    expect(within(dialog).getByText("vllm-gateway")).toBeTruthy();
    // "Has key: Yes" became a header badge; "Kind" is gone entirely.
    expect(within(dialog).getByText("In vault")).toBeTruthy();
    expect(within(dialog).queryByText("Yes")).toBeNull();
    expect(within(dialog).queryByText(/^kind$/i, { selector: "dt" })).toBeNull();
  });

  it("shows Unknown provider and the Keyless endpoint badge for a minimal entry", async () => {
    const { default: ModelDetailsDialog } = await import(
      "../app/(dashboard)/w/[workspaceId]/settings/models/components/ModelDetailsDialog"
    );
    const target = entry({ provider: undefined, kind: "custom_secret", base_url: undefined, has_key: false });
    render(withTooltipProvider(React.createElement(ModelDetailsDialog, { target, onOpenChange: vi.fn() } as never)));

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Unknown")).toBeTruthy();
    expect(within(dialog).getByText("Keyless endpoint")).toBeTruthy();
    expect(within(dialog).queryByText(/endpoint/i, { selector: "dt" })).toBeNull();
  });
});

describe("Row actions — Remove", () => {
  it("deletes a non-active entry only; the shared secret and sibling entry survive", async () => {
    const active = entry({ id: "e1", model_id: "claude-sonnet-5", active: true });
    const inactive = entry({ id: "e2", model_id: "claude-opus-4-8", active: false });
    deleteModelEntryActionMock.mockResolvedValue({ ok: true, data: undefined });
    listModelEntriesActionMock.mockResolvedValue({ ok: true, data: registry([active]) });
    await renderTable(registry([active, inactive]));

    const user = userEvent.setup();
    await clickRowAction(user, /^remove claude-opus-4-8$/i);

    const confirm = await screen.findByRole("alertdialog");
    await user.click(within(confirm).getByRole("button", { name: /^remove$/i }));

    await waitFor(() => expect(deleteModelEntryActionMock).toHaveBeenCalledWith("e2"));
    await waitFor(() => expect(listModelEntriesActionMock).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByTitle("claude-sonnet-5")).toBeTruthy());
  });

  it("surfaces a delete error inside the confirm dialog and still refreshes behind it", async () => {
    const inactive = entry({ id: "e2", model_id: "claude-opus-4-8", active: false });
    deleteModelEntryActionMock.mockResolvedValue({ ok: false, error: "conflict", errorCode: "UZ-MODELS-001" });
    listModelEntriesActionMock.mockResolvedValue({ ok: true, data: registry([inactive]) });
    await renderTable(registry([inactive]));

    const user = userEvent.setup();
    await clickRowAction(user, /^remove claude-opus-4-8$/i);
    const confirm = await screen.findByRole("alertdialog");
    await user.click(within(confirm).getByRole("button", { name: /^remove$/i }));

    await waitFor(() => expect(within(confirm).getByText(/conflict/i)).toBeTruthy());
    await waitFor(() => expect(listModelEntriesActionMock).toHaveBeenCalled());
  });

  it("disables Remove with a reason on the active entry's row", async () => {
    const active = entry({ id: "e1", model_id: "claude-sonnet-5", active: true });
    await renderTable(registry([active]));

    const user = userEvent.setup();
    const removeButton = screen.getByRole("button", { name: /cannot remove claude-sonnet-5 while it is active/i });
    expect(removeButton.hasAttribute("disabled")).toBe(true);

    await user.click(removeButton);
    expect(deleteModelEntryActionMock).not.toHaveBeenCalled();
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });
});
