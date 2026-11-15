import type { BillingReport } from "@/lib/api/billing_report";
import type { BillingSearch } from "./report-query";
import { getBillingReport } from "@/lib/api/billing_report";
import { reportOptions } from "./report-query";

type Loaded = { ok: true; report: BillingReport } | { ok: false };

/**
 * One report request; model names come from the dashboard's shared label
 * store, not a catalogue read here. A report outage never becomes an empty
 * ledger.
 */
export async function loadBillingReport(token: string, query: BillingSearch): Promise<Loaded> {
  try {
    return { ok: true, report: await getBillingReport(token, reportOptions(query)) };
  } catch {
    return { ok: false };
  }
}
