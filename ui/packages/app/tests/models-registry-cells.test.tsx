// The workspace Models table's cells, rendered on their own: what a row reads
// as, apart from the table's paging and actions (models-registry-table.test.tsx).

import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import type { TenantModelEntry, TenantPlatformDefault } from "@/lib/types";
import {
  ModelCell,
  RatesCell,
} from "../app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryCells";

afterEach(() => cleanup());

const ENTRY: TenantModelEntry = {
  id: "0190aaaa-aaaa-7aaa-aaaa-aaaaaaaaaaaa",
  model_id: "claude-sonnet-5-5",
  secret_ref: "anthropic-prod",
  provider: "anthropic",
  kind: "provider_key",
  has_key: true,
  active: false,
  created_at: 1_777_507_200_000,
};

/** The default's context window; any value, the cells under test ignore it. */
const DEFAULT_CONTEXT_TOKENS = 1_000_000;

/** A platform default the server sent no rates for. */
const UNPRICED_DEFAULT: TenantPlatformDefault = {
  provider: "anthropic",
  model: "claude-opus-5-5",
  context_cap_tokens: DEFAULT_CONTEXT_TOKENS,
};

function renderCell(cell: React.ReactNode) {
  return render(<TooltipProvider>{cell}</TooltipProvider>);
}

describe("workspace model cells", () => {
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

  it("a platform default priced nowhere reads Rates unavailable", () => {
    renderCell(<RatesCell row={{ kind: "default" }} platformDefault={UNPRICED_DEFAULT} libraryModels={[]} />);
    expect(screen.getByText("Rates unavailable")).toBeTruthy();
  });
});
