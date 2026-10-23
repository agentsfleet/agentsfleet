import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { TooltipProvider } from "@agentsfleet/design-system";
import { mockAuthOnce } from "./helpers/dashboard-mocks";
import { NANOS_PER_USD } from "@/lib/types";
import { MS_PER_SECOND } from "@/lib/utils";

vi.mock("@clerk/nextjs/server", async () =>
  (await import("./helpers/dashboard-mocks")).clerkServerMock());
vi.mock("next/navigation", async () =>
  (await import("./helpers/dashboard-mocks")).nextNavigationMock());

import BillingSettingsPage from "@/app/(dashboard)/settings/billing/page";

const MODEL = "accounts/fireworks/models/minimax-m3p1-flash";
const DISPLAY_NAME = "MiniMax M3.1 Flash";
const TIMESTAMP = "2026-10-23T10:00:00Z";
const CHARGE_NANOS = "100000";
const CONTEXT_TOKENS = 1_048_576;
const CHARGE = {
  id: "charge-1", tenant_id: "account-1", workspace_id: "workspace-1",
  fleet_id: "fleet-1", fleet_name: "indy-preview", event_id: "event-1",
  authoring_attempt_id: null, charge_type: "stage", posture: "platform", model: MODEL,
  credit_deducted_nanos: CHARGE_NANOS, token_count_input: 40, token_count_output: 10,
  wall_ms: MS_PER_SECOND, recorded_at: TIMESTAMP,
};
const CATALOGUE = {
  version: "fixture", total: 1, next_cursor: null,
  models: [{id: MODEL, provider: "fireworks", display_name: DISPLAY_NAME,
    advanced: false, context_cap_tokens: CONTEXT_TOKENS, context_source_url: "https://example.test/model",
    context_verified_at: TIMESTAMP, prices: [], input_nanos_per_mtok: "1",
    cached_input_nanos_per_mtok: "1", output_nanos_per_mtok: "1"}],
};

function backend(catalogue: unknown, catalogueStatus = 200, ledgerStatus = 200): void {
  vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request) => {
    const path = new URL(typeof input === "string" || input instanceof URL ? input : input.url,
      "https://example.test").pathname;
    if (path.endsWith("/models")) return Response.json(catalogue, {status: catalogueStatus});
    if (path.endsWith("/charges")) return Response.json({items: [CHARGE], next_cursor: null}, {status: ledgerStatus});
    if (path.endsWith("/billing")) return Response.json({balance_nanos: String(NANOS_PER_USD),
      updated_at: TIMESTAMP, is_exhausted: false, exhausted_at: null});
    throw new Error(`Unexpected test request: ${path}`);
  }));
}

async function markup(): Promise<string> {
  return renderToStaticMarkup(React.createElement(TooltipProvider, null, await BillingSettingsPage()));
}

beforeEach(() => {mockAuthOnce({token: "fixture-token"});});
afterEach(() => {vi.unstubAllGlobals();vi.clearAllMocks();});

describe("billing catalogue enrichment", () => {
  it("uses the published display name and preserves the ledger amount and fleet", async () => {
    backend(CATALOGUE);
    const html = await markup();
    expect(html).toContain(DISPLAY_NAME);
    expect(html).toContain("indy-preview");
    expect(html).toContain("−$0.0001");
    expect(html).toContain("40 input · 10 output");
  });

  it("keeps the successful ledger and exact model identifier when the catalogue refuses access", async () => {
    backend({error: "Unavailable"}, 403);
    const html = await markup();
    expect(html).toContain(MODEL);
    expect(html).toContain("indy-preview");
    expect(html).toContain("−$0.0001");
    expect(html).not.toContain(DISPLAY_NAME);
  });

  it("keeps the ledger visible when catalogue data is malformed", async () => {
    backend({...CATALOGUE, models: [{id: MODEL}]});
    const html = await markup();
    expect(html).toContain(MODEL);
    expect(html).toContain("−$0.0001");
  });

  it("propagates a failed ledger even when the catalogue loaded", async () => {
    backend(CATALOGUE, 200, 403);
    await expect(markup()).rejects.toMatchObject({name: "ApiError", status: 403});
  });
});
