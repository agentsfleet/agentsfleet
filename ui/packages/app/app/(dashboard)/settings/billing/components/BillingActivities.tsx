"use client";

import type { BillingReport } from "@/lib/api/billing_report";
import type { BillingDetail } from "../lib/report-detail";
import { formatCount } from "@/lib/events/run-figures-format";
import { useTransition } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger, EmptyState, PAGINATION_KIND, Pagination, Time } from "@agentsfleet/design-system";
import { useUrlCursorPages } from "@/lib/pagination/use-url-cursor-pages";
import { TABLE_PAGE_SIZE_OPTIONS } from "@/lib/pagination/cursor-trail";
import { formatChargeAmount } from "../lib/charges";
import { activityLabel, providerLabel, usageLabel } from "../lib/report-format";
import { BILLING_QUERY, activityKey, selectedActivity } from "../lib/report-query";
import { BillingActivityDetails } from "./BillingActivityDetails";

type Props = { report: BillingReport; pageSize: number; modelNames: Record<string, string>; detail: BillingDetail; selected: string };

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
  return <div className="space-y-lg" aria-busy={pending || feed.isLoading}>
    {report.activities.items.length === 0 ? <EmptyState title="No settled activity" description="Settled setup chats and fleet work will appear here. Try another period or filter." /> :
      <Accordion type="single" collapsible value={selected} onValueChange={open} disabled={pending || feed.isLoading}>
        {report.activities.items.map(activity => <AccordionItem key={activityKey(activity)} value={activityKey(activity)}>
          <AccordionTrigger>
            <div className="flex min-w-0 flex-1 flex-wrap items-center justify-between gap-lg pr-lg">
              <div className="min-w-0 space-y-xs"><p>{activityLabel(activity)}</p>
                <p className="text-label text-muted-foreground"><Time value={activity.recorded_at} format="absolute" /> · {formatCount(activity.totals.line_count)} charges</p>
                <p className="text-label text-muted-foreground">{activity.models.map(model => modelNames[model] ?? model).join(", ")}</p>
              </div>
              <span className="font-mono tabular-nums">{formatChargeAmount(activity.totals.charged_nanos)}</span>
            </div>
          </AccordionTrigger>
          <AccordionContent>
            <div className="space-y-lg">
              <p className="text-body-sm text-muted-foreground">{usageLabel(activity.totals.input_tokens, activity.totals.output_tokens, activity.totals.runtime_ms)}</p>
              <p className="text-body-sm text-muted-foreground">{providerLabel(activity.postures)}</p>
              {activityKey(activity) === selected ? <BillingActivityDetails detail={detail} modelNames={modelNames} /> : null}
            </div>
          </AccordionContent>
        </AccordionItem>)}
      </Accordion>}
    <Pagination aria-label="Activity pagination" kind={PAGINATION_KIND.page} page={feed.page} pageSize={pageSize} hasNext={feed.hasNext}
      totalLabel="activities" onPageChange={feed.goToPage} onPageSizeChange={feed.changePageSize}
      pageSizeOptions={TABLE_PAGE_SIZE_OPTIONS} isLoading={feed.isLoading || pending} />
  </div>;
}
