import React from "react";
import { beforeEach, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { TenantModelEntry } from "@/lib/types";
import { withTooltipProvider, listModelEntriesActionMock, registry } from "./models-registry-fixtures";

export const EDIT_MODEL = "claude-opus-5-5";

export async function clickRowAction(user: ReturnType<typeof userEvent.setup>, label: RegExp) {
  await user.click(screen.getByRole("button", { name: label }));
}

export async function renderEditDialog(target: TenantModelEntry) {
  const { default: EditModelEntryDialog } = await import(
    "../../app/(dashboard)/w/[workspaceId]/settings/models/components/EditModelEntryDialog"
  );
  const onOpenChange = vi.fn();
  const onSaved = vi.fn();
  const onCommitted = vi.fn();
  render(React.createElement(EditModelEntryDialog, {
    workspaceId: "ws_1", target, onOpenChange, onSaved, onCommitted,
  }));
  const dialog = await screen.findByRole("dialog");
  return { dialog, onOpenChange, onSaved, onCommitted, user: userEvent.setup() };
}

beforeEach(() => {
  listModelEntriesActionMock.mockResolvedValue({ ok: true, data: registry([]) });
});

