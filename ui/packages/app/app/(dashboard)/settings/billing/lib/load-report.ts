import type { BillingReport, BillingReportOptions } from "@/lib/api/billing_report";
import type { BillingDetail } from "./report-detail";
import { BILLING_DETAIL_STATE } from "./report-detail";
import type { BillingSearch } from "./report-query";
import { getBillingReport, listBillingActivityCharges } from "@/lib/api/billing_report";
import { getModelLibrary } from "@/lib/api/model_library";
import { BILLING_QUERY, DETAIL_PAGE_SIZE, activityKey, detailTrail, first, reportOptions } from "./report-query";

type Loaded = { ok: true; report: BillingReport; modelNames: Record<string, string>; selected: string; detail: BillingDetail; pageSize: number }
  | { ok: false };

/** A catalogue outage preserves ledger identity; a report outage never becomes an empty ledger. */
export async function loadBillingReport(token: string, query: BillingSearch): Promise<Loaded> {
  const options = reportOptions(query);
  try {
    // Fixed fan-out: one report and one optional catalogue read.
    const [report, catalogue] = await Promise.all([
      getBillingReport(token, options), getModelLibrary(token).catch(() => null),
    ]);
    const modelNames = Object.fromEntries(catalogue?.models.map(model => [model.id, model.display_name]) ?? []);
    const selected = report.activities.items.find(activity => activityKey(activity) === first(query[BILLING_QUERY.activity]));
    const detail = selected ? await loadDetails(token, report, selected, options, query) : { status: BILLING_DETAIL_STATE.closed } as const;
    return { ok: true, report, modelNames, selected: selected ? activityKey(selected) : "", detail, pageSize: options.limit };
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
