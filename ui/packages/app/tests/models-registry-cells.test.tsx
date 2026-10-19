// The workspace Models table's cells, rendered on their own: what a row reads
// as, apart from the table's paging and actions (models-registry-table.test.tsx).

import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import type { TenantModelEntry, TenantPlatformDefault } from "@/lib/types";
import { OLLAMA_PROVIDER } from "@/lib/api/model-library-types";
import { CATALOGUE_MODEL, PRICE } from "./helpers/catalogue-fixtures";
import { PRICE_UNKNOWN } from "@/lib/models/display";
import {
  ModelCell,
  ContextCell,
  RatesCell,
} from "../app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryCells";

afterEach(() => cleanup());

const ENTRY: TenantModelEntry = {
  id: "0190aaaa-aaaa-7aaa-aaaa-aaaaaaaaaaaa",
  model_id: "claude-sonnet-5-5",
  display_name: "Sonnet 5.5",
  secret_ref: "anthropic-prod",
  provider: "anthropic",
  kind: "provider_key",
  has_key: true,
  active: false,
  created_at: "2026-04-30T00:00:00Z",
};

/** The default's context window; any value, the cells under test ignore it. */
const DEFAULT_CONTEXT_TOKENS = 1_000_000;

/** A platform default the server sent no rates for. */
const UNPRICED_DEFAULT: TenantPlatformDefault = {
  provider: "anthropic",
  model: "claude-opus-5-5",
  display_name: "Opus 5.5",
  context_cap_tokens: DEFAULT_CONTEXT_TOKENS,
};

function renderCell(cell: React.ReactNode) {
  return render(<TooltipProvider>{cell}</TooltipProvider>);
}

describe("workspace model cells", () => {
  it("keeps Ollama pricing unknown even if a response supplies zero rates", () => {
    const entry = { ...ENTRY, provider: OLLAMA_PROVIDER, input_nanos_per_mtok: "0",
      cached_input_nanos_per_mtok: "0", output_nanos_per_mtok: "0" };
    renderCell(<RatesCell row={{ kind: "entry", entry }} platformDefault={null} libraryModels={[]} />);
    expect(screen.getByText(PRICE_UNKNOWN)).toBeTruthy();
    expect(screen.queryByText("0.00 / 0.00 / 0.00")).toBeNull();
  });

  it("the workspace model cell shows the name and keeps the id on hover", () => {
    renderCell(<ModelCell row={{ kind: "entry", entry: ENTRY }} platformDefault={null} />);
    expect(screen.getByTitle("claude-sonnet-5-5").textContent).toBe("Sonnet 5.5");
    expect(screen.getByRole("button", { name: "Copy model id: claude-sonnet-5-5" })).toBeTruthy();
  });

  it("the platform default reads its name, and its id stays reachable without a hover", () => {
    renderCell(<ModelCell row={{ kind: "default" }} platformDefault={UNPRICED_DEFAULT} />);
    expect(screen.getByTitle("claude-opus-5-5").textContent).toBe("Opus 5.5");
    expect(screen.getByRole("button", { name: "Copy model id: claude-opus-5-5" })).toBeTruthy();
  });

  it("a platform default priced nowhere reads Unknown", () => {
    renderCell(<RatesCell row={{ kind: "default" }} platformDefault={UNPRICED_DEFAULT} libraryModels={[]} />);
    expect(screen.getByText("Unknown")).toBeTruthy();
  });
});

it("shows null labels as exact IDs and leaves absent entry prices unknown", () => {
  const row = { kind: "entry" as const, entry: { ...ENTRY, display_name: null } };
  renderCell(<><ModelCell row={row} platformDefault={null} /><RatesCell row={row} platformDefault={null} libraryModels={[]} /></>);
  expect(screen.getByTitle(ENTRY.model_id).textContent).toBe(ENTRY.model_id);
  expect(screen.getByText(PRICE_UNKNOWN)).toBeTruthy();
  expect(screen.queryByText("0.00 / 0.00 / 0.00")).toBeNull();
});
it("shows catalogue evidence and the complete dated rate schedule for a registry entry", () => {
  const catalogue = { ...CATALOGUE_MODEL, id: ENTRY.model_id, provider: ENTRY.provider!, prices: [PRICE] };
  const row = { kind: "entry" as const, entry: ENTRY };
  renderCell(<><ContextCell row={row} platformDefault={null} libraryModels={[catalogue]} />
    <RatesCell row={row} platformDefault={null} libraryModels={[catalogue]} /></>);
  expect(screen.getByText("128,000")).toBeTruthy();
  expect(screen.getByRole("link", { name: "Source" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Price schedule (1)" }));
  expect(screen.getByText("Base tier")).toBeTruthy();
  expect(screen.getAllByText(PRICE_UNKNOWN)).toHaveLength(3);
});
