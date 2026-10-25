import { PROVIDER_MODE } from "@/lib/types";
import { describe, expect, it } from "vitest";
import { formatChargeAmount } from "@/app/(dashboard)/settings/billing/lib/charges";
import { activityLabel, providerLabel, usageLabel } from "@/app/(dashboard)/settings/billing/lib/report-format";
import { BILLING_ACTIVITY_SETUP } from "@/lib/api/billing_report";
import { billingActivity } from "./helpers/billing-report";

describe("recorded billing amounts", () => {
  it.each([
    ["0", "$0.00"], ["1", "$0.000000001"], ["49999", "$0.000049999"],
    // pin test: literal is the contract
    ["50000", "$0.00005"], ["1000000", "$0.001"], ["7111111", "$0.007111111"],
    ["9000000000000000", "$9,000,000.00"], ["9007199254740993", "$9,007,199.254740993"],
  ])("keeps the served amount %s exact at the display boundary", (nanos, expected) => {
    expect(formatChargeAmount(nanos)).toBe(expected);
  });

  it("retains deleted-fleet names and uses setup without exposing a private draft", () => {
    expect(activityLabel(billingActivity())).toBe("Payment alerts");
    expect(activityLabel(billingActivity({ activity_type: BILLING_ACTIVITY_SETUP, fleet_name: "private draft title" }))).toBe("Setup chat");
    expect(activityLabel(billingActivity({ fleet_name: null, fleet_id: "retained-id" }))).toBe("retained-id");
    expect(activityLabel(billingActivity({ fleet_name: null, fleet_id: null }))).toBe("Fleet runtime");
  });

  it("describes quantities without inferring a price from them", () => {
    expect(usageLabel(820, 1040, 3000)).toBe("820 input · 1,040 output · 3s runtime");
    expect(usageLabel(0, 0, 1)).toBe("0 input · 0 output · 0.001s runtime");
  });

  it("distinguishes platform credits from a customer's provider bill", () => {
    expect(providerLabel([PROVIDER_MODE.platform])).toBeNull();
    expect(providerLabel([])).toBeNull();
    expect(providerLabel([PROVIDER_MODE.self_managed])).toContain("billed by your provider");
    expect(providerLabel([PROVIDER_MODE.platform, PROVIDER_MODE.self_managed])).toContain("recorded platform charges");
  });
});
