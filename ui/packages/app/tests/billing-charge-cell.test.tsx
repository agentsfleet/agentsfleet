import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";

vi.mock("lucide-react", () => ({
  ActivityIcon: () => React.createElement("svg", { "data-icon": "ActivityIcon" }),
  Loader2Icon: () => React.createElement("svg", { "data-icon": "Loader2Icon" }),
  ArrowUp: () => React.createElement("svg", { "data-icon": "ArrowUp" }),
  ArrowDown: () => React.createElement("svg", { "data-icon": "ArrowDown" }),
  ChevronsUpDown: () => React.createElement("svg", { "data-icon": "ChevronsUpDown" }),
}));


// The usage ledger's pager reads the cursor trail from the URL.
vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: vi.fn() }),
  usePathname: () => "/settings/billing",
  useSearchParams: () => new URLSearchParams(),
}));

import BillingUsageTab from "@/app/(dashboard)/settings/billing/components/BillingUsageTab";
import {
  formatChargeTimestamp,
  type ChargeRow,
} from "@/app/(dashboard)/settings/billing/lib/charges";
import { CHARGE_TYPE, PROVIDER_MODE } from "@/lib/types";
import {
  DELETED_AGENT_LABEL,
  agentDisplayName,
} from "@/lib/fleets/agent-label";

// A fixed epoch-ms instant so the ISO datetime attribute is deterministic.
const RECORDED_AT_MS = 1_700_000_000_000;
/** Fixture-only charge amount; nothing asserts on it. */
const CREDIT_DEDUCTED_NANOS = "1000000";
/**
 * A purged fleet's identifier, kept on the charge since slot 915.
 *
 * The callsign is derived from it by a pure hash, so the value only has to be
 * stable — the test reads the expected label back out of the same composer the
 * component uses rather than hard-coding a callsign the hash owns.
 */
const PURGED_FLEET_ID = "01990000-0000-7000-8000-00000000000a";
/** The name that fleet carried when the charge was written. */
const CAPTURED_NAME = "deploy-bot";

function charge(over: Partial<ChargeRow> = {}): ChargeRow {
  return {
    id: "tel_1",
    tenant_id: "t_1",
    workspace_id: "w_1",
    fleet_id: "z_1",
    fleet_name: "z_1",
    event_id: "evt_1",
    authoring_attempt_id: null,
    charge_type: CHARGE_TYPE.stage,
    posture: PROVIDER_MODE.platform,
    model: "kimi-k2.6",
    credit_deducted_nanos: CREDIT_DEDUCTED_NANOS,
    token_count_input: 820,
    token_count_output: 1040,
    wall_ms: 3000,
    recorded_at: new Date(RECORDED_AT_MS).toISOString(),
    ...over,
  };
}

afterEach(() => cleanup());

describe("BillingUsageTab charge cell", () => {
  it("test_billing_charge_cell_time_label", () => {
    const { container } = render(
      React.createElement(BillingUsageTab, {
        initialCharges: [charge()],
        initialCursor: null,
        pageSize: 25,
      }),
    );

    const time = container.querySelector("time");
    expect(time).not.toBeNull();
    // Visible text is still the approved ledger string ("MMM DD, YYYY · HH:MM").
    expect(time?.textContent).toBe(formatChargeTimestamp(new Date(RECORDED_AT_MS).toISOString()));
    // The datetime attribute is the canonical ISO instant.
    expect(time?.getAttribute("datetime")).toBe(
      new Date(RECORDED_AT_MS).toISOString(),
    );
  });

  it("renders only the retained callsign after fleet deletion", () => {
    const { container } = render(
      React.createElement(BillingUsageTab, {
        initialCharges: [
          charge({ fleet_id: PURGED_FLEET_ID, fleet_name: CAPTURED_NAME }),
        ],
        initialCursor: null,
        pageSize: 25,
      }),
    );

    const expected = agentDisplayName(PURGED_FLEET_ID);
    const label = container.querySelector(`[data-agent-name="${expected}"]`);
    expect(label).not.toBeNull();
    expect(container.textContent).not.toContain(CAPTURED_NAME);
    expect(label?.textContent).toContain("AGENT ");
    expect(container.textContent).not.toContain(DELETED_AGENT_LABEL);
  });

  // The orphan case: the server wrote the fleet's identifier as its name, and
  // the cell must not print that identifier where a name belongs.
  it("renders the callsign alone when the charge captured no name", () => {
    const { container } = render(
      React.createElement(BillingUsageTab, {
        initialCharges: [
          charge({ fleet_id: PURGED_FLEET_ID, fleet_name: PURGED_FLEET_ID }),
        ],
        initialCursor: null,
        pageSize: 25,
      }),
    );

    const expected = agentDisplayName(PURGED_FLEET_ID);
    expect(container.querySelector(`[data-agent-name="${expected}"]`)).not.toBeNull();
    expect(container.textContent).not.toContain(PURGED_FLEET_ID);
  });
  it("labels an unsaved authoring charge as Setup chat", () => {
    const {container} = render(<BillingUsageTab initialCharges={[
      charge({fleet_id: null, fleet_name: null, charge_type: CHARGE_TYPE.authoring}),
    ]} initialCursor={null} pageSize={25} />);
    expect(container.querySelector('[data-agent-name="Setup chat"]')?.textContent).toBe("Setup chat");
    expect(container.textContent).not.toContain(DELETED_AGENT_LABEL);
  });
});
