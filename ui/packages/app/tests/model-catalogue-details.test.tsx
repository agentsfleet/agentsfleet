import React from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Button, TooltipProvider } from "@agentsfleet/design-system";
import type { LibraryModel } from "@/lib/api/model-library-types";
import type { TenantModelEntry } from "@/lib/types";
import { CATALOGUE_MODEL, PRICE, SOURCE_URL, VERIFIED_AT } from "./helpers/catalogue-fixtures";

const { read, router } = vi.hoisted(() => ({ read: vi.fn(), router: { push: vi.fn() } }));
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/actions", () => ({ getModelLibraryAction: read }));
vi.mock("next/navigation", () => ({ useRouter: () => router }));
import { ModelCatalogueProvider, useModelCatalogue } from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelCatalogueProvider";
import ModelDetailsDialog from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelDetailsDialog";
import ProviderModelSelect from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/ProviderModelSelect";

const ENTRY: TenantModelEntry = { id: "entry", model_id: CATALOGUE_MODEL.id, display_name: null,
  provider: CATALOGUE_MODEL.provider, secret_ref: "provider-key", kind: "provider_key", has_key: true,
  active: false, created_at: VERIFIED_AT };
const TIER = { ...PRICE, prompt_tokens_above: 200000, conditions: "Long prompts", cache_write_nanos_per_mtok: "0",
  cache_write_5m_nanos_per_mtok: "1", cache_write_1h_nanos_per_mtok: "9223372036854775807" };
const COMPLETE = { ...CATALOGUE_MODEL, prices: [PRICE, TIER] };
function page(models: LibraryModel[], next_cursor: string | null = null) {
  return { ok: true as const, data: { models, version: "v", total: null, next_cursor } };
}
function Intent({ children }: React.PropsWithChildren) {
  const { preload } = useModelCatalogue();
  return <><Button type="button" onClick={preload}>Load catalogue</Button>{children}</>;
}
function setup(children: React.ReactNode) {
  render(<TooltipProvider><ModelCatalogueProvider><Intent>{children}</Intent></ModelCatalogueProvider></TooltipProvider>);
}
async function load() {
  await act(async () => { screen.getByRole("button", { name: "Load catalogue" }).click(); });
}

beforeEach(() => vi.resetAllMocks());
afterEach(cleanup);

it("shows the selected model's verified context and complete exact price schedule", async () => {
  read.mockResolvedValueOnce(page([COMPLETE]));
  setup(<ProviderModelSelect id="complete-picker" model={COMPLETE.id} provider={COMPLETE.provider} onModelChange={vi.fn()} />);
  await load();
  expect(screen.getByText("128,000")).toBeTruthy();
  expect(document.querySelector(`time[datetime="${VERIFIED_AT.replace("Z", ".000Z")}"]`)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Price schedule (2)" }));
  expect(screen.getByText("Long prompts")).toBeTruthy();
  expect(screen.getByText("0.000000001")).toBeTruthy();
  expect(screen.getByText("9223372036.854775807")).toBeTruthy();
  expect(screen.getByText("0.00")).toBeTruthy();
  expect(screen.getAllByText("Unknown").length).toBeGreaterThan(0);
  expect(screen.getAllByRole("link", { name: "Source" }).every((link) => link.getAttribute("href") === SOURCE_URL)).toBe(true);
});

it("fills registry details from a later page and preserves the selected entry", async () => {
  read.mockResolvedValueOnce(page([], "next-page")).mockResolvedValueOnce(page([COMPLETE]));
  setup(<ModelDetailsDialog target={ENTRY} onOpenChange={vi.fn()} />);
  await load();
  expect(screen.getByText(/Catalogue evidence and price schedule are unavailable/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Load more models" }));
  await screen.findByRole("button", { name: "Price schedule (2)" });
  expect(screen.getByRole("dialog").textContent).toContain(ENTRY.model_id);
  expect(read).toHaveBeenLastCalledWith(false, "next-page");
});

it("shows a loading state then retries a refused registry catalogue read", async () => {
  const pending = Promise.withResolvers<ReturnType<typeof page>>();
  read.mockReturnValueOnce(pending.promise)
    .mockResolvedValueOnce({ ok: false, status: 422, error: "Invalid catalogue date range" })
    .mockResolvedValueOnce(page([COMPLETE]));
  setup(<ModelDetailsDialog target={ENTRY} onOpenChange={vi.fn()} />);
  await load();
  expect(screen.getByText("Loading catalogue details…")).toBeTruthy();
  await act(async () => { pending.reject(new Error("offline")); });
  fireEvent.click(screen.getByRole("button", { name: "Try again" }));
  await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("Invalid catalogue date range"));
  fireEvent.click(screen.getByRole("button", { name: "Try again" }));
  await screen.findByRole("button", { name: "Price schedule (2)" });
});

it("reads advanced registry evidence only after explicit opt-in", async () => {
  read.mockResolvedValueOnce(page([])).mockResolvedValueOnce(page([{ ...COMPLETE, advanced: true }]));
  setup(<ModelDetailsDialog target={ENTRY} onOpenChange={vi.fn()} />);
  await load();
  expect(read).toHaveBeenCalledTimes(1);
  fireEvent.click(screen.getByRole("switch", { name: "Show advanced models" }));
  await screen.findByText("Advanced model");
  expect(read.mock.calls).toEqual([[false, null], [true, null]]);
});
