import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { DataTableColumn } from "@agentsfleet/design-system";
import { ModelsRegistryView } from "./ModelsRegistryView";
import type { RegistryRow } from "./ModelsRegistryCells";

afterEach(cleanup);

const ROWS: RegistryRow[] = [
  {kind: "default"},
  {kind: "entry", entry: {id: "model-a", model_id: "claude-sonnet-5", provider: "anthropic",
    secret_ref: "anthropic-key", kind: "provider_key", has_key: true, active: false, created_at: "1970-01-01T00:00:00Z"}},
];
const COLUMNS: DataTableColumn<RegistryRow>[] = [
  {key: "model", header: "Model", cell: (row) => row.kind === "default" ? "Platform default" : row.entry.model_id},
];

it("filters loaded model names and providers while preserving the default when cleared", async () => {
  const onSortChange = vi.fn();
  render(<ModelsRegistryView rows={ROWS} columns={COLUMNS} platformDefault={null} sort={null} onSortChange={onSortChange} pending={false} />);
  const input = screen.getByRole("searchbox", {name: "Search loaded models"});
  fireEvent.change(input, {target: {value: "ANTHROPIC"}});
  await waitFor(() => expect(screen.queryByText("Platform default")).toBeNull());
  expect(screen.getByText("claude-sonnet-5")).toBeTruthy();
  fireEvent.change(input, {target: {value: "missing"}});
  await screen.findByText("No matching models");
  fireEvent.click(screen.getByRole("button", {name: "Clear search"}));
  await screen.findByText("Platform default");
  expect(screen.getByText("claude-sonnet-5")).toBeTruthy();
  expect(onSortChange).not.toHaveBeenCalled();
});

it.each([false, true])("pages cached models without a request and resets page size (more available: %s)", async (more) => {
  const load = vi.fn();
  const rows: RegistryRow[] = Array.from({length: 30}, (_, index) => ({kind: "entry", entry: {
    id: `model-${index}`, model_id: `choice-${index}`, provider: "anthropic", secret_ref: "key",
    kind: "provider_key", has_key: true, active: false, created_at: "1970-01-01T00:00:00Z",
  }}));
  render(<ModelsRegistryView rows={rows} onLoadMore={more ? load : undefined} columns={COLUMNS} platformDefault={null} sort={null} onSortChange={() => {}} pending={false} />);
  fireEvent.click(screen.getByRole("button", {name: "Next page"}));
  expect(screen.getByText("choice-25")).toBeTruthy();
  expect(screen.queryByText("choice-0")).toBeNull();
  const trigger = screen.getByRole("combobox", {name: "Rows per page"});
  fireEvent.click(trigger); fireEvent.keyDown(trigger, {key: "Enter"});
  fireEvent.click(await screen.findByRole("option", {name: "50"}));
  expect(screen.getByText("choice-0")).toBeTruthy();
  expect(screen.getByText("choice-29")).toBeTruthy();
  expect(load).not.toHaveBeenCalled();
  expect(screen.getByRole("button", {name: "Next page"})).toHaveProperty("disabled", !more);
});
