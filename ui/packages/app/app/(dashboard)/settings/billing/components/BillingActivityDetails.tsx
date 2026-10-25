"use client";

import type { BillingActivity, BillingCharge } from "@/lib/api/billing_report";
import type { BillingDetail } from "../lib/report-detail";
import { formatCount } from "@/lib/events/run-figures-format";
import { MS_PER_SECOND } from "@/lib/utils";
import { BILLING_DETAIL_STATE } from "../lib/report-detail";
import { BILLING_ACTIVITY_SETUP } from "@/lib/api/billing-report-values";
import { useTransition } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { Button, DescriptionDetails, DescriptionList, DescriptionTerm, EMPTY_STATE_SURFACE, EmptyState, PAGINATION_KIND, Pagination, UnavailableState } from "@agentsfleet/design-system";
import { formatChargeAmount } from "../lib/charges";
import { DETAIL_PAGE_SIZE, nextDetailPage } from "../lib/report-query";
import { providerLabel, runtimeSecondsLabel } from "../lib/report-format";

type Props = { activity: BillingActivity; detail: BillingDetail; modelNames: Record<string, string> };
const ROW_CLASS = "grid grid-cols-[minmax(0,1fr)_auto] items-baseline gap-lg py-xs";

export function BillingActivityDetails({ activity, detail, modelNames }: Props) {
  const router = useRouter();
  const params = useSearchParams();
  const pathname = usePathname();
  const [pending, startTransition] = useTransition();
  if (detail.status === BILLING_DETAIL_STATE.closed) return null;
  if (detail.status === BILLING_DETAIL_STATE.unavailable) return <UnavailableState title="Charges unavailable"
    description="This activity's individual charges could not be loaded. Its settled total remains in the report."
    actions={<Button variant="outline" size="sm" disabled={pending} onClick={() => startTransition(() => router.refresh())}>Retry charges</Button>} />;
  const turnPage = (target: number) => {
    const next = nextDetailPage(new URLSearchParams(params.toString()), target, detail.charges.next_cursor);
    if (next) startTransition(() => router.push(`${pathname}?${next}`, { scroll: false }));
  };
  const paged = detail.page > 1 || detail.charges.next_cursor !== null;
  const providerNote = providerLabel(activity.postures);
  return <div className="font-system text-eyebrow font-normal" aria-busy={pending}>
    <div className="mb-sm flex items-baseline justify-between gap-lg font-medium">
      <span>{activity.activity_type === BILLING_ACTIVITY_SETUP ? "Setup chat" : "Fleet run"} charge details</span>
      <span className="max-w-1/2 break-words text-right">{activity.models.map(model => modelNames[model] ?? model).join(", ")}</span>
    </div>
    <DescriptionList layout="stacked" aria-label="Activity charge details" className="space-y-0 text-eyebrow">
      <DetailLine label="Input / output tokens · includes compaction" value={`${formatCount(activity.totals.input_tokens)} / ${formatCount(activity.totals.output_tokens)}`} />
      {detail.charges.items.map(charge => <DetailLine key={charge.id} label={chargeLabel(charge, activity.models.length > 1 ? modelNames[charge.model] ?? charge.model : null)} value={formatChargeAmount(charge.charged_nanos)} />)}
      <DetailLine label="Total charged" value={formatChargeAmount(activity.totals.charged_nanos)} total />
    </DescriptionList>
    {detail.charges.items.length === 0 ? <EmptyState title="No matching settled charges" description="This activity has no settled lines for these filters." surface={EMPTY_STATE_SURFACE.plain} /> : null}
    {activity.totals.unpriced_tokens > 0 ? <p className="mt-sm text-muted-foreground">{formatCount(activity.totals.unpriced_tokens)} tokens awaiting a platform price</p> : null}
    {detail.charges.items.some(charge => charge.runtime_ms > 0 && charge.input_tokens + charge.output_tokens > 0) ?
      <p className="mt-sm text-muted-foreground">Separate historical model and runtime prices are unavailable for combined charges.</p> : null}
    {providerNote ? <p className="mt-sm text-muted-foreground">{providerNote}</p> : null}
    {paged ? <p className="mt-sm text-muted-foreground">Showing charge page {detail.page}. Total charged covers the entire activity.</p> : null}
    {paged ? <Pagination kind={PAGINATION_KIND.page} page={detail.page} pageSize={DETAIL_PAGE_SIZE}
      hasNext={detail.charges.next_cursor !== null} onPageChange={turnPage} isLoading={pending}
      className="mt-md border-t border-border" aria-label="Charge pagination" /> : null}
  </div>;
}

function DetailLine({ label, value, total = false }: { label: string; value: string; total?: boolean }) {
  return <div className={`${ROW_CLASS}${total ? " mt-xs border-t border-border font-medium" : ""}`}>
    <DescriptionTerm className="text-foreground">{label}</DescriptionTerm>
    <DescriptionDetails className="text-right tabular-nums">{value}</DescriptionDetails>
  </div>;
}

function chargeLabel(charge: BillingCharge, model: string | null): string {
  const inference = charge.input_tokens + charge.output_tokens + charge.unpriced_tokens > 0;
  const kind = charge.runtime_ms > 0 ? inference ? "Inference & runtime" : "Runtime" : inference ? "Inference" : "Recorded charge";
  const unit = charge.runtime_ms === MS_PER_SECOND ? "second" : "seconds";
  const duration = charge.runtime_ms > 0 ? ` · ${runtimeSecondsLabel(charge.runtime_ms)} ${unit}` : "";
  return `${kind}${duration}${model ? ` · ${model}` : ""}`;
}
