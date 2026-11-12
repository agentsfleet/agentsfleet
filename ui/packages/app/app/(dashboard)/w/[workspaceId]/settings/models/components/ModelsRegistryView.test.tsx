import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import type { ComponentProps } from "react";
import { ModelsRegistryView } from "./ModelsRegistryView";

afterEach(cleanup);

function props(): ComponentProps<typeof ModelsRegistryView> {
  return {entries: [{id: "model-a", workspace_id: "workspace-a", model_id: "claude-sonnet-5", display_name: "Sonnet", provider: "anthropic",
    secret_ref: "model-key", kind: "provider_key", has_key: true, active: false, created_at: "2026-11-12T00:00:00Z"}],
    platformDefault: null, workspaceNames: new Map([["workspace-a", "Main"]]), isDefaultLive: false,
    platformDefaultAvailable: false, pending: false, onSwitchDefault: vi.fn(), onSwitchEntry: vi.fn(), onView: vi.fn(),
    onEdit: vi.fn(), onRemove: vi.fn(), onIntent: vi.fn()};
}

it("keeps an unavailable platform default separate from workspace-backed models", () => {
  const data = props();
  render(<ModelsRegistryView {...data} />);
  expect(within(screen.getByLabelText("Platform default")).getByRole("button", {name: "Make active: platform default"})).toHaveProperty("disabled", true);
  expect(screen.getByText("model-key · Main")).toBeTruthy();
  fireEvent.click(within(screen.getByRole("region", {name: "Your models"})).getByRole("button", {name: "Make active: claude-sonnet-5"}));
  expect(data.onSwitchEntry).toHaveBeenCalledWith(data.entries[0], "workspace-a");
});

it("does not activate an entry without a credential workspace", () => {
  const data = props(); data.entries[0]!.workspace_id = undefined;
  render(<ModelsRegistryView {...data} />);
  const activate = within(screen.getByRole("region", {name: "Your models"})).getByRole("button", {name: "Make active: claude-sonnet-5"});
  expect(activate).toHaveProperty("disabled", true);
  fireEvent.click(activate);
  expect(data.onSwitchEntry).not.toHaveBeenCalled();
});

it("loads additional server rows only on request", () => {
  const load = vi.fn().mockResolvedValue(true);
  render(<ModelsRegistryView {...props()} onLoadMore={load} />);
  expect(load).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", {name: "Load more models"}));
  expect(load).toHaveBeenCalledOnce();
});
