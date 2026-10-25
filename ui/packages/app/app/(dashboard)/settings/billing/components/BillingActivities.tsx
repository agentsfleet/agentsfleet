"use client";

import type { BillingActivity, BillingReport } from "@/lib/api/billing_report";
import type { BillingDetail } from "../lib/report-detail";
import type { ReactNode } from "react";
import { formatCompactCount } from "@/lib/events/run-figures-format";
import { useTransition } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { DATA_TABLE_SURFACE, DataTable, EMPTY_STATE_SURFACE, EmptyState, PAGINATION_KIND, Time, formatTimeCompactUtc, type DataTableColumn } from "@agentsfleet/design-system";
import { useUrlCursorPages } from "@/lib/pagination/use-url-cursor-pages";
import { TABLE_PAGE_SIZE_OPTIONS } from "@/lib/pagination/cursor-trail";
import { formatBalance } from "@/lib/money";
import { activityLabel, runtimeLabel } from "../lib/report-format";
import { BILLING_ACTIVITY_SETUP } from "@/lib/api/billing-report-values";
import { BILLING_QUERY, activityKey, selectedActivity } from "../lib/report-query";
import { BillingActivityDetails } from "./BillingActivityDetails";

type Props = { report: BillingReport; pageSize: number; modelNames: Record<string, string>; detail: BillingDetail; selected: string };
const COLUMN = { activity: "Activity", model: "Model", tokens: "Tokens", runtime: "Runtime", charged: "Charged" } as const;

export function BillingActivities({ report, pageSize, modelNames, detail, selected }: Props) {
  const router = useRouter();
  const params = useSearchParams();
  const pathname = usePathname();
  const [pending, startTransition] = useTransition();
  const feed = useUrlCursorPages(report.activities.next_cursor, pageSize, { [BILLING_QUERY.through]: report.through_at });
  const open = (value: string) => {
    const next = selectedActivity(new URLSearchParams(params.toString()), value, report.through_at);
    startTransition(() => router.push(`${pathname}?${next}`, { scroll: false }));
  };
  return <DataTable columns={activityColumns(modelNames)} rows={report.activities.items} rowKey={activityKey} caption="Billing activity"
    surface={DATA_TABLE_SURFACE.plain} isLoading={pending || feed.isLoading} stickyHeader={false}
    className="billing-activity-table [&_table]:font-system [&_table]:text-eyebrow [&_table]:table-fixed [&_th]:h-auto [&_th]:py-2.5 [&_th]:font-normal [&_th]:text-eyebrow [&_th:first-child]:w-2/5 [&_th:nth-child(2)]:w-1/4 [&_td:not([colspan])]:h-auto [&_td:not([colspan])]:py-sm [&_td]:break-words"
    empty={<EmptyState title="No settled activity" description="Settled setup chats and fleet work will appear here. Try another period or filter." surface={EMPTY_STATE_SURFACE.plain} />}
    rowDisclosure={{expandedRow: selected, onExpandedRowChange: open,
      label: activity => `Charges for ${activityLabel(activity)} at ${activity.recorded_at}`,
      content: activity => <BillingActivityDetails activity={activity} detail={detail} modelNames={modelNames} />}}
    pagination={{"aria-label": "Activity pagination", kind: PAGINATION_KIND.page, page: feed.page, pageSize, hasNext: feed.hasNext,
      total: report.activities.total ?? undefined, totalLabel: "activities", onPageChange: feed.goToPage, onPageSizeChange: feed.changePageSize,
      pageSizeOptions: TABLE_PAGE_SIZE_OPTIONS, isLoading: feed.isLoading || pending}} />;
}

function activityColumns(modelNames: Record<string, string>): DataTableColumn<BillingActivity>[] {
  return [
    {key: "activity", header: COLUMN.activity, primary: true, cell: activity => <div>
      <p className="font-medium">{activityLabel(activity)}</p><p className="text-eyebrow font-normal text-muted-foreground">
        <Time value={activity.recorded_at} label={formatTimeCompactUtc(activity.recorded_at)} /> · {activity.activity_type === BILLING_ACTIVITY_SETUP ? "Setup turn" : "Fleet run"}
      </p></div>},
    {key: "model", header: COLUMN.model, cell: activity => mobileValue(COLUMN.model, activity.models.map(model => modelNames[model] ?? model).join(", "))},
    {key: "tokens", header: COLUMN.tokens, numeric: true,
      cell: activity => mobileValue(COLUMN.tokens, formatCompactCount(activity.totals.input_tokens + activity.totals.output_tokens))},
    {key: "runtime", header: COLUMN.runtime, numeric: true, cell: activity => mobileValue(COLUMN.runtime, runtimeLabel(activity.totals.runtime_ms))},
    {key: "amount", header: COLUMN.charged, numeric: true,
      cell: activity => <span className="tabular-nums">{formatBalance(activity.totals.charged_nanos)}</span>},
  ];
}

function mobileValue(label: string, value: ReactNode) {
  return <><span className="billing-mobile-label text-eyebrow text-muted-foreground">{label}</span>{value}</>;
}
