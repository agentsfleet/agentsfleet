"use client";

import type { BillingCharge } from "@/lib/api/billing_report";
import type { BillingDetail } from "../lib/report-detail";
import { formatCount } from "@/lib/events/run-figures-format";
import { BILLING_DETAIL_STATE } from "../lib/report-detail";
import { PROVIDER_MODE } from "@/lib/types";
import { useTransition } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { Button, DataTable, EmptyState, PAGINATION_KIND, Time, UnavailableState, type DataTableColumn } from "@agentsfleet/design-system";
import { formatChargeAmount } from "../lib/charges";
import { DETAIL_PAGE_SIZE, nextDetailPage } from "../lib/report-query";
import { usageLabel } from "../lib/report-format";

type Props = { detail: BillingDetail; modelNames: Record<string, string> };

export function BillingActivityDetails({ detail, modelNames }: Props) {
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
  return <div className="space-y-lg">
    <DataTable columns={columns(modelNames)} rows={detail.charges.items} rowKey={row => row.id}
      caption="Individual settled charges" viewportClassName="max-h-none" isLoading={pending}
      empty={<EmptyState title="No matching settled charges" description="This activity has no settled lines for these filters." />}
      pagination={{ kind: PAGINATION_KIND.page, page: detail.page, pageSize: DETAIL_PAGE_SIZE,
        hasNext: detail.charges.next_cursor !== null, onPageChange: turnPage, isLoading: pending, "aria-label": "Charge pagination" }} />
    <p className="text-label text-muted-foreground">Each amount is the recorded combined charge. Separate historical model and runtime prices are unavailable.</p>
  </div>;
}

function columns(names: Record<string, string>): DataTableColumn<BillingCharge>[] {
  return [
    { key: "model", header: "Model", primary: true, cell: row => <div>
      <p>{names[row.model] ?? row.model}</p>
      <p className="text-label text-muted-foreground">{row.posture === PROVIDER_MODE.self_managed ? "Own provider key" : "Platform key"}</p>
    </div> },
    { key: "settled", header: "Settled", hideOnMobile: true, cell: row => <Time value={row.settled_at} format="absolute" /> },
    { key: "usage", header: "Recorded usage", hideOnMobile: true,
      cell: row => <div><p>{usageLabel(row.input_tokens, row.output_tokens, row.runtime_ms)}</p>
        {row.unpriced_tokens > 0 ? <p className="text-label text-muted-foreground">{formatCount(row.unpriced_tokens)} tokens awaiting a platform price</p> : null}</div> },
    { key: "amount", header: "Amount", numeric: true, cell: row => <span className="font-mono tabular-nums">{formatChargeAmount(row.charged_nanos)}</span> },
  ];
}
