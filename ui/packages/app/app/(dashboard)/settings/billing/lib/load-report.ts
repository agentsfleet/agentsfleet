import type { BillingReport, BillingReportOptions } from "@/lib/api/billing_report";
import type { BillingDetail } from "./report-detail";
import { BILLING_DETAIL_STATE } from "./report-detail";
import type { BillingSearch } from "./report-query";
import { getBillingReport, listBillingActivityCharges } from "@/lib/api/billing_report";
import { BILLING_QUERY, DETAIL_PAGE_SIZE, activityKey, detailTrail, first, reportOptions } from "./report-query";

type Loaded = { ok: true; report: BillingReport; selected: string; detail: BillingDetail; pageSize: number }
  | { ok: false };

/**
 * One report request; model names come from the dashboard's shared label
 * store, not a catalogue read here. Charges are read only for an activity the
 * URL opens. A report outage never becomes an empty ledger.
 */
export async function loadBillingReport(token: string, query: BillingSearch): Promise<Loaded> {
  const options = reportOptions(query);
  try {
    const report = await getBillingReport(token, options);
    const selected = report.activities.items.find(activity => activityKey(activity) === first(query[BILLING_QUERY.activity]));
    const detail = selected ? await loadDetails(token, report, selected, options, query) : { status: BILLING_DETAIL_STATE.closed } as const;
    return { ok: true, report, selected: selected ? activityKey(selected) : "", detail, pageSize: options.limit };
  } catch {
    return { ok: false };
  }
}

async function loadDetails(
  token: string, report: BillingReport, activity: BillingReport["activities"]["items"][number],
  options: BillingReportOptions, query: BillingSearch,
): Promise<BillingDetail> {
  const trail = detailTrail(query[BILLING_QUERY.detailCursor]);
  try {
    const charges = await listBillingActivityCharges(token, activity.id, {
      ...options, activity_type: activity.activity_type, through_at: report.through_at,
      limit: DETAIL_PAGE_SIZE, starting_after: trail.at(-1),
    });
    return { status: BILLING_DETAIL_STATE.ready, charges, page: trail.length + 1 };
  } catch {
    return { status: BILLING_DETAIL_STATE.unavailable };
  }
}
